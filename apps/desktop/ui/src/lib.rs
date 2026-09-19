use leptos::{prelude::*, task::spawn_local};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch,js_namespace=window,js_name=medsciInvoke)]
    async fn invoke(command: &str, args: JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace=window,js_name=medsciNative)]
    fn native() -> bool;
    #[wasm_bindgen(js_namespace=window,js_name=medsciReferences)]
    fn references() -> JsValue;
    #[wasm_bindgen(js_namespace=window,js_name=medsciReferencesPending)]
    fn references_pending() -> bool;
    #[wasm_bindgen(js_namespace=window,js_name=medsciRemoveReferences)]
    fn remove_references(sent: &str);
    #[wasm_bindgen(catch,js_namespace=window,js_name=medsciCopyText)]
    async fn copy_text(text: &str) -> Result<JsValue, JsValue>;

}
async fn call(command: &str, args: Value) -> Result<Value, String> {
    let result = invoke(command, serde_wasm_bindgen::to_value(&args).unwrap())
        .await
        .map_err(|e| e.as_string().unwrap_or_else(|| "操作失败".into()))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}
fn markdown(text: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(
        text,
        pulldown_cmark::Options::ENABLE_TABLES | pulldown_cmark::Options::ENABLE_STRIKETHROUGH,
    )
    .map(|e| match e {
        pulldown_cmark::Event::Html(v) | pulldown_cmark::Event::InlineHtml(v) => {
            pulldown_cmark::Event::Text(v)
        }
        e => e,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    ammonia::Builder::default().clean(&html).to_string()
}
#[derive(Clone, Copy, PartialEq)]
enum ActivityStatus {
    Running,
    Success,
    Warning,
    Failed,
    Interrupted,
}
impl ActivityStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "执行中",
            Self::Success => "已完成",
            Self::Warning => "有警告",
            Self::Failed => "失败",
            Self::Interrupted => "已停止",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum ActivityKind {
    Shell,
    Read,
    Search,
    FileMutation,
    Web,
    Generic,
}
impl ActivityKind {
    fn label(self) -> &'static str {
        match self {
            Self::Shell => "运行命令",
            Self::Read => "读取文件",
            Self::Search => "搜索内容",
            Self::FileMutation => "修改文件",
            Self::Web => "访问网络",
            Self::Generic => "工具活动",
        }
    }
}
#[derive(Clone)]
struct ActivityView {
    kind: ActivityKind,
    status: ActivityStatus,
    name: String,
    subject: String,
    summary: String,
    output: String,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum FileOperation {
    Created,
    Updated,
    Renamed,
    Deleted,
}
impl FileOperation {
    fn label(self) -> &'static str {
        match self {
            Self::Created => "已创建",
            Self::Updated => "已更新",
            Self::Renamed => "已重命名",
            Self::Deleted => "已删除",
        }
    }
}
struct FileMutationView {
    status: ActivityStatus,
    operation: FileOperation,
    path: String,
    added: Option<u64>,
    deleted: Option<u64>,
    output: String,
}
enum TimelineItem {
    User(String),
    Answer(String),
    Reasoning {
        text: String,
        streaming: bool,
        duration_ms: Option<u64>,
    },
    Activity(ActivityView),
    ActivityGroup(Vec<ActivityView>),
    FileMutation(FileMutationView),
    Approval(Value),
    UserInput(Value),
    SystemNotice(String),
    Error(String),
}
fn card_item_kind(card: &Value) -> &str {
    card.pointer("/payload/item/kind")
        .and_then(Value::as_str)
        .unwrap_or("")
}
fn tool_input(payload: &Value) -> Value {
    if let Some(value) = payload.pointer("/tool/input") {
        return value.clone();
    }
    match payload.pointer("/item/metadata/tool_input") {
        Some(Value::String(value)) => serde_json::from_str(value).unwrap_or(Value::Null),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}
fn tool_name(payload: &Value) -> String {
    payload["tool_name"]
        .as_str()
        .or(payload.pointer("/tool/name").and_then(Value::as_str))
        .or(payload
            .pointer("/item/metadata/tool_name")
            .and_then(Value::as_str))
        .unwrap_or("Agent")
        .to_owned()
}
fn activity_kind(payload: &Value, input: &Value) -> ActivityKind {
    let item_kind = payload.pointer("/item/kind").and_then(Value::as_str);
    if item_kind == Some("command_execution") {
        return ActivityKind::Shell;
    }
    if item_kind == Some("file_change") {
        return ActivityKind::FileMutation;
    }
    let name = tool_name(payload).to_ascii_lowercase();
    let action = input["action"].as_str().unwrap_or("").to_ascii_lowercase();
    if [
        "write", "create", "edit", "replace", "delete", "remove", "move", "rename",
    ]
    .iter()
    .any(|value| action.contains(value))
    {
        ActivityKind::FileMutation
    } else if action.contains("search") || name.contains("search") || name.contains("grep") {
        ActivityKind::Search
    } else if name.contains("shell")
        || name.contains("bash")
        || name.contains("command")
        || name.contains("exec")
    {
        ActivityKind::Shell
    } else if name.contains("web") || name.contains("browser") || name.contains("http") {
        ActivityKind::Web
    } else if action.contains("read")
        || name.contains("read")
        || name == "file"
        || name.contains("list")
    {
        ActivityKind::Read
    } else {
        ActivityKind::Generic
    }
}
fn activity_status(card: &Value) -> ActivityStatus {
    let payload = &card["payload"];
    if payload
        .pointer("/item/metadata/is_error")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return ActivityStatus::Failed;
    }
    if payload
        .pointer("/item/metadata/warning")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return ActivityStatus::Warning;
    }
    match payload
        .pointer("/item/status")
        .and_then(Value::as_str)
        .or_else(|| card["event"].as_str())
        .unwrap_or("")
    {
        "completed" | "item.completed" => ActivityStatus::Success,
        "failed" | "item.failed" => ActivityStatus::Failed,
        "interrupted" | "canceled" | "item.interrupted" | "item.canceled" => {
            ActivityStatus::Interrupted
        }
        _ => ActivityStatus::Running,
    }
}
fn input_subject(input: &Value) -> String {
    for key in [
        "command",
        "cmd",
        "path",
        "file_path",
        "query",
        "pattern",
        "url",
    ] {
        if let Some(value) = input[key].as_str().filter(|value| !value.trim().is_empty()) {
            return value.to_owned();
        }
    }
    String::new()
}
fn activity_view(card: &Value) -> ActivityView {
    let payload = &card["payload"];
    let input = tool_input(payload);
    let status = activity_status(card);
    let item = &payload["item"];
    ActivityView {
        kind: activity_kind(payload, &input),
        status,
        name: tool_name(payload),
        subject: input_subject(&input),
        summary: payload["description"]
            .as_str()
            .or(payload["intent_summary"].as_str())
            .or(item["summary"].as_str())
            .unwrap_or("")
            .to_owned(),
        output: if status == ActivityStatus::Running {
            String::new()
        } else {
            item["detail"].as_str().unwrap_or("").to_owned()
        },
    }
}
fn metadata_count(item: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        item.pointer(&format!("/metadata/{key}"))
            .and_then(Value::as_u64)
    })
}
fn file_mutation_view(card: &Value) -> FileMutationView {
    let activity = activity_view(card);
    let payload = &card["payload"];
    let item = &payload["item"];
    let input = tool_input(payload);
    let action = item
        .pointer("/metadata/operation")
        .and_then(Value::as_str)
        .or(input["action"].as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let operation = if action.contains("delete") || action.contains("remove") {
        FileOperation::Deleted
    } else if action.contains("move") || action.contains("rename") {
        FileOperation::Renamed
    } else if action.contains("create") {
        FileOperation::Created
    } else {
        FileOperation::Updated
    };
    FileMutationView {
        status: activity.status,
        operation,
        path: item
            .pointer("/metadata/path")
            .and_then(Value::as_str)
            .or(input["destination"].as_str())
            .or(input["to"].as_str())
            .filter(|path| !path.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if activity.subject.is_empty() {
                    "文件".into()
                } else {
                    activity.subject
                }
            }),
        added: metadata_count(item, &["added", "additions", "lines_added"]),
        deleted: metadata_count(item, &["deleted", "deletions", "lines_deleted"]),
        output: activity.output,
    }
}
fn classify_timeline_item(entry: Value) -> Option<TimelineItem> {
    if entry["card"].is_object() {
        let card = entry["card"].clone();
        return match card["event"].as_str().unwrap_or("") {
            "approval.required" => Some(TimelineItem::Approval(card)),
            "user_input.required" => Some(TimelineItem::UserInput(card)),
            _ => match card_item_kind(&card) {
                "tool_call" | "command_execution" | "file_change" => {
                    let activity = activity_view(&card);
                    if activity.kind == ActivityKind::FileMutation {
                        Some(TimelineItem::FileMutation(file_mutation_view(&card)))
                    } else {
                        Some(TimelineItem::Activity(activity))
                    }
                }
                "status" | "context_compaction" => Some(TimelineItem::SystemNotice(
                    card.pointer("/payload/item/detail")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            card.pointer("/payload/item/summary")
                                .and_then(Value::as_str)
                        })
                        .unwrap_or("")
                        .to_owned(),
                )),
                "error" => Some(TimelineItem::Error(
                    card.pointer("/payload/item/detail")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            card.pointer("/payload/item/summary")
                                .and_then(Value::as_str)
                        })
                        .unwrap_or("Agent 执行失败")
                        .to_owned(),
                )),
                _ => None,
            },
        };
    }
    let text = entry["text"].as_str().unwrap_or("").to_owned();
    match entry["role"].as_str().unwrap_or("") {
        "user" => Some(TimelineItem::User(text)),
        "reasoning" => Some(TimelineItem::Reasoning {
            text,
            streaming: entry["streaming"].as_bool().unwrap_or(false),
            duration_ms: entry["duration_ms"].as_u64(),
        }),
        "assistant" => Some(TimelineItem::Answer(text)),
        _ => None,
    }
}
fn groupable_activity(activity: &ActivityView) -> bool {
    matches!(activity.kind, ActivityKind::Read | ActivityKind::Search)
        && matches!(
            activity.status,
            ActivityStatus::Running | ActivityStatus::Success
        )
}
fn flush_activity_group(pending: &mut Vec<ActivityView>, output: &mut Vec<TimelineItem>) {
    if pending.len() >= 3 {
        output.push(TimelineItem::ActivityGroup(std::mem::take(pending)));
    } else {
        output.extend(pending.drain(..).map(TimelineItem::Activity));
    }
}
fn group_activities(items: Vec<TimelineItem>) -> Vec<TimelineItem> {
    let mut output = Vec::with_capacity(items.len());
    let mut pending = Vec::new();
    for item in items {
        match item {
            TimelineItem::Activity(activity) if groupable_activity(&activity) => {
                pending.push(activity)
            }
            other => {
                flush_activity_group(&mut pending, &mut output);
                output.push(other);
            }
        }
    }
    flush_activity_group(&mut pending, &mut output);
    output
}
fn timeline(data: &Value) -> Vec<TimelineItem> {
    let mut entries = data["messages"].as_array().cloned().unwrap_or_default();
    let count = entries.len() as u64;
    for (index, entry) in entries.iter_mut().enumerate() {
        if entry["sequence"].as_u64().unwrap_or(0) == 0 {
            entry["sequence"] = json!(index + 1);
        }
    }
    for (index, card) in data["cards"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        if card["event"] == "approval.required"
            || card["event"] == "user_input.required"
            || card["event"]
                .as_str()
                .is_some_and(|event| event.starts_with("item."))
        {
            entries.push(json!({"sequence":card["sequence"].as_u64().unwrap_or(count+index as u64+1),"card":card}));
        }
    }
    entries.sort_by_key(|entry| entry["sequence"].as_u64().unwrap_or(0));
    group_activities(
        entries
            .into_iter()
            .filter_map(classify_timeline_item)
            .collect(),
    )
}
fn context_label(data: &Value) -> String {
    let context = &data["metrics"]["context"];
    match (
        context["used_tokens"].as_f64(),
        context["total_tokens"].as_f64(),
    ) {
        (Some(used), Some(total)) if total > 0.0 => format!(
            "上下文 {:.1}K/{:.0}K {:.0}%",
            used / 1024.0,
            total / 1024.0,
            used / total * 100.0
        ),
        _ => "上下文 —K/—K —%".into(),
    }
}
fn cost_label(data: &Value) -> String {
    let t = &data["metrics"]["totals"];
    let cny = t["cny_priced_turns"].as_u64().unwrap_or(0);
    let usd = t["priced_turns"].as_u64().unwrap_or(0);
    if cny > 0 {
        format!(
            "计费 {}¥{:.4}",
            if t["cny_unpriced_turns"].as_u64().unwrap_or(0) > 0 {
                "≥"
            } else {
                ""
            },
            t["cost_cny"].as_f64().unwrap_or(0.0)
        )
    } else if usd > 0 {
        format!(
            "计费 {}${:.4}",
            if t["cost_complete"].as_bool() == Some(true) {
                ""
            } else {
                "≥"
            },
            t["cost_usd"].as_f64().unwrap_or(0.0)
        )
    } else {
        "计费 暂无数据".into()
    }
}
fn action(command: &'static str, args: Value, error: RwSignal<String>) {
    spawn_local(async move {
        if let Err(e) = call(command, args).await {
            error.set(e)
        }
    })
}
fn preview_text(text: &str, max_lines: usize, max_chars: usize) -> (String, bool) {
    let mut preview = String::new();
    let mut truncated = false;
    for (index, line) in text.lines().enumerate() {
        if index >= max_lines {
            truncated = true;
            break;
        }
        if !preview.is_empty() {
            preview.push('\n');
        }
        let remaining = max_chars.saturating_sub(preview.chars().count());
        if line.chars().count() > remaining {
            preview.extend(line.chars().take(remaining));
            truncated = true;
            break;
        }
        preview.push_str(line);
    }
    if !truncated && text.chars().count() > preview.chars().count() {
        truncated = true;
    }
    (preview, truncated)
}
fn reasoning_label(streaming: bool, duration_ms: Option<u64>) -> String {
    if streaming {
        "思考中…".into()
    } else if let Some(duration_ms) = duration_ms {
        if duration_ms < 1_000 {
            "思考了不到 1 秒".into()
        } else {
            format!("思考了 {} 秒", (duration_ms + 500) / 1_000)
        }
    } else {
        "思考".into()
    }
}
#[component]
fn Answer(text: String) -> impl IntoView {
    let copied = RwSignal::new(false);
    let source = StoredValue::new(text.clone());
    view! {
        <article class="answer">
            <button class="copy-answer" type="button" on:click=move |_| spawn_local(async move {
                if copy_text(&source.get_value()).await.is_ok() {
                    copied.set(true);
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                    copied.set(false);
                }
            })>{move || if copied.get() { "已复制" } else { "复制 Markdown" }}</button>
            <div inner_html=markdown(&text)></div>
        </article>
    }
}
#[component]
fn Reasoning(text: String, streaming: bool, duration_ms: Option<u64>) -> impl IntoView {
    let long = !streaming && (text.lines().count() > 4 || text.chars().count() > 280);
    let expanded = RwSignal::new(false);
    let html = markdown(&text);
    let label = StoredValue::new(reasoning_label(streaming, duration_ms));
    view! {
        <article class="reasoning">
            <Show
                when=move || long
                fallback=move || view! { <div class="reasoning-header">{label.get_value()}</div> }
            >
                <button class="reasoning-header inline-action" aria-expanded=move || expanded.get() on:click=move |_| expanded.update(|value| *value = !*value)>
                    {move || format!("{} · {}", label.get_value(), if expanded.get() { "收起" } else { "展开" })}
                </button>
            </Show>
            <div class=move || if streaming { "reasoning-body streaming" } else if long && !expanded.get() { "reasoning-body collapsed" } else { "reasoning-body" } inner_html=html></div>
        </article>
    }
}
#[component]
fn ActivityGroup(activities: Vec<ActivityView>) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let running = activities
        .iter()
        .filter(|activity| activity.status == ActivityStatus::Running)
        .count();
    let completed = activities.len() - running;
    let count = activities.len();
    let activities = StoredValue::new(activities);
    let status = if running > 0 {
        format!("{completed} 项完成 · {running} 项仍在进行")
    } else {
        format!("{completed} 项完成")
    };
    view! {
        <section class="activity activity-group">
            <button class="activity-group-header" type="button" aria-expanded=move || expanded.get() on:click=move |_| expanded.update(|value| *value = !*value)>
                <span>{format!("检查了 {count} 项内容")}</span>
                <small>{status}</small>
            </button>
            <Show when=move || expanded.get()>
                <div class="activity-group-items">{move || activities.get_value().into_iter().map(|activity| {
                    let symbol = if activity.status == ActivityStatus::Success { "✓" } else { "○" };
                    let detail = if activity.subject.is_empty() { activity.summary } else { activity.subject };
                    view! { <div class="activity-group-item" data-status=activity.status.key()><span aria-hidden="true">{symbol}</span><span>{format!("{} {}", activity.kind.label(), detail)}</span></div> }
                }).collect_view()}</div>
            </Show>
        </section>
    }
}
#[component]
fn Activity(activity: ActivityView) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let (max_lines, max_chars) = match activity.status {
        ActivityStatus::Success => (6, 2_000),
        ActivityStatus::Warning => (10, 4_000),
        ActivityStatus::Failed => (20, 8_000),
        ActivityStatus::Interrupted => (10, 4_000),
        ActivityStatus::Running => (0, 0),
    };
    let (preview, truncated) = preview_text(&activity.output, max_lines, max_chars);
    let output = StoredValue::new(activity.output);
    let preview = StoredValue::new(preview);
    let subject = if activity.subject.is_empty() {
        activity.summary.clone()
    } else {
        activity.subject.clone()
    };
    let summary = if activity.summary == subject
        || activity.summary.starts_with(&format!("{}:", activity.name))
    {
        String::new()
    } else {
        activity.summary.clone()
    };
    let subject = StoredValue::new(subject);
    let summary = StoredValue::new(summary);
    view! {
        <section class="activity" data-status=activity.status.key() aria-label=format!("{}，{}", activity.kind.label(), activity.status.label())>
            <div class="activity-header">{format!("{} · {}", activity.kind.label(), activity.status.label())}</div>
            <Show when=move || !subject.get_value().is_empty()><div class="activity-summary">{subject.get_value()}</div></Show>
            <Show when=move || !summary.get_value().is_empty()><div class="activity-meta">{summary.get_value()}</div></Show>
            <Show when=move || !output.get_value().trim().is_empty()>
                <pre class="activity-output">{move || if expanded.get() { output.get_value() } else { preview.get_value() }}</pre>
                <Show when=move || truncated>
                    <button class="inline-action" aria-expanded=move || expanded.get() on:click=move |_| expanded.update(|value| *value = !*value)>
                        {move || if expanded.get() { "收起输出" } else { "查看输出" }}
                    </button>
                </Show>
            </Show>
        </section>
    }
}
fn document_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".docx", ".pptx", ".xlsx", ".pdf"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}
fn file_diff_label(path: &str, added: Option<u64>, deleted: Option<u64>) -> String {
    if document_file(path) {
        return String::new();
    }
    match (added, deleted) {
        (Some(added), Some(deleted)) => format!("+{added} −{deleted}"),
        (Some(added), None) => format!("+{added}"),
        (None, Some(deleted)) => format!("−{deleted}"),
        (None, None) => String::new(),
    }
}
#[component]
fn FileMutation(file: FileMutationView, error: RwSignal<String>) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let is_document = document_file(&file.path);
    let title = if file.operation == FileOperation::Created && is_document {
        "生成文件"
    } else if file.operation == FileOperation::Deleted {
        "删除文件"
    } else {
        "修改文件"
    };
    let diff = file_diff_label(&file.path, file.added, file.deleted);
    let (preview, truncated) = preview_text(&file.output, 20, 8_000);
    let output = StoredValue::new(file.output);
    let preview = StoredValue::new(preview);
    let path = StoredValue::new(file.path);
    let can_open = file.status == ActivityStatus::Success
        && file.operation != FileOperation::Deleted
        && path.get_value() != "文件";
    view! {
        <section class="file-mutation" data-status=file.status.key() data-operation=file.operation.label() aria-label=format!("{title}，{}", file.status.label())>
            <div class="activity-header">{format!("{title} · {}", file.status.label())}</div>
            <div class="file-path">{path.get_value()}</div>
            <div class="activity-meta">{if diff.is_empty() { file.operation.label().to_owned() } else { format!("{} · {diff}", file.operation.label()) }}</div>
            <Show when=move || !output.get_value().trim().is_empty() && file.status == ActivityStatus::Failed>
                <pre class="activity-output">{move || if expanded.get() { output.get_value() } else { preview.get_value() }}</pre>
                <Show when=move || truncated><button class="inline-action" type="button" aria-expanded=move || expanded.get() on:click=move |_| expanded.update(|value| *value = !*value)>{move || if expanded.get() { "收起输出" } else { "查看输出" }}</button></Show>
            </Show>
            <Show when=move || can_open><button class="inline-action file-action" type="button" on:click=move |_| action("open_file", json!({"path": path.get_value()}), error)>"打开文件"</button></Show>
        </section>
    }
}
fn interaction_id(payload: &Value) -> String {
    payload["approval_id"]
        .as_str()
        .or(payload["input_id"].as_str())
        .or(payload["request_id"].as_str())
        .or(payload["id"].as_str())
        .unwrap_or("")
        .to_owned()
}
#[component]
fn ApprovalCard(card: Value, error: RwSignal<String>) -> impl IntoView {
    let p = card["payload"].clone();
    let id = StoredValue::new(interaction_id(&p));
    let description = p["description"]
        .as_str()
        .or(p["intent_summary"].as_str())
        .unwrap_or("Agent 请求执行一项需要确认的操作。")
        .to_owned();
    view! {
        <section class="approval-card" aria-label="需要你的确认">
            <h3>"需要你的确认"</h3>
            <p>{description}</p>
            <div class="row">
                <button on:click=move |_| action("decision", json!({"id": id.get_value(), "allow": false}), error)>"拒绝"</button>
                <button class="primary" on:click=move |_| action("decision", json!({"id": id.get_value(), "allow": true}), error)>"允许一次"</button>
            </div>
        </section>
    }
}
#[component]
fn UserInputCard(card: Value, error: RwSignal<String>) -> impl IntoView {
    let p = card["payload"].clone();
    let id = StoredValue::new(interaction_id(&p));
    let questions = p
        .pointer("/request/questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let questions = StoredValue::new(questions);
    let answers = RwSignal::new(std::collections::BTreeMap::<String, String>::new());
    view! {
        <section class="user-input-card" aria-label="Agent 需要补充信息">
            <h3>"Agent 需要补充信息"</h3>
            {move || questions.get_value().into_iter().map(|question| {
                let qid = question["id"].as_str().unwrap_or("").to_owned();
                let title = question["question"].as_str().or(question["title"].as_str()).unwrap_or("请补充信息").to_owned();
                let options = question["options"].as_array().cloned().unwrap_or_default();
                let has_options = !options.is_empty();
                let qid_for_input = qid.clone();
                view! {
                    <fieldset>
                        <legend>{title.clone()}</legend>
                        <Show
                            when=move || has_options
                            fallback=move || {
                                let qid = qid_for_input.clone();
                                view! { <input aria-label=title.clone() on:input=move |event| { let value = event_target_value(&event); answers.update(|current| { current.insert(qid.clone(), value); }); }/> }
                            }
                        >
                            <div class="choices">{options.clone().into_iter().map(|option| {
                                let label = option["label"].as_str().unwrap_or("选项").to_owned();
                                let value = option["value"].as_str().unwrap_or(&label).to_owned();
                                let selected_id = qid.clone();
                                let selected_value = value.clone();
                                let pressed_id = qid.clone();
                                let pressed_value = value.clone();
                                let click_id = qid.clone();
                                let click_value = value.clone();
                                view! {
                                    <button
                                        type="button"
                                        class=move || if answers.get().get(&selected_id) == Some(&selected_value) { "choice selected" } else { "choice" }
                                        aria-pressed=move || answers.get().get(&pressed_id) == Some(&pressed_value)
                                        on:click=move |_| answers.update(|current| { current.insert(click_id.clone(), click_value.clone()); })
                                    >{label}</button>
                                }
                            }).collect_view()}</div>
                        </Show>
                    </fieldset>
                }
            }).collect_view()}
            <div class="row"><button class="primary" on:click=move |_| action("answer_questions", json!({"id": id.get_value(), "answers": answers.get().into_iter().map(|(id, value)| json!({"id": id, "label": value, "value": value})).collect::<Vec<_>>()}), error)>"提交回答"</button></div>
        </section>
    }
}
#[component]
fn ApiSettings(settings: Value, notice: RwSignal<String>) -> impl IntoView {
    let provider = RwSignal::new(
        settings["provider"]
            .as_str()
            .unwrap_or("deepseek")
            .to_owned(),
    );
    let base = RwSignal::new(
        settings["base_url"]
            .as_str()
            .unwrap_or("https://api.deepseek.com")
            .to_owned(),
    );
    let model = RwSignal::new(
        settings["model"]
            .as_str()
            .unwrap_or("deepseek-flash")
            .to_owned(),
    );
    let effort = RwSignal::new(
        settings["reasoning_effort"]
            .as_str()
            .unwrap_or("auto")
            .to_owned(),
    );
    let secret = RwSignal::new(String::new());
    let has_key = RwSignal::new(false);
    let key_edited = RwSignal::new(false);
    let key_version = RwSignal::new(0_u64);
    let key_status = RwSignal::new(String::new());
    let catalog = RwSignal::new(Vec::<Value>::new());
    let efforts = RwSignal::new(vec!["auto".to_owned()]);
    let loading = RwSignal::new(true);
    let saving = RwSignal::new(false);
    Effect::new(move |_| {
        let selected = provider.get();
        let version = key_version.get_untracked();
        spawn_local(async move {
            let result = call("api_key_status", json!({"provider": selected})).await;
            if provider.try_get_untracked().as_ref() != Some(&selected)
                || key_version.try_get_untracked() != Some(version)
            {
                return;
            }
            match result {
                Ok(value) => has_key.set(value.as_bool().unwrap_or(false)),
                Err(e) => key_status.set(e),
            }
        });
    });
    spawn_local(async move {
        match call("api_catalog", json!({})).await {
            Ok(value) => catalog.set(value.as_array().cloned().unwrap_or_default()),
            Err(e) => notice.set(e),
        }
    });
    Effect::new(move |_| {
        let route = (provider.get(), base.get(), model.get());
        loading.set(true);
        spawn_local(async move {
            let result = call(
                "api_efforts",
                json!({"provider":route.0,"baseUrl":route.1,"model":route.2}),
            )
            .await;
            if route
                != (
                    provider.get_untracked(),
                    base.get_untracked(),
                    model.get_untracked(),
                )
            {
                return;
            }
            match result {
                Ok(value) => {
                    let options: Vec<String> = value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    if !options.contains(&effort.get_untracked()) {
                        effort.set("auto".into());
                    }
                    efforts.set(options);
                }
                Err(e) => {
                    efforts.set(vec!["auto".into()]);
                    effort.set("auto".into());
                    notice.set(e);
                }
            }
            loading.set(false);
        });
    });
    let choose_provider = move |id: String| {
        if id == provider.get_untracked() {
            return;
        }
        if let Some(row) = catalog.get_untracked().iter().find(|row| row["id"] == id) {
            provider.set(id);
            base.set(row["base_url"].as_str().unwrap_or_default().into());
            model.set(row["model"].as_str().unwrap_or_default().into());
            effort.set("auto".into());
            secret.set(String::new());
            has_key.set(false);
            key_edited.set(false);
            key_status.set(String::new());
            key_version.update(|v| *v += 1);
            notice.set(String::new());
        }
    };
    let models = move || {
        catalog
            .get()
            .into_iter()
            .find(|row| row["id"] == provider.get())
            .and_then(|row| row["models"].as_array().cloned())
            .unwrap_or_default()
    };
    let common = move || matches!(provider.get().as_str(), "deepseek" | "moonshot" | "zai");
    let effort_label = move |value: &str| match value {
        "auto" => "自动",
        "off" => "关闭",
        "minimal" => "极低",
        "low" => "低",
        "medium" => "中",
        "high"
            if (provider.get() == "moonshot" && model.get() != "kimi-k3")
                || (provider.get() == "zai" && model.get().eq_ignore_ascii_case("glm-5-turbo")) =>
        {
            "开启"
        }
        "high" => "高",
        "xhigh" => "极高",
        "max" => "最大",
        "ultra" => "超高",
        _ => "自动",
    };
    view! {
        <h2>"API 与模型"</h2>
        <fieldset class="api-options"><legend>"服务商"</legend><div class="provider-choices">
            {[("deepseek", "DeepSeek"), ("moonshot", "Kimi"), ("zai", "GLM")].into_iter().map(|(id, label)| view! {
                <button type="button" class="choice" class:selected=move || provider.get() == id aria-pressed=move || provider.get() == id on:click=move |_| choose_provider(id.into())>{label}</button>
            }).collect_view()}
            <select aria-label="其他服务商" prop:value=move || if common() { String::new() } else { provider.get() } on:change=move |e| choose_provider(event_target_value(&e))>
                <option value="" disabled=true>"其他服务商…"</option>
                {move || catalog.get().into_iter().filter(|row| !matches!(row["id"].as_str(), Some("deepseek" | "moonshot" | "zai"))).map(|row| view! {
                    <option value=row["id"].as_str().unwrap_or_default().to_owned()>{row["name"].as_str().unwrap_or_default().to_owned()}</option>
                }).collect_view()}
            </select>
        </div></fieldset>
        <fieldset class="api-options"><legend>"模型"</legend>
            <Show when=common>
                <div class="model-choices">{move || models().into_iter().filter(|row| !matches!(row["id"].as_str(), Some("deepseek-v4-flash" | "deepseek-v4-flash-vision-exp"))).map(|row| {
                    let id = row["id"].as_str().unwrap_or_default().to_owned();
                    let selected = id.clone(); let pressed = id.clone(); let label = id.clone();
                    view! { <button type="button" class="choice" class:selected=move || model.get() == selected aria-pressed=move || model.get() == pressed on:click=move |_| model.set(id.clone())>{label}</button> }
                }).collect_view()}</div>
            </Show>
            <Show when=move || !common()>
                <select aria-label="选择模型" prop:value=move || model.get() on:change=move |e| model.set(event_target_value(&e))>
                    {move || models().into_iter().map(|row| view! { <option value=row["id"].as_str().unwrap_or_default().to_owned()>{row["name"].as_str().unwrap_or_default().to_owned()}</option> }).collect_view()}
                </select>
            </Show>
            <details class="api-custom"><summary>"自定义模型 ID"</summary><label for="model">"模型 ID"</label><input id="model" prop:value=move || model.get() on:input=move |e| model.set(event_target_value(&e))/></details>
        </fieldset>
        <div class="effort-heading"><label for="effort">"思考强度"</label><output for="effort">{move || effort_label(&effort.get())}</output></div>
        <input id="effort" type="range" min="0" max=move || efforts.get().len().saturating_sub(1).max(1) step="1"
            disabled=move || loading.get() || efforts.get().len() < 2
            prop:value=move || efforts.get().iter().position(|v| v == &effort.get()).unwrap_or(0).to_string()
            aria-valuetext=move || effort_label(&effort.get())
            on:input=move |e| { if let Ok(index) = event_target_value(&e).parse::<usize>() { if let Some(value) = efforts.get().get(index) { effort.set(value.clone()); } } }/>
        <div class="effort-ticks" aria-hidden="true">{move || efforts.get().iter().map(|value| view! { <span>{effort_label(value)}</span> }).collect_view()}</div>
        <label for="key">"API Key"</label>
        <input id="key" type="password" autocomplete="off"
            prop:value=move || if has_key.get() && !key_edited.get() { "********".to_owned() } else { secret.get() }
            on:focus=move |_| { if !key_edited.get_untracked() { secret.set(String::new()); key_edited.set(true); } }
            on:blur=move |_| { if secret.get_untracked().is_empty() { key_edited.set(false); } }
            on:input=move |event| {
                let value = event_target_value(&event);
                let selected = provider.get_untracked();
                key_version.update(|v| *v += 1);
                let version = key_version.get_untracked();
                key_edited.set(true);
                secret.set(value.clone());
                key_status.set(if value.is_empty() { String::new() } else { "正在自动保存…".into() });
                spawn_local(async move {
                    let result = call("save_api_key", json!({"provider":selected,"secret":value})).await;
                    if provider.try_get_untracked().as_ref() != Some(&selected)
                        || key_version.try_get_untracked() != Some(version) { return; }
                    match result {
                        Ok(saved) if saved == true => {
                            has_key.set(true);
                            key_status.set("共享密钥已保存；其他窗口重启 Agent 后生效".into());
                        }
                        Ok(_) => {}
                        Err(e) => key_status.set(e),
                    }
                });
            }/>
        <Show when=move || !key_status.get().is_empty()><p class="api-hint" role="status">{move || key_status.get()}</p></Show>
        <div class="row">
            <button class="primary" disabled=move || saving.get() || loading.get() on:click=move |_| {
                saving.set(true);
                let args = json!({"settings":{"schema_version":2,"provider":provider.get(),"base_url":base.get(),"model":model.get(),"reasoning_effort":effort.get()},"secret":""});
                spawn_local(async move {
                    match call("save_api", args).await { Ok(_) => { notice.set("配置已保存，用于当前窗口及新窗口；其他窗口保持原配置".into()); }, Err(e) => notice.set(e) }
                    saving.set(false);
                });
            }>{move || if saving.get() { "保存中…" } else { "保存" }}</button>
        </div>
    }
}

#[component]
fn App() -> impl IntoView {
    let data = RwSignal::new(
        json!({"messages":[],"cards":[],"settings":{"model":"deepseek-flash"},"status":"未启动"}),
    );
    let draft = RwSignal::new(String::new());
    let reference_count = RwSignal::new(0usize);
    let sending = RwSignal::new(false);
    let pasting = RwSignal::new(false);
    let has_content = move || !draft.get().trim().is_empty() || reference_count.get() > 0;
    let panel = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let notice = RwSignal::new(String::new());
    let sessions = RwSignal::new(Vec::<Value>::new());
    let choosing_workspace = RwSignal::new(false);
    let choose_workspace = move |_| {
        if choosing_workspace.get_untracked() {
            return;
        }
        choosing_workspace.set(true);
        spawn_local(async move {
            if let Err(e) = call("open_folder", json!({})).await {
                error.set(e);
            }
            choosing_workspace.set(false);
        });
    };
    spawn_local(async move {
        if !native() {
            return;
        }
        loop {
            match call("snapshot", json!({})).await {
                Ok(v) => {
                    if data.get_untracked() != v {
                        data.set(v)
                    }
                }
                Err(e) => error.set(e),
            }
            gloo_timers::future::TimeoutFuture::new(350).await;
        }
    });
    let submit = move || {
        if sending.get_untracked() || references_pending() {
            return;
        }
        if !data.get()["api_configured"].as_bool().unwrap_or(false) {
            panel.set("api".into());
            return;
        }
        let text = draft.get();
        let attached: Value = serde_wasm_bindgen::from_value(references()).unwrap_or(json!([]));
        if text.trim().is_empty() && attached.as_array().is_none_or(Vec::is_empty) {
            return;
        }
        error.set(String::new());
        sending.set(true);
        spawn_local(async move {
            match call("send_message", json!({"text":text,"references":attached})).await {
                Ok(_) => {
                    if draft.get_untracked() == text {
                        draft.set(String::new());
                    }
                    remove_references(&attached.to_string());
                }
                Err(e) => error.set(e),
            }
            sending.set(false);
        });
    };
    let open_api = move |_| {
        notice.set(String::new());
        panel.set("api".into());
    };
    view! {
    <div class="app">
     <header class="titlebar" data-tauri-drag-region><button type="button" class="workspace" aria-label="切换工作目录" title=move||format!("{}\n点击切换工作目录",data.get()["workspace"].as_str().unwrap_or("未选择工作目录")) disabled=move||choosing_workspace.get() on:click=choose_workspace>{move||data.get()["workspace"].as_str().map(|path|path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().filter(|name|!name.is_empty()).unwrap_or(path)).unwrap_or("选择工作目录").to_owned()}</button><div class="window-controls"><button data-window="minimize" aria-label="最小化">"−"</button><button data-window="maximize" aria-label="最大化或还原">"□"</button><button data-window="close" aria-label="关闭">"×"</button></div></header>
     <main aria-live="polite">
      {move || timeline(&data.get()).into_iter().map(|item| match item {
        TimelineItem::User(text) => view! { <article class="user"><div inner_html=markdown(&text)></div></article> }.into_any(),
        TimelineItem::Answer(text) => view! { <Answer text=text/> }.into_any(),
        TimelineItem::Reasoning { text, streaming, duration_ms } => view! { <Reasoning text=text streaming=streaming duration_ms=duration_ms/> }.into_any(),
        TimelineItem::Activity(activity) => view! { <Activity activity=activity/> }.into_any(),
        TimelineItem::ActivityGroup(activities) => view! { <ActivityGroup activities=activities/> }.into_any(),
        TimelineItem::FileMutation(file) => view! { <FileMutation file=file error=error/> }.into_any(),
        TimelineItem::Approval(card) => view! { <ApprovalCard card=card error=error/> }.into_any(),
        TimelineItem::UserInput(card) => view! { <UserInputCard card=card error=error/> }.into_any(),
        TimelineItem::SystemNotice(text) => view! { <div class="system-notice" role="status">{text}</div> }.into_any(),
        TimelineItem::Error(text) => view! { <div class="error" role="alert">{text}</div> }.into_any(),
      }).collect_view()}
      <Show when=move||!error.get().is_empty()><div class="error" role="alert">{move||error.get()}</div></Show>
      <Show when=move||data.get()["error"].is_string()><div class="error" role="alert">{move||data.get()["error"].as_str().unwrap_or("").to_string()}<div class="row"><button on:click=move |_|action("restart",json!({}),error)>"重启 Agent"</button></div></div></Show>
     </main>
     <button id="back-to-latest" class="back-to-latest" type="button" hidden>"回到最新"</button>
     <div class="composer" on:input=move |_| {let refs: Value=serde_wasm_bindgen::from_value(references()).unwrap_or(json!([]));reference_count.set(refs.as_array().map_or(0,Vec::len));pasting.set(references_pending());}><div id="attachments" role="list" aria-label="本次任务引用的文件和文件夹"></div><div id="reference-notice" role="status"></div><div class="input-row"><textarea rows="1" aria-label="输入任务" placeholder=move||if data.get()["busy"].as_bool().unwrap_or(false){"补充指令，调整当前任务…"}else{"描述你的任务…"} prop:value=move||draft.get() on:input=move|e|draft.set(event_target_value(&e)) on:keydown=move|e|{if e.key()=="Enter" && !e.shift_key() && !e.is_composing(){e.prevent_default();submit();}}></textarea><button class="send" disabled=move||sending.get()||pasting.get()||(!has_content()&&!data.get()["busy"].as_bool().unwrap_or(false)) aria-label=move||if !has_content(){"停止"}else if data.get()["busy"].as_bool().unwrap_or(false){"发送补充指令"}else{"发送"} title=move||if !has_content(){"停止"}else if data.get()["busy"].as_bool().unwrap_or(false){"Steer 当前任务"}else{"发送"} on:click=move |_|{if !has_content(){action("stop",json!({}),error)}else{submit()}}>{move||if !has_content(){"■"}else if data.get()["busy"].as_bool().unwrap_or(false){"↗"}else{"↑"}}</button></div></div>
     <footer><button on:click=open_api>{move||data.get()["settings"]["model"].as_str().unwrap_or("deepseek-flash").to_owned()}</button><button on:click=move |_|{panel.set("sessions".into());notice.set("正在读取…".into());spawn_local(async move {match call("conversations",json!({})).await {Ok(value)=>{sessions.set(value.as_array().cloned().unwrap_or_default());notice.set(String::new());},Err(e)=>notice.set(e)}})}>"会话"</button><span class="meter" title="本会话累计费用，按模型用量回执及价格表计算；≥ 表示仅部分调用可计价，实际结算以服务商账单为准。">{move||cost_label(&data.get())}</span><span class="meter" title="当前保留上下文的估算占用／模型上下文上限，按 1024 tokens = 1K；每轮结束后更新。">{move||context_label(&data.get())}</span><span class="spacer"></span><button on:click=open_api>{move||if data.get()["api_configured"].as_bool().unwrap_or(false){"API 已配置"}else{"配置 API"}}</button><button on:click=move |_|{notice.set(String::new());panel.set("login".into())}>"未登录"</button></footer>
    </div>
    <Show when=move||!panel.get().is_empty()>
    <div class="overlay" on:keydown=move|e|{if e.key()=="Escape"{panel.set(String::new());}}><section class=move||if panel.get()=="login"{"dialog login"}else if panel.get()=="api"{"dialog api-dialog"}else{"dialog"} role="dialog" aria-modal="true" aria-label="设置">
    <Show when=move||panel.get()=="sessions"><h2>"当前目录的会话"</h2><div class="session-list">{move||sessions.get().into_iter().map(|session|{let id=session["id"].as_str().unwrap_or("").to_owned();let store=session["store"].as_str().unwrap_or("legacy").to_owned();let active=id==data.get()["session_id"].as_str().unwrap_or("");let title=session["title"].as_str().unwrap_or("新会话").to_owned();view!{<button class=if active{"session-entry active"}else{"session-entry"} on:click=move |_|{let id=id.clone();let store=store.clone();spawn_local(async move{match call("select_conversation",json!({"id":id,"store":store})).await{Ok(_)=>panel.set(String::new()),Err(e)=>notice.set(e)}})}><span>{title}</span><small>{if active{"当前"}else{""}}</small></button>}}).collect_view()}</div><Show when=move||sessions.get().is_empty()&&notice.get().is_empty()><p>"此目录暂无既往会话。"</p></Show><div class="row"><button class="primary" on:click=move |_|spawn_local(async move{match call("new_session",json!({})).await{Ok(_)=>panel.set(String::new()),Err(e)=>notice.set(e)}})>"新建会话"</button></div></Show>
    <Show when=move||panel.get()=="api"><ApiSettings settings=data.get()["settings"].clone() notice=notice/></Show>
    <Show when=move||panel.get()=="login"><h2>"公司账号"</h2><p>"公司认证服务尚未接入，不影响使用个人 API 密钥测试。"</p><label for="account">"公司账号"</label><input id="account" autocomplete="username"/><label for="password">"密码"</label><input id="password" type="password" autocomplete="off"/><div class="row"><button class="primary" on:click=move |_|spawn_local(async move{if let Err(e)=call("company_login",json!({})).await{notice.set(e)}})>"登录"</button></div></Show>
    <Show when=move||panel.get()=="permissions"><h2>"工作区与权限"</h2><p>"Full Access 已启用：Agent 可执行命令、访问网络及读写当前系统用户有权限访问的文件，不再等待操作审批。"</p><p>{move||data.get()["workspace"].as_str().unwrap_or("未选择工作区").to_owned()}</p><p>"此开发预览默认禁用 MCP Registry；本地工具优先。"</p></Show>
    <Show when=move||panel.get()=="network"><h2>"网络连接"</h2><p>"当前构建使用仅直连模式。系统代理、PAC 自动回退和手动代理正在开发，尚不能作为已通过验收的功能。"</p><button on:click=move |_|spawn_local(async move{notice.set(match call("test_api",json!({})).await{Ok(v)=>v.as_str().unwrap_or("").into(),Err(e)=>e})})>"测试已保存配置"</button></Show>
    <Show when=move||panel.get()=="agent"><h2>"Agent 与运行环境"</h2><p>"Codewhale 0.9.13 · stdio JSON-RPC · 本地 SQLite 会话"</p><p>{move||format!("共享 Python：{}",data.get()["python"].as_str().unwrap_or("未初始化"))}</p><button on:click=move |_|spawn_local(async move{notice.set("正在初始化，请稍候…".into());notice.set(match call("initialize_python",json!({})).await{Ok(v)=>v.as_str().unwrap_or("").into(),Err(e)=>e})})>"初始化共享 Python"</button><p>"当前版本会话可恢复 Agent 历史；旧版本仅保存的文字记录不含 Runtime 推理上下文。"</p><button on:click=move |_|action("restart",json!({}),error)>"重启 Agent"</button></Show>
    <Show when=move||!notice.get().is_empty()><p role="status">{move||notice.get()}</p></Show><div class="row"><button on:click=move |_|{panel.set(String::new());notice.set(String::new());}>"关闭"</button></div>
    </section></div></Show>
    <Show when=move||data.get()["pending_workspace"].is_string()><div class="overlay"><section class="dialog" role="alertdialog" aria-modal="true"><h2>"切换工作区？"</h2><p>"当前任务正在执行。切换将停止任务并保存已有消息。"</p><div class="row"><button on:click=move |_|action("resolve_switch",json!({"allow":false}),error)>"取消"</button><button on:click=move |_|action("resolve_switch",json!({"allow":true}),error)>"停止并切换"</button></div></section></div></Show>
    }
}
#[wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(App)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_activity(status: ActivityStatus, subject: &str) -> ActivityView {
        ActivityView {
            kind: ActivityKind::Read,
            status,
            name: "File".into(),
            subject: subject.into(),
            summary: String::new(),
            output: String::new(),
        }
    }

    #[test]
    fn successful_output_preview_keeps_six_lines() {
        let output = (1..=8)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (preview, truncated) = preview_text(&output, 6, 2_000);
        assert!(truncated);
        assert_eq!(preview.lines().count(), 6);
        assert!(!preview.contains("line 7"));
    }

    #[test]
    fn failed_tool_event_becomes_failed_activity() {
        let item = classify_timeline_item(json!({
            "card":{
                "event":"item.failed",
                "payload":{"item":{
                    "kind":"tool_call",
                    "status":"failed",
                    "summary":"File failed",
                    "detail":"file is locked",
                    "metadata":{"tool_name":"File","tool_input":"{\"action\":\"read\",\"path\":\"data.xlsx\"}","is_error":true}
                }}
            }
        }))
        .expect("tool event should be visible");
        let TimelineItem::Activity(activity) = item else {
            panic!("tool event was not classified as activity");
        };
        assert_eq!(activity.status.key(), "failed");
        assert_eq!(activity.kind.label(), "读取文件");
        assert_eq!(activity.subject, "data.xlsx");
        assert_eq!(activity.output, "file is locked");
    }

    #[test]
    fn three_consecutive_reads_form_one_group() {
        let grouped = group_activities(vec![
            TimelineItem::Activity(read_activity(ActivityStatus::Success, "a.md")),
            TimelineItem::Activity(read_activity(ActivityStatus::Success, "b.md")),
            TimelineItem::Activity(read_activity(ActivityStatus::Running, "c.md")),
        ]);
        assert_eq!(grouped.len(), 1);
        let TimelineItem::ActivityGroup(activities) = &grouped[0] else {
            panic!("read activities were not grouped");
        };
        assert_eq!(activities.len(), 3);
    }

    #[test]
    fn failed_read_is_never_hidden_in_a_group() {
        let grouped = group_activities(vec![
            TimelineItem::Activity(read_activity(ActivityStatus::Success, "a.md")),
            TimelineItem::Activity(read_activity(ActivityStatus::Success, "b.md")),
            TimelineItem::Activity(read_activity(ActivityStatus::Failed, "c.md")),
            TimelineItem::Activity(read_activity(ActivityStatus::Success, "d.md")),
        ]);
        assert_eq!(grouped.len(), 4);
        assert!(
            grouped
                .iter()
                .all(|item| !matches!(item, TimelineItem::ActivityGroup(_)))
        );
    }

    #[test]
    fn reasoning_duration_label_is_human_readable() {
        assert_eq!(reasoning_label(true, Some(8_000)), "思考中…");
        assert_eq!(reasoning_label(false, Some(8_450)), "思考了 8 秒");
        assert_eq!(reasoning_label(false, None), "思考");
    }

    #[test]
    fn file_write_uses_file_mutation_model() {
        let item = classify_timeline_item(json!({
            "card":{
                "event":"item.completed",
                "payload":{"item":{
                    "kind":"tool_call","status":"completed","detail":"updated",
                    "metadata":{"tool_name":"File","tool_input":"{\"action\":\"edit\",\"path\":\"report.md\"}","additions":12,"deletions":3}
                }}
            }
        }))
        .expect("file write should be visible");
        let TimelineItem::FileMutation(file) = item else {
            panic!("file write did not use the file mutation model");
        };
        assert_eq!(file.operation, FileOperation::Updated);
        assert_eq!(file.path, "report.md");
        assert_eq!(file.added, Some(12));
        assert_eq!(file.deleted, Some(3));
    }

    #[test]
    fn office_files_never_show_text_diff_counts() {
        assert_eq!(file_diff_label("研究报告.docx", Some(28), Some(4)), "");
        assert_eq!(file_diff_label("analysis.py", Some(28), Some(4)), "+28 −4");
    }
}
