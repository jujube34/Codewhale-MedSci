//! Owned NDJSON supervisor. The renderer cannot choose methods or process arguments.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, mpsc, oneshot},
};
type Reply = oneshot::Sender<Result<Value, String>>;
pub struct Agent {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, Reply>>>,
    next: AtomicU64,
}
impl Agent {
    pub async fn start(
        binary: &Path,
        cwd: &Path,
        home: &Path,
        config: &Path,
        key: &str,
        python_bin: Option<&Path>,
    ) -> Result<(Arc<Self>, mpsc::Receiver<(Value, oneshot::Sender<()>)>), String> {
        let mut cmd = Command::new(binary);
        cmd.args([
            "app-server",
            "--stdio",
            "--config",
            &config.to_string_lossy(),
        ])
        .current_dir(cwd)
        .env_clear()
        .env("CODEWHALE_HOME", home)
        .env("CODEWHALE_DESKTOP_SUPERVISED", "1")
        .env("DEEPSEEK_API_KEY", key)
        .env("CODEWHALE_TELEMETRY", "0")
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        for name in [
            "HOME",
            "USERPROFILE",
            "LOCALAPPDATA",
            "APPDATA",
            "SystemRoot",
            "WINDIR",
            "TEMP",
            "TMP",
            "TMPDIR",
            "LANG",
        ] {
            if let Some(v) = std::env::var_os(name) {
                cmd.env(name, v);
            }
        }
        let system_path = if cfg!(windows) {
            std::env::var("SystemRoot")
                .map(|v| format!("{v}\\System32;{v}"))
                .unwrap_or_default()
        } else {
            "/usr/bin:/bin:/usr/sbin:/sbin".into()
        };
        let path = python_bin
            .map(|p| {
                format!(
                    "{}{}{}",
                    p.display(),
                    if cfg!(windows) { ";" } else { ":" },
                    system_path
                )
            })
            .unwrap_or(system_path);
        cmd.env("PATH", path);
        #[cfg(windows)]
        {
            let git = binary
                .parent()
                .ok_or("安装包资源路径无效")?
                .join("git-bash");
            let bash = git.join("usr/bin/bash.exe");
            if !bash.is_file() || !git.join("cmd/git.exe").is_file() {
                return Err("安装包缺少内置 Git Bash，请重新安装完整版本".into());
            }
            let mut paths = vec![git.join("usr/bin")];
            if let Some(python) = python_bin {
                paths.push(python.to_path_buf());
            }
            paths.extend([git.join("mingw64/bin"), git.join("bin"), git.join("cmd")]);
            if let Some(windows) = std::env::var_os("SystemRoot") {
                let windows = std::path::PathBuf::from(windows);
                paths.extend([windows.join("System32"), windows]);
            }
            cmd.env(
                "PATH",
                std::env::join_paths(paths).map_err(|e| e.to_string())?,
            )
            .env("SHELL", &bash)
            .env("MSYSTEM", "MINGW64")
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8");
            if let Some(home) = std::env::var_os("USERPROFILE") {
                cmd.env("HOME", home);
            }
            // Exercise the same environment before advertising a usable Agent.
            let mut probe = Command::new(&bash);
            probe
                .env_clear()
                .current_dir(cwd)
                .kill_on_drop(true)
                .creation_flags(0x08000000);
            for (name, value) in cmd.as_std().get_envs() {
                if let Some(value) = value {
                    probe.env(name, value);
                }
            }
            probe.args(["--noprofile", "--norc", "-c",
                "test -n \"$BASH_VERSION\" && pwd >/dev/null && git --version >/dev/null && printf MEDSCI_BASH_OK"])
                .stdin(Stdio::null());
            let output = tokio::time::timeout(std::time::Duration::from_secs(30), probe.output())
                .await
                .map_err(|_| "内置 Git Bash 自检超时".to_string())?
                .map_err(|e| format!("无法启动内置 Git Bash：{e}"))?;
            if !output.status.success() || output.stdout != b"MEDSCI_BASH_OK" {
                return Err(format!(
                    "内置 Git Bash 自检失败：{}",
                    super::redact(&String::from_utf8_lossy(&output.stderr), key)
                ));
            }
        }
        if let Some(p) = python_bin.and_then(Path::parent) {
            cmd.env("VIRTUAL_ENV", p);
        }
        #[cfg(unix)]
        cmd.process_group(0);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let mut child = cmd
            .spawn()
            .map_err(|_| "Agent 启动失败，请检查安装包完整性".to_string())?;
        let stdin = child.stdin.take().ok_or("Agent stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("Agent stdout unavailable")?;
        let pending: Arc<Mutex<HashMap<u64, Reply>>> = Arc::default();
        let (tx, rx) = mpsc::channel(256);
        let waiters = pending.clone();
        let secret = key.to_owned();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.len() > 9 * 1024 * 1024 {
                    break;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(id) = v["id"].as_u64() {
                    if let Some(sender) = waiters.lock().await.remove(&id) {
                        let reply = if v.get("error").is_some() {
                            Err(super::redact(
                                &v["error"]["message"].as_str().unwrap_or("Agent 请求失败"),
                                &secret,
                            ))
                        } else {
                            Ok(v["result"].clone())
                        };
                        let _ = sender.send(reply);
                    }
                } else {
                    let (ack, done) = oneshot::channel();
                    if tx.send((v, ack)).await.is_err() || done.await.is_err() {
                        break;
                    }
                }
            }
            for (_, sender) in waiters.lock().await.drain() {
                let _ = sender.send(Err("Agent 已退出，可点击重启恢复".into()));
            }
            let (ack, _) = oneshot::channel();
            let _ = tx.send((json!({"type":"crashed"}), ack)).await;
        });
        Ok((
            Arc::new(Self {
                child: Mutex::new(child),
                stdin: Mutex::new(stdin),
                pending,
                next: AtomicU64::new(1),
            }),
            rx,
        ))
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.enqueue(method, params)
            .await?
            .await
            .map_err(|_| "Agent 通道已关闭".to_string())?
    }
    // Transport write only; native Runtime owns steer admission and scheduling.
    pub async fn enqueue(
        &self,
        method: &str,
        params: Value,
    ) -> Result<oneshot::Receiver<Result<Value, String>>, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let line =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .map_err(|e| e.to_string())?;
        let result = async {
            let mut writer = self.stdin.lock().await;
            writer.write_all(&line).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await
        }
        .await;
        if result.is_err() {
            self.pending.lock().await.remove(&id);
            return Err("Agent 通道已关闭".into());
        }
        Ok(rx)
    }
    pub async fn kill(&self) {
        // Give app-server time to drop its RuntimeBridge and reap its child.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.request("shutdown", json!({})),
        )
        .await;
        let mut child = self.child.lock().await;
        if tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
            .await
            .is_err()
        {
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                // This process group was created exclusively for this sidecar.
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}
