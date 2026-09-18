#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agent;
use agent::Agent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tauri::{Manager, State};
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    schema_version: u32,
    base_url: String,
    model: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-flash".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Message {
    #[serde(default)]
    sequence: u64,
    role: String,
    text: String,
}
#[derive(Clone, Serialize)]
struct Snapshot {
    workspace: Option<String>,
    session_id: String,
    metrics: Value,
    status: String,
    busy: bool,
    api_configured: bool,
    settings: Settings,
    messages: Vec<Message>,
    cards: Vec<Value>,
    error: Option<String>,
    pending_workspace: Option<String>,
    network: String,
    python: String,
}
struct Inner {
    view: Snapshot,
    agent: Option<Arc<Agent>>,
    thread: Option<String>,
    generation: u64,
    stop_requested: bool,
}
struct Desktop {
    inner: Arc<Mutex<Inner>>,
    home: PathBuf,
    resources: PathBuf,
    runtime_transaction: Mutex<()>,
}
fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.medsci.codewhale", "deepseek")
        .map_err(|_| "无法访问系统安全凭据库".into())
}
fn key() -> Result<String, String> {
    credential()?
        .get_password()
        .map_err(|_| "请先配置 DeepSeek API 密钥".into())
}
fn canonical(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if !p.is_absolute() || path.contains('\0') {
        return Err("请选择绝对文件夹路径".into());
    }
    let p = p
        .canonicalize()
        .map_err(|_| "文件夹不存在或无法访问".to_string())?;
    if !p.is_dir() {
        return Err("请选择文件夹，而非文件".into());
    }
    Ok(p)
}
fn official(base: &str) -> bool {
    matches!(
        base.trim_end_matches('/'),
        "https://api.deepseek.com"
            | "https://api.deepseek.com/v1"
            | "https://api.deepseek.com/beta"
    )
}
fn validate_settings(mut s: Settings) -> Result<Settings, String> {
    let u = url::Url::parse(&s.base_url).map_err(|_| "API 地址无效".to_string())?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err("API 地址必须使用 HTTPS，且不能包含凭据、查询或片段".into());
    }
    if s.model.trim().is_empty() || s.model.len() > 200 {
        return Err("模型名称无效".into());
    }
    if official(&s.base_url)
        && matches!(
            s.model.as_str(),
            "deepseek-v4-flash" | "deepseek-v4-flash-vision-exp"
        )
    {
        s.model = "deepseek-flash".into()
    }
    s.schema_version = 1;
    Ok(s)
}
fn redact(text: &str, secret: &str) -> String {
    let s = if secret.is_empty() {
        text.to_owned()
    } else {
        text.replace(secret, "[已脱敏]")
    };
    s.split_whitespace()
        .map(|w| {
            if w.contains("sk-")
                || w.to_ascii_lowercase().contains("authorization")
                || w.contains("base64,")
                || w.contains("://")
            {
                "[已脱敏]"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(1500)
        .collect()
}
// Launchpad supplies no workspace. Resolve the OS user home, never the
// process working directory; an invalid explicit path must not widen access.
fn startup_workspace(explicit: Option<&str>, user_home: &Path) -> Result<String, String> {
    let path = explicit
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.to_path_buf());
    canonical(&path.to_string_lossy()).map(|p| p.to_string_lossy().into_owned())
}
fn next_message_sequence(view: &Snapshot) -> u64 {
    view.messages
        .iter()
        .enumerate()
        .map(|(index, m)| m.sequence.max(index as u64 + 1))
        .chain(view.cards.iter().enumerate().map(|(index, c)| {
            c["sequence"]
                .as_u64()
                .unwrap_or(view.messages.len() as u64 + index as u64 + 1)
        }))
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}
// Runtime emits reasoning sequentially: start, deltas, completion before the next item.
fn append_stream(messages: &mut Vec<Message>, sequence: u64, role: &str, text: &str) {
    if let Some(message) = messages.last_mut() {
        if message.role == role && message.sequence + 1 == sequence {
            message.text.push_str(text);
            return;
        }
    }
    messages.push(Message {
        sequence,
        role: role.into(),
        text: text.into(),
    });
}
fn reasoning_event(messages: &mut Vec<Message>, sequence: u64, event: &Value) -> bool {
    let payload = &event["payload"];
    if payload["kind"] != "agent_reasoning" && payload["item"]["kind"] != "agent_reasoning" {
        return false;
    }
    match event["event"].as_str().unwrap_or("") {
        "item.started" => messages.push(Message {
            sequence,
            role: "reasoning".into(),
            text: payload["item"]["detail"].as_str().unwrap_or("").into(),
        }),
        "item.delta" => append_stream(
            messages,
            sequence,
            "reasoning",
            payload["delta"].as_str().unwrap_or(""),
        ),
        "item.completed" => {
            if let Some(text) = payload["item"]["detail"].as_str() {
                if let Some(message) = messages.iter_mut().rev().find(|m| m.role == "reasoning") {
                    message.text = text.into();
                } else {
                    append_stream(messages, sequence, "reasoning", text);
                }
            }
        }
        _ => {}
    }
    true
}
// This is a transient rendering projection, never a second session store.
fn apply_session(view: &mut Snapshot, session: &Value) -> Result<(), String> {
    let id = session["detail"]["thread"]["id"]
        .as_str()
        .ok_or("Codewhale 未返回会话详情")?;
    if session["runtime_id"] != id {
        return Err("Codewhale 会话 ID 不一致".into());
    }
    view.session_id = id.into();
    view.metrics = session["usage"].clone();
    view.messages.clear();
    view.cards.clear();
    for (index, item) in session["detail"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let sequence = index as u64 + 1;
        let role = match item["kind"].as_str().unwrap_or("") {
            "user_message" => Some("user"),
            "agent_message" => Some("assistant"),
            "agent_reasoning" => Some("reasoning"),
            _ => None,
        };
        if let Some(role) = role {
            view.messages.push(Message {
                sequence,
                role: role.into(),
                text: item["detail"]
                    .as_str()
                    .or(item["summary"].as_str())
                    .unwrap_or("")
                    .into(),
            });
        } else {
            let metadata = &item["metadata"];
            if let Some(name) = metadata["tool_name"].as_str() {
                view.cards.push(json!({"sequence":sequence,"event":"item.started","payload":{"item":item,"tool":{"id":metadata["tool_use_id"],"name":name,"input":metadata["tool_input"]}}}));
            }
        }
    }
    Ok(())
}
#[tauri::command]
async fn conversations(state: State<'_, Desktop>) -> Result<Value, String> {
    let (a, thread, _) = ensure_agent(&state).await?;
    a.request(
        "desktop/session",
        json!({"thread_id":thread,"operation":"list"}),
    )
    .await
}
#[tauri::command]
async fn select_conversation(state: State<'_, Desktop>, id: String) -> Result<(), String> {
    stop_internal(&state).await?;
    let (a, thread, generation) = ensure_agent(&state).await?;
    let session = a
        .request(
            "desktop/session",
            json!({"thread_id":thread,"runtime_id":id}),
        )
        .await?;
    let mut i = state.inner.lock().await;
    if i.generation != generation {
        return Err("会话已变更，请重新选择".into());
    }
    apply_session(&mut i.view, &session)
}
#[tauri::command]
async fn snapshot(state: State<'_, Desktop>) -> Result<Snapshot, String> {
    let initialize = {
        let i = state.inner.lock().await;
        i.agent.is_none()
            && i.view.session_id.is_empty()
            && i.view.api_configured
            && i.view.error.is_none()
    };
    if initialize {
        if let Err(error) = ensure_agent(&state).await {
            state.inner.lock().await.view.error = Some(error);
        }
    }
    Ok(state.inner.lock().await.view.clone())
}
async fn switch_workspace(state: &Desktop, path: String) -> Result<(), String> {
    let path = canonical(&path)?.to_string_lossy().into_owned();
    let mut i = state.inner.lock().await;
    if i.view.workspace.as_ref() == Some(&path) {
        return Ok(());
    }
    if i.view.busy {
        i.view.pending_workspace = Some(path);
        return Ok(());
    }
    if let Some(a) = i.agent.take() {
        a.kill().await
    }
    i.generation += 1;
    i.thread = None;
    i.view.session_id.clear();
    i.view.messages.clear();
    i.view.cards.clear();
    i.view.metrics = Value::Null;
    i.view.workspace = Some(path);
    i.view.error = None;
    i.view.status = "未启动".into();
    i.view.pending_workspace = None;
    Ok(())
}
#[tauri::command]
async fn open_folder(state: State<'_, Desktop>) -> Result<(), String> {
    if let Some(p) = rfd::AsyncFileDialog::new().pick_folder().await {
        switch_workspace(&state, p.path().to_string_lossy().into_owned()).await?;
    }
    Ok(())
}
#[tauri::command]
async fn resolve_switch(state: State<'_, Desktop>, allow: bool) -> Result<(), String> {
    let path = state.inner.lock().await.view.pending_workspace.take();
    if allow {
        stop_internal(&state).await?;
        if let Some(p) = path {
            switch_workspace(&state, p).await?
        }
    }
    Ok(())
}
#[tauri::command]
async fn save_api(
    state: State<'_, Desktop>,
    settings: Settings,
    secret: String,
) -> Result<(), String> {
    let settings = validate_settings(settings)?;
    let mut i = state.inner.lock().await;
    if i.view.busy {
        return Err("请先停止当前任务再修改 API 设置".into());
    }
    if !secret.is_empty() {
        if secret.trim().is_empty() {
            return Err("API 密钥不能为空白".into());
        }
        credential()?
            .set_password(secret.trim())
            .map_err(|_| "密钥保存失败".to_string())?;
    }
    let temp = state.home.join("settings.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temp, state.home.join("settings.json")).map_err(|e| e.to_string())?;
    if let Some(a) = i.agent.take() {
        a.kill().await
    }
    i.generation += 1;
    i.thread = None;
    i.view.settings = settings;
    i.view.api_configured = key().is_ok();
    i.view.status = "未启动".into();
    Ok(())
}
#[tauri::command]
async fn delete_api(state: State<'_, Desktop>) -> Result<(), String> {
    stop_internal(&state).await?;
    match credential()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err("删除系统凭据失败".into()),
    }
    state.inner.lock().await.view.api_configured = false;
    Ok(())
}
#[tauri::command]
async fn test_api(state: State<'_, Desktop>) -> Result<String, String> {
    let s = state.inner.lock().await.view.settings.clone();
    let secret = key().unwrap_or_default();
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| "网络初始化失败".to_string())?;
    let response = client
        .get(format!("{}/models", s.base_url.trim_end_matches('/')))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|_| "直连失败：请检查网络。当前构建尚未启用系统代理回退。".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "API 返回 HTTP {}，请检查密钥、地址或稍后重试",
            response.status().as_u16()
        ));
    }
    state.inner.lock().await.view.network = "直连".into();
    Ok("连接成功；模型视觉能力需通过实际图片任务验证".into())
}
async fn ensure_agent(state: &Desktop) -> Result<(Arc<Agent>, String, u64), String> {
    let mut i = state.inner.lock().await;
    if let (Some(a), Some(t)) = (&i.agent, &i.thread) {
        return Ok((a.clone(), t.clone(), i.generation));
    }
    let path = i.view.workspace.clone().ok_or("请先选择工作文件夹")?;
    let secret = key()?;
    let s = i.view.settings.clone();
    let config = state.home.join("agent.toml");
    // Serialize strings as JSON: these quoted strings are also valid TOML basic strings.
    let cfg = format!(
        "provider = \"deepseek\"\nmodel = {}\nbase_url = {}\napproval_policy = \"auto\"\nsandbox_mode = \"danger-full-access\"\nsandbox_network_access = true\ntelemetry = false\nlocale = \"zh-Hans\"\n[features]\nmcp = false\n",
        json!(s.model),
        json!(s.base_url)
    );
    std::fs::write(&config, cfg).map_err(|e| e.to_string())?;
    let binary = state.resources.join(if cfg!(windows) {
        "codewhale.exe"
    } else {
        "codewhale"
    });
    if !binary.is_file() {
        return Err("安装包缺少 Codewhale sidecar，请重新安装完整构建".into());
    }
    let shared = state
        .home
        .join("python/shared/current")
        .join(if cfg!(windows) { "Scripts" } else { "bin" });
    i.view.status = "启动中".into();
    let (a, mut events) = Agent::start(
        &binary,
        Path::new(&path),
        &state.home.join("agent"),
        &config,
        &secret,
        shared.is_dir().then_some(shared.as_path()),
    )
    .await?;
    let health = tokio::time::timeout(Duration::from_secs(20), a.request("healthz", json!({})))
        .await
        .map_err(|_| "Agent 握手超时".to_string())??;
    if health["version"] != "0.9.13" {
        a.kill().await;
        return Err("Agent 版本不兼容，需要 0.9.13".into());
    }
    let caps = a.request("capabilities", json!({})).await?;
    if !caps["methods"]
        .as_array()
        .is_some_and(|v| v.contains(&json!("desktop/approval")))
    {
        a.kill().await;
        return Err("Agent 缺少桌面审批协议".into());
    }
    let started = a
        .request(
            "thread/start",
            json!({"cwd":path,"model":s.model,"model_provider":"deepseek"}),
        )
        .await?;
    let thread = started["thread_id"]
        .as_str()
        .ok_or("Agent 未返回会话 ID")?
        .to_string();
    let session = a
        .request(
            "desktop/session",
            json!({"thread_id":thread,"runtime_id":if i.view.session_id.is_empty(){None}else{Some(&i.view.session_id)},"operation":if i.view.session_id.is_empty(){"latest"}else{"read"}}),
        )
        .await?;
    apply_session(&mut i.view, &session)?;
    i.generation += 1;
    let generation = i.generation;
    i.agent = Some(a.clone());
    i.thread = Some(thread.clone());
    i.view.status = "就绪".into();
    let inner = state.inner.clone();
    tokio::spawn(async move {
        while let Some((event, ack)) = events.recv().await {
            let mut i = inner.lock().await;
            if i.generation != generation {
                break;
            }
            match event["type"].as_str().unwrap_or("") {
                "response_delta" => {
                    let sequence = next_message_sequence(&i.view);
                    append_stream(
                        &mut i.view.messages,
                        sequence,
                        "assistant",
                        event["delta"].as_str().unwrap_or(""),
                    );
                }
                "runtime_event" => {
                    let sequence = next_message_sequence(&i.view);
                    if reasoning_event(&mut i.view.messages, sequence, &event) {
                        let _ = ack.send(());
                        continue;
                    }
                    if event["event"] == "turn.steered" {
                        let sequence = next_message_sequence(&i.view);
                        i.view.messages.push(Message {
                            sequence,
                            role: "user".into(),
                            text: event["payload"]["input"].as_str().unwrap_or("").into(),
                        });
                        i.view.status = "执行中 · 已接收补充指令".into();
                    }
                    let name = event["event"].as_str().unwrap_or("");
                    if matches!(name, "approval.decided" | "approval.timeout") {
                        let id = event["payload"]["approval_id"]
                            .as_str()
                            .or(event["payload"]["id"].as_str())
                            .unwrap_or("");
                        i.view.cards.retain(|c| {
                            !(c["event"] == "approval.required"
                                && (c["payload"]["approval_id"] == id || c["payload"]["id"] == id))
                        });
                    }
                    if matches!(name, "user_input.answered" | "user_input.canceled") {
                        let id = event["payload"]["input_id"]
                            .as_str()
                            .or(event["payload"]["id"].as_str())
                            .unwrap_or("");
                        i.view.cards.retain(|c| {
                            !(c["event"] == "user_input.required"
                                && (c["payload"]["input_id"] == id || c["payload"]["id"] == id))
                        });
                    }
                    if name.starts_with("approval.")
                        || name.starts_with("user_input.")
                        || name.starts_with("item.")
                    {
                        let mut card = event.clone();
                        card["sequence"] = json!(next_message_sequence(&i.view));
                        if let Some(p) = card["payload"].as_object_mut() {
                            p.remove("arguments");
                            p.remove("output");
                        }
                        i.view.cards.push(card);
                        if i.view.cards.len() > 100 {
                            i.view.cards.remove(0);
                        }
                    }
                }
                "crashed" => {
                    i.view.status = "已退出".into();
                    i.view.busy = false;
                    i.agent = None;
                    i.thread = None;
                }
                _ => {}
            }
            let _ = ack.send(());
        }
    });
    Ok((a, thread, generation))
}
#[tauri::command]
async fn send_message(
    state: State<'_, Desktop>,
    text: String,
    images: Vec<Value>,
) -> Result<(), String> {
    if text.trim().is_empty() && images.is_empty() {
        return Err("请输入任务".into());
    }
    if text.len() > 128 * 1024 || images.len() > 10 {
        return Err("任务或附件超过限制".into());
    }
    let mut total_bytes = 0usize;
    for image in &images {
        use base64::Engine;
        let mime = image["mime"].as_str().ok_or("图片格式缺失")?;
        let encoded = image["dataBase64"].as_str().ok_or("图片数据缺失")?;
        if encoded.len() > 6 * 1024 * 1024 {
            return Err("单图上限为 4 MB".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "图片编码无效".to_string())?;
        total_bytes += bytes.len();
        if bytes.len() > 4 * 1024 * 1024 || total_bytes > 5 * 1024 * 1024 {
            return Err("附件总计不能超过 5 MB".into());
        }
        let valid = match mime {
            "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
            "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            _ => false,
        };
        if !valid {
            return Err("图片内容与声明格式不符".into());
        }
    }
    {
        let mut i = state.inner.lock().await;
        if i.view.busy {
            if !images.is_empty() {
                return Err("Codewhale 当前 steer 接口仅支持文字，请在本轮完成后发送图片".into());
            }
            let agent = i
                .agent
                .clone()
                .ok_or("Agent 正在启动，请稍后发送补充指令")?;
            let thread = i.thread.clone().ok_or("尚无活动回合")?;
            let generation = i.generation;
            let receipt = agent
                .enqueue("thread/steer", json!({"thread_id":thread,"prompt":text}))
                .await?;
            i.view.status = "执行中 · 补充指令已发送".into();
            let inner = state.inner.clone();
            tokio::spawn(async move {
                let result = receipt
                    .await
                    .unwrap_or_else(|_| Err("Agent 通道已关闭".into()));
                if let Err(error) = result {
                    let mut i = inner.lock().await;
                    if i.generation == generation {
                        i.view.error = Some(format!("补充指令未被接收：{error}\n原指令：{text}"));
                    }
                }
            });
            return Ok(());
        }
        if !images.is_empty()
            && !(official(&i.view.settings.base_url) && i.view.settings.model == "deepseek-flash")
        {
            return Err("该精确路由的视觉能力未知，请选择官方 DeepSeek Flash".into());
        }
        i.view.busy = true;
        i.stop_requested = false;
        i.view.error = None;
    }
    if !state.home.join("python/shared/current").is_dir() {
        if let Err(e) = ensure_python(&state).await {
            state.inner.lock().await.view.busy = false;
            return Err(e);
        }
        // Reload the process environment after the offline Python environment exists.
        stop_internal(&state).await?;
        state.inner.lock().await.view.busy = true;
    }
    let (a, thread, generation) = match ensure_agent(&state).await {
        Ok(v) => v,
        Err(e) => {
            let mut i = state.inner.lock().await;
            i.view.busy = false;
            i.view.status = "启动失败".into();
            i.view.error = Some(e.clone());
            return Err(e);
        }
    };
    {
        let mut i = state.inner.lock().await;
        let sequence = next_message_sequence(&i.view);
        i.view.messages.push(Message {
            sequence,
            role: "user".into(),
            text: text.clone(),
        });
        i.view.status = "执行中".into();
    }
    let inner = state.inner.clone();
    tokio::spawn(async move {
        let result = a
            .request(
                "thread/message",
                json!({"thread_id":thread,"input":text,"images":images}),
            )
            .await;
        let metrics = a
            .request("desktop/session", json!({"thread_id":thread}))
            .await
            .ok();
        let mut i = inner.lock().await;
        if i.generation != generation {
            return;
        }
        i.view.busy = false;
        i.view.status = "就绪".into();
        if let Some(metrics) = metrics {
            if let Err(e) = apply_session(&mut i.view, &metrics) {
                i.view.error = Some(e);
            }
        }
        if let Err(e) = result {
            if i.stop_requested && e == "turn interrupted" {
                i.view.status = "已停止".into();
            } else {
                i.view.error = Some(e);
            }
        }
        i.stop_requested = false;
    });
    Ok(())
}
// Stop means interrupt the active turn, as in the TUI; keep its Engine/session alive.
async fn interrupt_current(state: &Desktop) -> Result<(), String> {
    let target = {
        let mut i = state.inner.lock().await;
        if !i.view.busy {
            return Ok(());
        }
        i.view.status = "停止中".into();
        i.stop_requested = true;
        i.agent.clone().zip(i.thread.clone())
    };
    if let Some((agent, thread)) = target {
        // The stdio response follows terminal Runtime events, so the native
        // tool-result receipts finish before another prompt can be admitted.
        agent
            .request("thread/interrupt", json!({"thread_id":thread}))
            .await?;
    }
    Ok(())
}
async fn stop_internal(state: &Desktop) -> Result<(), String> {
    interrupt_current(state).await?;
    let agent = state.inner.lock().await.agent.clone();
    if let Some(agent) = agent {
        agent.kill().await;
    }
    let mut i = state.inner.lock().await;
    i.generation += 1;
    i.agent = None;
    i.thread = None;
    i.view.busy = false;
    i.view.status = "未启动".into();
    i.view
        .cards
        .retain(|c| c["event"] != "approval.required" && c["event"] != "user_input.required");
    Ok(())
}
#[tauri::command]
async fn stop(state: State<'_, Desktop>) -> Result<(), String> {
    interrupt_current(&state).await
}
#[tauri::command]
async fn restart(state: State<'_, Desktop>) -> Result<(), String> {
    stop_internal(&state).await?;
    ensure_agent(&state).await?;
    Ok(())
}
#[tauri::command]
async fn new_session(state: State<'_, Desktop>) -> Result<(), String> {
    stop_internal(&state).await?;
    let (a, thread, generation) = ensure_agent(&state).await?;
    let session = a
        .request(
            "desktop/session",
            json!({"thread_id":thread,"operation":"new"}),
        )
        .await?;
    let mut i = state.inner.lock().await;
    if i.generation != generation {
        return Err("会话已变更".into());
    }
    apply_session(&mut i.view, &session)
}
#[tauri::command]
async fn decision(state: State<'_, Desktop>, id: String, allow: bool) -> Result<(), String> {
    let (a, t) = {
        let i = state.inner.lock().await;
        (
            i.agent.clone().ok_or("Agent 未启动")?,
            i.thread.clone().ok_or("无活动会话")?,
        )
    };
    // ACK follows the streaming turn; do not block the UI while it runs.
    let inner = state.inner.clone();
    tokio::spawn(async move {
        if let Err(e) = a
            .request(
                "desktop/approval",
                json!({"thread_id":t,"id":id,"decision":if allow{"allow"}else{"deny"}}),
            )
            .await
        {
            inner.lock().await.view.error = Some(e)
        }
    });
    Ok(())
}
#[tauri::command]
async fn answer_questions(
    state: State<'_, Desktop>,
    id: String,
    answers: Vec<Value>,
) -> Result<(), String> {
    let (agent, thread) = {
        let i = state.inner.lock().await;
        (
            i.agent.clone().ok_or("Agent 未启动")?,
            i.thread.clone().ok_or("无活动会话")?,
        )
    };
    let inner = state.inner.clone();
    tokio::spawn(async move {
        if let Err(e) = agent
            .request(
                "desktop/user-input",
                json!({"thread_id":thread,"id":id,"answers":answers}),
            )
            .await
        {
            inner.lock().await.view.error = Some(e)
        }
    });
    Ok(())
}
async fn ensure_python(state: &Desktop) -> Result<String, String> {
    let _guard = state
        .runtime_transaction
        .try_lock()
        .map_err(|_| "共享 Python 正在初始化，请稍候".to_string())?;
    {
        let mut i = state.inner.lock().await;
        i.view.python = "正在离线初始化…".into();
    }
    let runtime = state.resources.join("python");
    let python = runtime.join("cpython").join(if cfg!(windows) {
        "python.exe"
    } else {
        "bin/python3"
    });
    let mut command = tokio::process::Command::new(python);
    command
        .arg("-I")
        .arg("-X")
        .arg("utf8")
        .arg(runtime.join("manager.py"))
        .arg("--home")
        .arg(state.home.join("python/shared"))
        .arg("initialize")
        .env("PYTHONNOUSERSITE", "1");
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let result = command.output().await;
    let mut i = state.inner.lock().await;
    match result {
        Ok(output) if output.status.success() => {
            i.view.python = "已初始化".into();
            Ok("离线初始化完成；所有工作区共享此环境".into())
        }
        failure => {
            i.view.python = "初始化失败".into();
            let detail = match failure {
                Ok(output) => String::from_utf8_lossy(&output.stderr).into_owned(),
                Err(error) => error.to_string(),
            };
            let cause = detail
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("初始化进程未返回错误详情");
            let cause = redact(cause, &key().unwrap_or_default());
            Err(format!("共享环境初始化失败；原环境保持不变。{cause}"))
        }
    }
}
#[tauri::command]
async fn initialize_python(state: State<'_, Desktop>) -> Result<String, String> {
    if state.inner.lock().await.view.busy {
        return Err("请先停止 Agent，再更新共享环境".into());
    }
    ensure_python(&state).await
}
#[tauri::command]
fn company_login() -> Result<(), String> {
    Err("企业登录暂未启用，不影响使用个人 DeepSeek 密钥".into())
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            if let Some(p) = args.iter().skip(1).find(|v| !v.starts_with('-')).cloned() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<Desktop>();
                    if let Err(e) = switch_workspace(&state, p).await {
                        state.inner.lock().await.view.error = Some(e)
                    }
                });
            }
        }))
        .setup(|app| {
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                window.set_decorations(false)?;
            }
            let home = app
                .path()
                .app_local_data_dir()?
                .parent()
                .unwrap_or(&app.path().app_local_data_dir()?)
                .join("Codewhale-MedSci");
            std::fs::create_dir_all(&home)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))?;
            }
            let settings = std::fs::read(home.join("settings.json"))
                .ok()
                .and_then(|s| serde_json::from_slice(&s).ok())
                .and_then(|s| validate_settings(s).ok())
                .unwrap_or_default();
            let argument = std::env::args().skip(1).find(|a| !a.starts_with('-'));
            let (path, startup_error) =
                match startup_workspace(argument.as_deref(), &app.path().home_dir()?) {
                    Ok(path) => (Some(path), None),
                    Err(error) => (None, Some(error)),
                };
            let resources = app.path().resource_dir()?.join("resources");
            let view = Snapshot {
                workspace: path,
                session_id: String::new(),
                metrics: Value::Null,
                status: "未启动".into(),
                busy: false,
                api_configured: key().is_ok(),
                settings,
                messages: vec![],
                cards: vec![],
                error: startup_error,
                pending_workspace: None,
                network: "仅直连".into(),
                python: if home.join("python/shared/current").is_dir() {
                    "已初始化"
                } else {
                    "未初始化"
                }
                .into(),
            };
            app.manage(Desktop {
                inner: Arc::new(Mutex::new(Inner {
                    view,
                    agent: None,
                    thread: None,
                    generation: 0,
                    stop_requested: false,
                })),
                home,
                resources,
                runtime_transaction: Mutex::new(()),
            });
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
                let app_menu = SubmenuBuilder::new(app, "Codewhale-MedSci")
                    .hide_with_text("隐藏 Codewhale-MedSci")
                    .separator()
                    .quit_with_text("退出 Codewhale-MedSci")
                    .build()?;
                let open = MenuItemBuilder::with_id("open-folder", "打开工作文件夹…")
                    .accelerator("CmdOrCtrl+O")
                    .build(app)?;
                let new = MenuItemBuilder::with_id("new-session", "新建会话")
                    .accelerator("CmdOrCtrl+N")
                    .build(app)?;
                let file = SubmenuBuilder::new(app, "文件")
                    .item(&open)
                    .item(&new)
                    .build()?;
                let edit = SubmenuBuilder::new(app, "编辑")
                    .undo_with_text("撤销")
                    .redo_with_text("重做")
                    .separator()
                    .cut_with_text("剪切")
                    .copy_with_text("复制")
                    .paste_with_text("粘贴")
                    .select_all_with_text("全选")
                    .build()?;
                let tools = SubmenuBuilder::new(app, "工具")
                    .text("initialize-python", "初始化离线办公环境")
                    .text("restart-agent", "重启 Agent")
                    .build()?;
                app.set_menu(
                    MenuBuilder::new(app)
                        .items(&[&app_menu, &file, &edit, &tools])
                        .build()?,
                )?;
                app.on_menu_event(|app, event| {
                    let id = event.id().as_ref().to_owned();
                    if !matches!(
                        id.as_str(),
                        "open-folder" | "new-session" | "initialize-python" | "restart-agent"
                    ) {
                        return;
                    }
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<Desktop>();
                        let result = match id.as_str() {
                            "open-folder" => open_folder(state.clone()).await,
                            "new-session" => new_session(state.clone()).await,
                            "initialize-python" => {
                                initialize_python(state.clone()).await.map(|_| ())
                            }
                            _ => restart(state.clone()).await,
                        };
                        if let Err(error) = result {
                            state.inner.lock().await.view.error = Some(error);
                        }
                    });
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            conversations,
            select_conversation,
            open_folder,
            resolve_switch,
            save_api,
            delete_api,
            test_api,
            send_message,
            stop,
            restart,
            new_session,
            decision,
            initialize_python,
            answer_questions,
            company_login
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 Codewhale-MedSci")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let s = app.state::<Desktop>();
                tauri::async_runtime::block_on(async {
                    let _ = stop_internal(&s).await;
                });
            }
        });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_history_is_the_only_rendered_session() {
        let mut view = Snapshot {
            workspace: Some("/a".into()),
            session_id: "stale-gui-id".into(),
            metrics: Value::Null,
            status: "就绪".into(),
            busy: false,
            api_configured: false,
            settings: Settings::default(),
            messages: vec![Message {
                sequence: 1,
                role: "assistant".into(),
                text: "stale GUI copy".into(),
            }],
            cards: vec![],
            error: None,
            pending_workspace: None,
            network: String::new(),
            python: String::new(),
        };
        let session = json!({"runtime_id":"thr_native","usage":{"context":{"used_tokens":42}},"detail":{
        "thread":{"id":"thr_native"},"items":[
            {"kind":"user_message","detail":"继续任务"},
            {"kind":"agent_reasoning","detail":"检查已有上下文"},
            {"kind":"tool_call","status":"completed","metadata":{"tool_name":"File","tool_use_id":"call_1"}},
            {"kind":"agent_message","detail":"接续工作"}
        ]}});
        apply_session(&mut view, &session).unwrap();
        assert_eq!(view.session_id, "thr_native");
        assert_eq!(
            view.messages
                .iter()
                .map(|m| m.role.as_str())
                .collect::<Vec<_>>(),
            ["user", "reasoning", "assistant"]
        );
        assert_eq!(view.messages[2].text, "接续工作");
        assert_eq!(view.cards[0]["sequence"], 3);
        assert_eq!(view.messages[2].sequence, 4);
        assert_eq!(view.metrics["context"]["used_tokens"], 42);
        let before = view.session_id.clone();
        assert!(
            apply_session(
                &mut view,
                &json!({"runtime_id":"mismatch","detail":{"thread":{"id":"other"}}})
            )
            .is_err()
        );
        assert_eq!(view.session_id, before);
    }
    #[test]
    fn reasoning_stream_preserves_text_order_and_final_response() {
        let mut messages = vec![];
        assert!(reasoning_event(
            &mut messages,
            1,
            &json!({"event":"item.started","payload":{"item":{"kind":"agent_reasoning","detail":""}}})
        ));
        for text in ["先检查", "资料。"] {
            assert!(reasoning_event(
                &mut messages,
                2,
                &json!({"event":"item.delta","payload":{"kind":"agent_reasoning","delta":text}})
            ));
        }
        assert_eq!(messages[0].text, "先检查资料。");
        reasoning_event(
            &mut messages,
            2,
            &json!({"event":"item.completed","payload":{"item":{"kind":"agent_reasoning","detail":"先检查资料。"}}}),
        );
        append_stream(&mut messages, 2, "assistant", "最终");
        append_stream(&mut messages, 3, "assistant", "回复");
        // A tool occupies sequence 3: the next response must remain after it.
        append_stream(&mut messages, 4, "assistant", "工具之后");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].text, "最终回复");
        assert_eq!(messages[2].sequence, 4);
        let restored: Vec<Message> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        assert_eq!(restored[0].role, "reasoning");
        assert_eq!(restored[0].text, "先检查资料。");
    }
    #[test]
    fn launchpad_defaults_to_home_but_explicit_workspace_wins() {
        let home = std::env::temp_dir().canonicalize().unwrap();
        let project = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert_eq!(
            startup_workspace(None, &home).unwrap(),
            home.to_string_lossy()
        );
        assert_eq!(
            startup_workspace(Some(&project.to_string_lossy()), &home).unwrap(),
            project.to_string_lossy()
        );
        assert!(startup_workspace(Some("/does/not/exist"), &home).is_err());
        assert!(startup_workspace(Some("."), &home).is_err());
    }
    #[test]
    fn aliases_are_endpoint_scoped() {
        for base in ["https://api.deepseek.com", "https://proxy.example/v1"] {
            let s = validate_settings(Settings {
                base_url: base.into(),
                model: "deepseek-v4-flash".into(),
                ..Default::default()
            })
            .unwrap();
            assert_eq!(
                s.model,
                if official(base) {
                    "deepseek-flash"
                } else {
                    "deepseek-v4-flash"
                }
            );
        }
    }
    #[test]
    fn endpoints_reject_credential_leaks() {
        for url in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com?token=secret",
        ] {
            assert!(
                validate_settings(Settings {
                    base_url: url.into(),
                    ..Default::default()
                })
                .is_err()
            )
        }
    }
    #[test]
    fn errors_hide_secrets() {
        let s = redact(
            "oops sk-private https://user:pass@proxy text secret",
            "secret",
        );
        assert!(!s.contains("private"));
        assert!(!s.contains("pass"));
        assert!(!s.contains("secret"));
    }
    #[test]
    fn workspace_rejects_files_and_relative_paths() {
        assert!(canonical(".").is_err());
        assert!(canonical("/does/not/exist").is_err());
    }
}
