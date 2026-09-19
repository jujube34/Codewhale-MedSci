#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agent;
mod file_references;
use agent::Agent;
use codewhale_config::{ProviderKind, catalog::bundled_models_dev_catalog};
use file_references::{FileReference, clipboard_file_references, resolve_file_references};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::Write,
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
    provider: String,
    reasoning_effort: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 2,
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-flash".into(),
            provider: "deepseek".into(),
            reasoning_effort: "auto".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Message {
    #[serde(default)]
    sequence: u64,
    role: String,
    text: String,
    #[serde(default)]
    streaming: bool,
    #[serde(default)]
    duration_ms: Option<u64>,
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
    credentials_changed: bool,
    runtime: Option<RuntimeScope>,
}
struct RuntimeScope {
    path: PathBuf,
    _lease: File,
}
struct Desktop {
    inner: Arc<Mutex<Inner>>,
    home: PathBuf,
    resources: PathBuf,
    runtime_transaction: Mutex<()>,
    key_edits: Mutex<std::collections::HashMap<String, u64>>,
    instance: tempfile::TempDir,
    session_transition: Mutex<()>,
}
impl Desktop {
    fn begin_transition(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        self.session_transition
            .try_lock()
            .map_err(|_| "会话正在切换，请稍后重试".into())
    }
}
// Locks live outside ephemeral instance directories. Never unlink a lock file:
// another process may already hold an open handle to it.
fn open_lock(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| e.to_string())
}
fn claim_runtime(home: &Path, id: &str) -> Result<RuntimeScope, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("无效会话 ID".into());
    }
    let tasks = home.join("agent/tasks");
    let parent = if id == "legacy" {
        tasks.clone()
    } else {
        tasks.join("desktop")
    };
    let path = parent
        .join(if id == "legacy" { "runtime" } else { id })
        .canonicalize()
        .map_err(|_| "会话存储不存在".to_string())?;
    if !path.starts_with(parent.canonicalize().map_err(|e| e.to_string())?) {
        return Err("会话存储路径无效".into());
    }
    let file = open_lock(&path.join("desktop-owner.lock"))?;
    file.try_lock()
        .map_err(|_| "此会话已在另一个窗口打开，请先在该窗口切换会话或关闭窗口".to_string())?;
    Ok(RuntimeScope { path, _lease: file })
}
fn new_runtime(home: &Path) -> Result<RuntimeScope, String> {
    let root = home.join("agent/tasks/desktop");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    // Persistent native history, deliberately separate from instance scratch.
    let path = tempfile::Builder::new()
        .prefix("session-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?
        .keep();
    claim_runtime(
        home,
        path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("会话路径无效")?,
    )
}
fn save_settings(home: &Path, settings: &Settings) -> Result<(), String> {
    let lock = open_lock(&home.join("settings.lock"))?;
    lock.try_lock()
        .map_err(|_| "另一个窗口正在保存设置，请重试".to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(home).map_err(|e| e.to_string())?;
    temp.write_all(&serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    // persist replaces atomically on Windows as well as Unix. Last successful
    // save becomes the default for future windows; existing snapshots stay put.
    temp.persist(home.join("settings.json"))
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn credential(provider: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.medsci.codewhale", provider)
        .map_err(|_| "无法访问系统安全凭据库".into())
}
fn key(provider: &str) -> Result<String, String> {
    credential(provider)?
        .get_password()
        .map_err(|_| "请先配置当前服务商的 API 密钥".into())
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
fn api_providers() -> Vec<ProviderKind> {
    use codewhale_config::provider::CredentialAcquisition;
    ProviderKind::all()
        .iter()
        .copied()
        .filter(|kind| {
            let p = kind.provider();
            matches!(
                p.credential_help().acquisition,
                CredentialAcquisition::ApiKey | CredentialAcquisition::ApiKeyOrOAuth
            ) && p.default_base_url().starts_with("https://")
                && !p.env_vars().is_empty()
        })
        .collect()
}

// Read the same provider-scoped catalog as the TUI. Unknown/custom routes keep
// automatic control rather than inheriting another endpoint's capabilities.
fn effort_options(provider: ProviderKind, base: &str, model: &str) -> Vec<&'static str> {
    let mut efforts = vec!["auto"];
    let native = provider.provider();
    let base = base.trim().trim_end_matches('/');
    let same_endpoint = base == native.default_base_url().trim_end_matches('/')
        || (provider == ProviderKind::Deepseek && official(base))
        || (provider == ProviderKind::Moonshot
            && matches!(
                base,
                "https://api.moonshot.cn/v1" | "https://api.moonshot.ai/v1"
            ))
        || (provider == ProviderKind::Zai
            && matches!(
                base,
                "https://api.z.ai/api/paas/v4"
                    | "https://api.z.ai/api/coding/paas/v4"
                    | "https://open.bigmodel.cn/api/paas/v4"
                    | "https://open.bigmodel.cn/api/coding/paas/v4"
            ));
    if !same_endpoint {
        return efforts;
    }
    let catalog = bundled_models_dev_catalog();
    let Some(row) = catalog.provider_model(provider.as_str(), model) else {
        return efforts;
    };
    if row.reasoning != Some(true) {
        return efforts;
    }
    // Native Z.ai request shaping overrides the older bundled tier metadata.
    // See TUI config::is_exact_zai_forced_thinking_route: 5.3 rejects off.
    if provider == ProviderKind::Zai {
        match model.to_ascii_lowercase().as_str() {
            "glm-5.3" | "glm-5.3-flash" => return vec!["auto", "high", "max"],
            "glm-5.2" => return vec!["auto", "off", "high", "max"],
            "glm-5-turbo" => return vec!["auto", "off", "high"],
            _ => {}
        }
    }
    for option in &row.reasoning_options {
        if option["type"] != "effort" && option["type"] != "thinking" {
            continue;
        }
        if let Some(values) = option["values"].as_array() {
            for raw in values.iter().filter_map(Value::as_str) {
                let value = match raw {
                    "off" | "none" | "disabled" => "off",
                    "enabled" => "high",
                    "minimal" => "minimal",
                    "low" => "low",
                    "medium" => "medium",
                    "high" => "high",
                    "xhigh" => "xhigh",
                    "max" => "max",
                    "ultra" => "ultra",
                    _ => continue,
                };
                if !efforts.contains(&value) {
                    efforts.push(value);
                }
            }
        }
    }
    if efforts.len() == 1 {
        match provider {
            ProviderKind::Deepseek => efforts.extend(["off", "low", "high", "max"]),
            ProviderKind::Moonshot if model == "kimi-k3" => efforts.extend(["low", "high", "max"]),
            ProviderKind::Moonshot => efforts.extend(["off", "high"]),
            _ => {}
        }
    }
    efforts
}

#[tauri::command]
fn api_catalog() -> Value {
    let catalog = bundled_models_dev_catalog();
    json!(api_providers().into_iter().map(|kind| {
        let p = kind.provider();
        let mut models: Vec<Value> = catalog.provider(kind.as_str()).map(|row| row.models.iter()
            .filter(|(_, model)| model.supports_text_chat() && model.tool_call != Some(false))
            .map(|(id, model)| json!({"id":id,"name":model.name.as_deref().unwrap_or(id),"efforts":effort_options(kind,p.default_base_url(),id)}))
            .collect()).unwrap_or_default();
        if !models.iter().any(|m| m["id"] == p.default_model()) {
            models.insert(0, json!({"id":p.default_model(),"name":p.default_model(),"efforts":["auto"]}));
        }
        json!({"id":kind.as_str(),"name":p.display_name(),"base_url":p.default_base_url(),"model":p.default_model(),"models":models})
    }).collect::<Vec<_>>())
}

#[tauri::command]
fn api_efforts(provider: String, base_url: String, model: String) -> Vec<&'static str> {
    ProviderKind::parse(&provider)
        .map(|p| effort_options(p, &base_url, &model))
        .unwrap_or_else(|| vec!["auto"])
}

fn validate_settings(mut s: Settings) -> Result<Settings, String> {
    let provider = ProviderKind::parse(&s.provider)
        .filter(|p| api_providers().contains(p))
        .ok_or("不支持的 API 服务商")?;
    s.provider = provider.as_str().into();
    s.base_url = s.base_url.trim().to_string();
    s.model = s.model.trim().to_string();
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
    if provider == ProviderKind::Deepseek
        && official(&s.base_url)
        && matches!(
            s.model.as_str(),
            "deepseek-v4-flash" | "deepseek-v4-flash-vision-exp"
        )
    {
        s.model = "deepseek-flash".into()
    }
    if !effort_options(provider, &s.base_url, &s.model).contains(&s.reasoning_effort.as_str()) {
        return Err("当前模型或 API 地址不支持所选思考强度，请重新选择".into());
    }
    s.schema_version = 2;
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
fn startup_workspace(explicit: Option<&str>, launch_dir: &Path) -> Result<String, String> {
    let path = explicit
        .map(PathBuf::from)
        .unwrap_or_else(|| launch_dir.to_path_buf());
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
fn item_duration_ms(item: &Value) -> Option<u64> {
    let started = chrono::DateTime::parse_from_rfc3339(item["started_at"].as_str()?).ok()?;
    let ended = chrono::DateTime::parse_from_rfc3339(item["ended_at"].as_str()?).ok()?;
    u64::try_from((ended - started).num_milliseconds()).ok()
}
fn item_card_id(card: &Value) -> Option<&str> {
    card["item_id"]
        .as_str()
        .or(card.pointer("/payload/item/id").and_then(Value::as_str))
        .or(card
            .pointer("/payload/item/metadata/tool_use_id")
            .and_then(Value::as_str))
        .or(card.pointer("/payload/tool/id").and_then(Value::as_str))
}
fn is_timeline_item(card: &Value) -> bool {
    matches!(
        card.pointer("/payload/item/kind").and_then(Value::as_str),
        Some(
            "tool_call"
                | "command_execution"
                | "file_change"
                | "context_compaction"
                | "status"
                | "error"
        )
    ) || card.pointer("/payload/tool").is_some_and(Value::is_object)
}
fn record_timeline_card(view: &mut Snapshot, event: &Value) {
    let name = event["event"].as_str().unwrap_or("");
    if matches!(name, "approval.required" | "user_input.required") {
        let mut card = event.clone();
        card["sequence"] = json!(next_message_sequence(view));
        view.cards.push(card);
    } else if matches!(
        name,
        "item.started" | "item.completed" | "item.failed" | "item.interrupted" | "item.canceled"
    ) && is_timeline_item(event)
    {
        let existing = item_card_id(event).and_then(|id| {
            view.cards
                .iter()
                .position(|card| item_card_id(card) == Some(id))
        });
        let sequence = existing
            .and_then(|index| view.cards[index]["sequence"].as_u64())
            .unwrap_or_else(|| next_message_sequence(view));
        let previous_tool = existing.and_then(|index| {
            view.cards[index]
                .pointer("/payload/tool")
                .filter(|tool| tool.is_object())
                .cloned()
        });
        let mut card = event.clone();
        card["sequence"] = json!(sequence);
        if let Some(payload) = card["payload"].as_object_mut() {
            payload.remove("arguments");
            payload.remove("output");
            if !payload.get("tool").is_some_and(Value::is_object)
                && let Some(tool) = previous_tool
            {
                payload.insert("tool".into(), tool);
            }
        }
        if let Some(index) = existing {
            view.cards[index] = card;
        } else {
            view.cards.push(card);
        }
    }
    if view.cards.len() > 100 {
        view.cards.remove(0);
    }
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
        streaming: role == "reasoning",
        duration_ms: None,
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
            streaming: true,
            duration_ms: None,
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
                    message.streaming = false;
                    message.duration_ms = item_duration_ms(&payload["item"]);
                } else {
                    append_stream(messages, sequence, "reasoning", text);
                    if let Some(message) = messages.last_mut() {
                        message.streaming = false;
                        message.duration_ms = item_duration_ms(&payload["item"]);
                    }
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
                streaming: item["status"] == "in_progress",
                duration_ms: item_duration_ms(item),
            });
        } else if matches!(
            item["kind"].as_str(),
            Some(
                "tool_call"
                    | "command_execution"
                    | "file_change"
                    | "context_compaction"
                    | "status"
                    | "error"
            )
        ) {
            let event = match item["status"].as_str().unwrap_or("") {
                "failed" => "item.failed",
                "interrupted" | "canceled" => "item.interrupted",
                "in_progress" | "queued" => "item.started",
                _ => "item.completed",
            };
            view.cards.push(
                json!({"sequence":sequence,"event":event,"item_id":item["id"],"payload":{"item":item}}),
            );
        }
    }
    Ok(())
}
#[tauri::command]
async fn conversations(state: State<'_, Desktop>) -> Result<Value, String> {
    let _transition = state.begin_transition()?;
    list_conversations(&state).await
}
async fn list_conversations(state: &Desktop) -> Result<Value, String> {
    let (a, thread, _) = ensure_agent(state).await?;
    a.request(
        "desktop/session",
        json!({"thread_id":thread,"operation":"list"}),
    )
    .await
}
#[tauri::command]
async fn select_conversation(
    state: State<'_, Desktop>,
    id: String,
    store: String,
) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    if state.inner.lock().await.view.session_id == id {
        return Ok(());
    }
    // Only native history entries for this workspace may select a store.
    let records = list_conversations(&state).await?;
    if !records
        .as_array()
        .is_some_and(|rows| rows.iter().any(|r| r["id"] == id && r["store"] == store))
    {
        return Err("此工作目录中找不到该会话".into());
    }
    let same_store = state.inner.lock().await.runtime.as_ref().is_some_and(|r| {
        r.path.file_name().and_then(|n| n.to_str())
            == Some(if store == "legacy" { "runtime" } else { &store })
    });
    // Claim before stopping this window's task or mutating Runtime metadata.
    let target = if same_store {
        None
    } else {
        Some(claim_runtime(&state.home, &store)?)
    };
    stop_internal(&state).await?;
    let (previous_view, previous_runtime) = {
        let mut i = state.inner.lock().await;
        let previous_view = i.view.clone();
        let previous_runtime = target.map(|scope| i.runtime.replace(scope));
        i.view.session_id = id;
        (previous_view, previous_runtime)
    };
    if let Err(error) = ensure_agent(&state).await {
        let mut i = state.inner.lock().await;
        i.view = previous_view;
        if let Some(previous) = previous_runtime {
            i.runtime = previous;
        }
        return Err(error);
    }
    Ok(())
}
#[tauri::command]
async fn snapshot(app: tauri::AppHandle, state: State<'_, Desktop>) -> Result<Snapshot, String> {
    let view = state.inner.lock().await.view.clone();
    if let Some(window) = app.get_webview_window("main") {
        let title = format!(
            "{} · {} · Codewhale-MedSci [{}]",
            view.workspace.as_deref().unwrap_or("未选择工作目录"),
            if view.session_id.is_empty() {
                "新会话"
            } else {
                &view.session_id
            },
            std::process::id()
        );
        if window.title().ok().as_deref() != Some(&title) {
            let _ = window.set_title(&title);
        }
    }
    Ok(view)
}
async fn switch_workspace(state: &Desktop, path: String) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    switch_workspace_inner(state, path).await
}
async fn switch_workspace_inner(state: &Desktop, path: String) -> Result<(), String> {
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
    i.runtime = None;
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
    let workspace = state.inner.lock().await.view.workspace.clone();
    let mut dialog = rfd::AsyncFileDialog::new().set_title("选择工作目录");
    if let Some(path) = workspace {
        dialog = dialog.set_directory(path);
    }
    if let Some(p) = dialog.pick_folder().await {
        switch_workspace(&state, p.path().to_string_lossy().into_owned()).await?;
    }
    Ok(())
}
#[tauri::command]
async fn open_file(state: State<'_, Desktop>, path: String) -> Result<(), String> {
    let workspace = state
        .inner
        .lock()
        .await
        .view
        .workspace
        .clone()
        .ok_or("未选择工作区")?;
    let root = PathBuf::from(workspace)
        .canonicalize()
        .map_err(|_| "工作区无法访问".to_string())?;
    let requested = PathBuf::from(path);
    let resolved = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    }
    .canonicalize()
    .map_err(|_| "文件不存在或无法访问".to_string())?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err("只能打开当前工作区内的文件".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(&resolved)
        .spawn()
        .map_err(|error| format!("无法打开文件：{error}"))?;
    Ok(())
}
#[tauri::command]
async fn resolve_switch(state: State<'_, Desktop>, allow: bool) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    let path = state.inner.lock().await.view.pending_workspace.take();
    if allow {
        stop_internal(&state).await?;
        if let Some(p) = path {
            switch_workspace_inner(&state, p).await?
        }
    }
    Ok(())
}
#[tauri::command]
fn api_key_status(provider: String) -> Result<bool, String> {
    let provider = ProviderKind::parse(&provider)
        .filter(|p| api_providers().contains(p))
        .ok_or("不支持的 API 服务商")?;
    match credential(provider.as_str())?.get_password() {
        Ok(value) => Ok(!value.is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(_) => Err("无法读取系统凭据库中的密钥状态".into()),
    }
}

fn validate_api_secret(secret: &str) -> Result<&str, String> {
    let secret = secret.trim();
    if secret.is_empty() || secret.chars().all(|c| matches!(c, '*' | '•' | '●')) {
        return Err("请输入有效的 API 密钥".into());
    }
    Ok(secret)
}

#[tauri::command]
async fn save_api_key(
    state: State<'_, Desktop>,
    provider: String,
    secret: String,
) -> Result<bool, String> {
    let provider = ProviderKind::parse(&provider)
        .filter(|p| api_providers().contains(p))
        .ok_or("不支持的 API 服务商")?;
    let provider = provider.as_str();
    // Debounce per provider in the host so closing the dialog or changing
    // providers cannot cancel a submitted edit or write it to another slot.
    let revision = {
        let mut edits = state.key_edits.lock().await;
        let revision = edits.entry(provider.into()).or_default();
        *revision += 1;
        *revision
    };
    if secret.is_empty() {
        return Ok(false);
    } // Clear cancels pending input; keeps the stored key.
    let secret = validate_api_secret(&secret)?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let edits = state.key_edits.lock().await;
    if edits.get(provider) != Some(&revision) {
        return Ok(false);
    }
    let mut i = state.inner.lock().await;
    credential(provider)?
        .set_password(secret)
        .map_err(|_| "密钥自动保存失败".to_string())?;
    if i.view.settings.provider == provider {
        i.view.api_configured = true;
        i.credentials_changed = true;
    }
    Ok(true)
}

#[tauri::command]
async fn save_api(
    state: State<'_, Desktop>,
    settings: Settings,
    secret: String,
) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    let settings = validate_settings(settings)?;
    let mut i = state.inner.lock().await;
    if i.view.busy {
        return Err("请先停止当前任务再修改 API 设置".into());
    }
    if !secret.is_empty() {
        credential(&settings.provider)?
            .set_password(validate_api_secret(&secret)?)
            .map_err(|_| "密钥保存失败".to_string())?;
    }
    save_settings(&state.home, &settings)?;
    if let Some(a) = i.agent.take() {
        a.kill().await
    }
    i.generation += 1;
    i.thread = None;
    i.credentials_changed = false;
    i.view.settings = settings;
    i.view.api_configured = key(&i.view.settings.provider).is_ok();
    i.view.status = "未启动".into();
    Ok(())
}
#[tauri::command]
async fn delete_api(state: State<'_, Desktop>, provider: String) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    let provider = ProviderKind::parse(&provider)
        .filter(|p| api_providers().contains(p))
        .ok_or("不支持的 API 服务商")?;
    stop_internal(&state).await?;
    match credential(provider.as_str())?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err("删除系统凭据失败".into()),
    }
    let mut i = state.inner.lock().await;
    i.view.api_configured = key(&i.view.settings.provider).is_ok();
    Ok(())
}
#[tauri::command]
async fn test_api(state: State<'_, Desktop>) -> Result<String, String> {
    let s = state.inner.lock().await.view.settings.clone();
    let secret = key(&s.provider)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| "网络初始化失败".to_string())?;
    let provider = ProviderKind::parse(&s.provider).ok_or("不支持的服务商")?;
    let anthropic = provider.provider().wire_policy().fixed()
        == Some(codewhale_config::provider::WireFormat::AnthropicMessages);
    let base = s.base_url.trim_end_matches('/');
    let endpoint = if anthropic && !base.ends_with("/v1") {
        format!("{base}/v1/models")
    } else {
        format!("{base}/models")
    };
    let request = client.get(endpoint);
    let request = if anthropic {
        request
            .header("x-api-key", &secret)
            .header("anthropic-version", "2023-06-01")
    } else {
        request.bearer_auth(&secret)
    };
    let response = request
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
    Ok("模型列表接口连接成功；所选模型的调用权限需通过实际任务验证".into())
}
async fn refresh_api_credentials(i: &mut Inner) {
    if i.credentials_changed {
        if let Some(agent) = i.agent.take() {
            agent.kill().await;
        }
        i.thread = None;
        i.generation += 1;
        i.credentials_changed = false;
    }
}
async fn ensure_agent(state: &Desktop) -> Result<(Arc<Agent>, String, u64), String> {
    let mut i = state.inner.lock().await;
    if !i.view.busy {
        refresh_api_credentials(&mut i).await;
    }
    if let (Some(a), Some(t)) = (&i.agent, &i.thread) {
        return Ok((a.clone(), t.clone(), i.generation));
    }
    let path = i.view.workspace.clone().ok_or("请先选择工作文件夹")?;
    let s = i.view.settings.clone();
    let secret = key(&s.provider)?;
    let config = state.instance.path().join("agent.toml");
    if i.runtime.is_none() {
        i.runtime = Some(new_runtime(&state.home)?);
    }
    // Serialize strings as JSON: these quoted strings are also valid TOML basic strings.
    let cfg = format!(
        "provider = {}\nmodel = {}\nbase_url = {}\nreasoning_effort = {}\napproval_policy = \"auto\"\nsandbox_mode = \"danger-full-access\"\nsandbox_network_access = true\ntelemetry = false\nlocale = \"zh-Hans\"\n[features]\nmcp = false\n",
        json!(s.provider),
        json!(s.model),
        json!(s.base_url),
        json!(s.reasoning_effort)
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
    let python_lease = open_lock(&state.home.join("python/shared/usage.lock"))?;
    python_lease
        .try_lock_shared()
        .map_err(|_| "共享 Python 正在更新，请稍后重试".to_string())?;
    let python_lease = shared.is_dir().then_some(python_lease);
    i.view.status = "启动中".into();
    let (a, mut events) = Agent::start(
        &binary,
        Path::new(&path),
        &state.home.join("agent"),
        &config,
        &secret,
        ProviderKind::parse(&s.provider)
            .ok_or("不支持的服务商")?
            .provider()
            .env_vars()[0],
        shared.is_dir().then_some(shared.as_path()),
        python_lease,
        &i.runtime.as_ref().ok_or("会话存储未初始化")?.path,
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
    if caps["desktop_multi_instance"] != true {
        a.kill().await;
        return Err("Agent 不支持独立会话存储，请更新完整安装包".into());
    }
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
            json!({"cwd":path,"model":s.model,"model_provider":s.provider}),
        )
        .await?;
    let thread = started["thread_id"]
        .as_str()
        .ok_or("Agent 未返回会话 ID")?
        .to_string();
    let session = a
        .request(
            "desktop/session",
            json!({"thread_id":thread,"runtime_id":if i.view.session_id.is_empty(){None}else{Some(&i.view.session_id)},"operation":if i.view.session_id.is_empty(){"new"}else{"read"}}),
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
                            streaming: false,
                            duration_ms: None,
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
                    record_timeline_card(&mut i.view, &event);
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
    references: Vec<FileReference>,
) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    let text = file_references::prepend(text, references)?;
    if text.trim().is_empty() {
        return Err("请输入任务".into());
    }
    {
        let mut i = state.inner.lock().await;
        if i.view.busy {
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
        refresh_api_credentials(&mut i).await;
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
            streaming: false,
            duration_ms: None,
        });
        i.view.status = "执行中".into();
    }
    let inner = state.inner.clone();
    tokio::spawn(async move {
        let result = a
            .request("thread/message", json!({"thread_id":thread,"input":text}))
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
    let _transition = state.begin_transition()?;
    stop_internal(&state).await?;
    ensure_agent(&state).await?;
    Ok(())
}
#[tauri::command]
async fn new_session(state: State<'_, Desktop>) -> Result<(), String> {
    let _transition = state.begin_transition()?;
    stop_internal(&state).await?;
    {
        let mut i = state.inner.lock().await;
        i.runtime = None;
        i.view.session_id.clear();
        i.view.messages.clear();
        i.view.cards.clear();
        i.view.metrics = Value::Null;
    }
    ensure_agent(&state).await?;
    Ok(())
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
            let cause = redact(cause, &key(&i.view.settings.provider).unwrap_or_default());
            Err(format!("共享环境初始化失败；原环境保持不变。{cause}"))
        }
    }
}
#[tauri::command]
async fn initialize_python(state: State<'_, Desktop>) -> Result<String, String> {
    let _transition = state.begin_transition()?;
    if state.inner.lock().await.view.busy {
        return Err("请先停止 Agent，再更新共享环境".into());
    }
    // Release this window's environment lease. Other windows remain protected.
    stop_internal(&state).await?;
    ensure_python(&state).await
}
#[tauri::command]
fn company_login() -> Result<(), String> {
    Err("企业登录暂未启用，不影响使用个人 DeepSeek 密钥".into())
}
fn main() {
    tauri::Builder::default()
        .setup(|app| {
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
            let launch_dir = std::env::current_dir().unwrap_or(app.path().home_dir()?);
            let (path, startup_error) = match startup_workspace(argument.as_deref(), &launch_dir) {
                Ok(path) => (Some(path), None),
                Err(error) => (None, Some(error)),
            };
            let resources = app.path().resource_dir()?.join("resources");
            let instances = home.join("instances");
            std::fs::create_dir_all(&instances)?;
            let instance = tempfile::Builder::new()
                .prefix("window-")
                .tempdir_in(instances)?;
            // Build from the merged platform configuration to preserve Windows
            // custom chrome. Browser data and caches belong to this instance.
            let webview_data = instance.path().join("webview");
            let view = Snapshot {
                workspace: path,
                session_id: String::new(),
                metrics: Value::Null,
                status: "未启动".into(),
                busy: false,
                api_configured: key(&settings.provider).is_ok(),
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
                    credentials_changed: false,
                    runtime: None,
                })),
                home,
                resources,
                runtime_transaction: Mutex::new(()),
                key_edits: Mutex::new(std::collections::HashMap::new()),
                instance,
                session_transition: Mutex::new(()),
            });
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .data_directory(webview_data)
                .incognito(true)
                .build()?;
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
            open_file,
            resolve_switch,
            api_catalog,
            api_key_status,
            save_api_key,
            api_efforts,
            save_api,
            delete_api,
            test_api,
            send_message,
            clipboard_file_references,
            resolve_file_references,
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
    #[cfg(windows)]
    #[test]
    fn multi_instance_python_update_guard() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let python = repository.join("apps/desktop/src-tauri/resources/python/cpython/python.exe");
        let manager = repository.join("desktop/runtime/manager.py");
        let home = tempfile::tempdir().unwrap();
        let first = open_lock(&home.path().join("usage.lock")).unwrap();
        first.try_lock_shared().unwrap();
        let second = open_lock(&home.path().join("usage.lock")).unwrap();
        second.try_lock_shared().unwrap();
        let probe = |blocked: bool| {
            let output = std::process::Command::new(&python).args(["-I", "-X", "utf8", "-c",
                "import importlib.util,pathlib,sys\ns=importlib.util.spec_from_file_location('manager',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)\ndef reached(home): raise RuntimeError('RECOVERY_ENTERED')\nm.recover=reached\ntry: m.transact(pathlib.Path(sys.argv[2]))\nexcept RuntimeError as e: assert ('RECOVERY_ENTERED' not in str(e)) == (sys.argv[3]=='true'), str(e)\nelse: raise AssertionError('missing probe')"])
                .arg(&manager).arg(home.path()).arg(blocked.to_string()).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        probe(true);
        drop(first);
        probe(true);
        drop(second);
        probe(false);
    }
    #[test]
    fn multi_instance_child() {
        let Some(home) = std::env::var_os("MEDSCI_TEST_HOME") else {
            return;
        };
        let home = PathBuf::from(home);
        if let Ok(scope) = std::env::var("MEDSCI_TEST_SCOPE") {
            assert!(
                claim_runtime(&home, &scope).is_err(),
                "another process owns this session"
            );
        } else {
            assert!(
                save_settings(&home, &Settings::default()).is_err(),
                "another process is saving settings"
            );
        }
    }
    #[test]
    fn multi_instance_storage_and_settings() {
        let home = tempfile::tempdir().unwrap();
        let first = new_runtime(home.path()).unwrap();
        let second = new_runtime(home.path()).unwrap();
        assert_ne!(first.path, second.path);
        let scope = first.path.file_name().unwrap().to_str().unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::multi_instance_child"])
            .env("MEDSCI_TEST_HOME", home.path())
            .env("MEDSCI_TEST_SCOPE", scope)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        let saved_path = first.path.clone();
        std::fs::write(saved_path.join("history-receipt"), "keep").unwrap();
        let scope = scope.to_string();
        drop(first);
        assert!(claim_runtime(home.path(), &scope).is_ok());
        assert_eq!(
            std::fs::read_to_string(saved_path.join("history-receipt")).unwrap(),
            "keep"
        );
        assert!(claim_runtime(home.path(), "../escape").is_err());
        save_settings(home.path(), &Settings::default()).unwrap();
        let lock = open_lock(&home.path().join("settings.lock")).unwrap();
        lock.try_lock().unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::multi_instance_child"])
            .env("MEDSCI_TEST_HOME", home.path())
            .env_remove("MEDSCI_TEST_SCOPE")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        drop(lock);
        let changed = Settings {
            model: "another-model".into(),
            ..Settings::default()
        };
        save_settings(home.path(), &changed).unwrap();
        let restored: Settings =
            serde_json::from_slice(&std::fs::read(home.path().join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(restored.model, "another-model");
    }
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
                streaming: false,
                duration_ms: None,
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
        assert!(!restored[0].streaming);
    }
    #[test]
    fn reasoning_duration_uses_runtime_item_timestamps() {
        let item = json!({
            "started_at":"2026-09-19T08:00:00.100Z",
            "ended_at":"2026-09-19T08:00:08.650Z"
        });
        assert_eq!(item_duration_ms(&item), Some(8_550));
        assert_eq!(item_duration_ms(&json!({})), None);
    }
    #[test]
    fn tool_lifecycle_updates_one_timeline_card() {
        let mut view = Snapshot {
            workspace: None,
            session_id: String::new(),
            metrics: Value::Null,
            status: String::new(),
            busy: true,
            api_configured: false,
            settings: Settings::default(),
            messages: vec![],
            cards: vec![],
            error: None,
            pending_workspace: None,
            network: String::new(),
            python: String::new(),
        };
        record_timeline_card(
            &mut view,
            &json!({
                "event":"item.started",
                "item_id":"item_tool",
                "payload":{
                    "item":{"id":"item_tool","kind":"tool_call","status":"in_progress","detail":"{}"},
                    "tool":{"id":"call_1","name":"File","input":{"action":"read","path":"report.md"}}
                }
            }),
        );
        let sequence = view.cards[0]["sequence"].as_u64().unwrap();
        record_timeline_card(
            &mut view,
            &json!({
                "event":"item.completed",
                "item_id":"item_tool",
                "payload":{"item":{
                    "id":"item_tool","kind":"tool_call","status":"completed",
                    "summary":"File: read 12 lines","detail":"read 12 lines",
                    "metadata":{"tool_use_id":"call_1","tool_name":"File","tool_input":"{\"action\":\"read\",\"path\":\"report.md\"}"}
                }}
            }),
        );
        assert_eq!(view.cards.len(), 1);
        assert_eq!(view.cards[0]["sequence"], sequence);
        assert_eq!(view.cards[0]["event"], "item.completed");
        assert_eq!(view.cards[0]["payload"]["tool"]["name"], "File");
        assert_eq!(view.cards[0]["payload"]["item"]["detail"], "read 12 lines");
    }
    #[test]
    fn launch_directory_is_default_but_explicit_workspace_wins() {
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
    fn api_settings_keep_legacy_credentials_and_validate_model_effort() {
        let old: Settings = serde_json::from_value(json!({"schema_version":1,"model":"deepseek-flash","base_url":"https://api.deepseek.com"})).unwrap();
        let migrated = validate_settings(old).unwrap();
        assert_eq!(migrated.provider, "deepseek");
        assert_eq!(migrated.reasoning_effort, "auto");
        assert_eq!(migrated.schema_version, 2);
        for (provider, model, valid, invalid) in [
            (ProviderKind::Deepseek, "deepseek-flash", "low", "medium"),
            (ProviderKind::Moonshot, "kimi-k3", "low", "off"),
            (ProviderKind::Moonshot, "kimi-k2.6", "off", "max"),
            (ProviderKind::Zai, "GLM-5.3", "max", "off"),
        ] {
            let settings = Settings {
                provider: provider.as_str().into(),
                model: model.into(),
                base_url: provider.provider().default_base_url().into(),
                reasoning_effort: valid.into(),
                ..Default::default()
            };
            assert!(validate_settings(settings.clone()).is_ok(), "{model}");
            assert!(
                validate_settings(Settings {
                    reasoning_effort: invalid.into(),
                    ..settings
                })
                .is_err(),
                "{model}"
            );
            assert_eq!(
                effort_options(provider, "https://proxy.example/v1", model),
                ["auto"]
            );
        }
    }
    #[test]
    fn api_key_masks_and_empty_edits_cannot_replace_a_credential() {
        for invalid in ["", "  ", "********", "••••••••", "●●●●"] {
            assert!(validate_api_secret(invalid).is_err());
        }
        assert_eq!(
            validate_api_secret("  example-test-key  ").unwrap(),
            "example-test-key"
        );
    }

    #[test]
    fn api_catalog_offers_native_defaults_without_oauth_only_routes() {
        let catalog = api_catalog();
        let rows = catalog.as_array().unwrap();
        for kind in [
            ProviderKind::Deepseek,
            ProviderKind::Moonshot,
            ProviderKind::Zai,
        ] {
            let row = rows.iter().find(|row| row["id"] == kind.as_str()).unwrap();
            assert!(
                row["models"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|model| model["id"] == row["model"])
            );
            assert!(!kind.provider().env_vars().is_empty());
        }
        assert!(!rows.iter().any(|row| row["id"] == "openai-codex"));
        assert!(
            validate_settings(Settings {
                provider: "unknown".into(),
                ..Default::default()
            })
            .is_err()
        );
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
