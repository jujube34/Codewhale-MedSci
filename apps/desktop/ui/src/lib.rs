use leptos::{prelude::*, task::spawn_local};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch,js_namespace=window,js_name=medsciInvoke)]
    async fn invoke(command: &str, args: JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace=window,js_name=medsciNative)]
    fn native() -> bool;
    #[wasm_bindgen(js_namespace=window,js_name=medsciImages)]
    fn images() -> JsValue;

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
fn timeline(data: &Value) -> Vec<Value> {
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
            || (card["event"] == "item.started" && card["payload"]["tool"].is_object())
        {
            entries.push(json!({"sequence":card["sequence"].as_u64().unwrap_or(count+index as u64+1),"card":card}));
        }
    }
    entries.sort_by_key(|entry| entry["sequence"].as_u64().unwrap_or(0));
    entries
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
#[component]
fn EventCard(card: Value, error: RwSignal<String>) -> impl IntoView {
    let event = card["event"].as_str().unwrap_or("").to_owned();
    let p = card["payload"].clone();
    let approval = event == "approval.required";
    let question = event == "user_input.required";
    let id = StoredValue::new(
        p["approval_id"]
            .as_str()
            .or(p["input_id"].as_str())
            .or(p["id"].as_str())
            .unwrap_or("")
            .to_owned(),
    );
    let name = p["tool_name"]
        .as_str()
        .or(p.pointer("/tool/name").and_then(Value::as_str))
        .unwrap_or("Agent")
        .to_owned();
    let description = p["description"]
        .as_str()
        .or(p["intent_summary"].as_str())
        .or(p.pointer("/item/summary").and_then(Value::as_str))
        .unwrap_or("")
        .to_owned();
    let questions = p
        .pointer("/request/questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let questions = StoredValue::new(questions);
    let answers = RwSignal::new(std::collections::BTreeMap::<String, String>::new());
    view! {<section class="tool-event"><div class="tool-label">{format!("{} · {}",name,if approval{"等待审批"}else if question{"需要你的回答"}else{match p["item"]["status"].as_str(){Some("completed")=>"已完成",Some("failed")=>"失败",Some("interrupted")=>"已停止",_=>"执行中"}})}</div><p>{description}</p>
    <Show when=move||approval><div class="row"><button on:click=move |_|action("decision",json!({"id":id.get_value(),"allow":false}),error)>"拒绝"</button><button class="primary" on:click=move |_|action("decision",json!({"id":id.get_value(),"allow":true}),error)>"允许一次"</button></div></Show>
    <Show when=move||question>{move||questions.get_value().into_iter().map(|q|{
      let qid=q["id"].as_str().unwrap_or("").to_owned();
      let title=q["question"].as_str().or(q["title"].as_str()).unwrap_or("请补充信息").to_owned();
      let options=q["options"].as_array().map(|v|v.iter().filter_map(|o|o["label"].as_str()).collect::<Vec<_>>().join(" / ")).unwrap_or_default();
      view!{<label>{title.clone()}<small>{options}</small><input aria-label=title on:input=move|e|{let value=event_target_value(&e);answers.update(|a|{a.insert(qid.clone(),value);});}/></label>}
    }).collect_view()}<div class="row"><button class="primary" on:click=move |_|action("answer_questions",json!({"id":id.get_value(),"answers":answers.get().into_iter().map(|(id,value)|json!({"id":id,"label":value,"value":value})).collect::<Vec<_>>()}),error)>"提交回答"</button></div></Show>
    </section>}
}
#[component]
fn App() -> impl IntoView {
    let data = RwSignal::new(
        json!({"messages":[],"cards":[],"settings":{"model":"deepseek-flash"},"status":"未启动"}),
    );
    let draft = RwSignal::new(String::new());
    let panel = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let base = RwSignal::new("https://api.deepseek.com".to_string());
    let model = RwSignal::new("deepseek-flash".to_string());
    let secret = RwSignal::new(String::new());
    let notice = RwSignal::new(String::new());
    let sessions = RwSignal::new(Vec::<Value>::new());
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
        if !data.get()["api_configured"].as_bool().unwrap_or(false) {
            panel.set("api".into());
            return;
        }
        let text = draft.get();
        let attached: Value = serde_wasm_bindgen::from_value(images()).unwrap_or(json!([]));
        if text.trim().is_empty() && attached.as_array().is_none_or(Vec::is_empty) {
            return;
        }
        error.set(String::new());
        spawn_local(async move {
            match call("send_message", json!({"text":text,"images":attached})).await {
                Ok(_) => draft.set(String::new()),
                Err(e) => error.set(e),
            }
        });
    };
    let open_api = move |_| {
        let s = data.get()["settings"].clone();
        base.set(
            s["base_url"]
                .as_str()
                .unwrap_or("https://api.deepseek.com")
                .into(),
        );
        model.set(s["model"].as_str().unwrap_or("deepseek-flash").into());
        secret.set(String::new());
        notice.set(String::new());
        panel.set("api".into());
    };
    view! {
    <div class="app">
     <header class="titlebar" data-tauri-drag-region><span class="workspace" data-tauri-drag-region>{move||data.get()["workspace"].as_str().and_then(|s|s.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next()).unwrap_or("").to_owned()}</span><div class="window-controls"><button data-window="minimize" aria-label="最小化">"−"</button><button data-window="maximize" aria-label="最大化或还原">"□"</button><button data-window="close" aria-label="关闭">"×"</button></div></header>
     <main aria-live="polite">
      {move||timeline(&data.get()).into_iter().map(|entry|{
        if entry["card"].is_object() {view!{<EventCard card=entry["card"].clone() error=error/>}.into_any()}
        else {let user=entry["role"]=="user";let reasoning=entry["role"]=="reasoning";let html=markdown(entry["text"].as_str().unwrap_or(""));view!{<article class=if user{"user"}else if reasoning{"assistant reasoning"}else{"assistant"}><div class="role">{if user{"你"}else if reasoning{"思考"}else{"CODEWHALE"}}</div><div inner_html=html></div></article>}.into_any()}
      }).collect_view()}
      <Show when=move||!error.get().is_empty()><div class="error" role="alert">{move||error.get()}</div></Show>
      <Show when=move||data.get()["error"].is_string()><div class="error" role="alert">{move||data.get()["error"].as_str().unwrap_or("").to_string()}<div class="row"><button on:click=move |_|action("restart",json!({}),error)>"重启 Agent"</button></div></div></Show>
     </main>
     <div class="composer"><div id="attachments"></div><div class="input-row"><textarea rows="1" aria-label="输入任务" placeholder=move||if data.get()["busy"].as_bool().unwrap_or(false){"补充指令，调整当前任务…"}else{"描述你的任务…"} prop:value=move||draft.get() on:input=move|e|draft.set(event_target_value(&e)) on:keydown=move|e|{if e.key()=="Enter" && !e.shift_key() && !e.is_composing(){e.prevent_default();submit();}}></textarea><button class="send" disabled=move||draft.get().trim().is_empty()&&!data.get()["busy"].as_bool().unwrap_or(false) aria-label=move||if draft.get().trim().is_empty(){"停止"}else if data.get()["busy"].as_bool().unwrap_or(false){"发送补充指令"}else{"发送"} title=move||if draft.get().trim().is_empty(){"停止"}else if data.get()["busy"].as_bool().unwrap_or(false){"Steer 当前任务"}else{"发送"} on:click=move |_|{if draft.get().trim().is_empty(){action("stop",json!({}),error)}else{submit()}}>{move||if draft.get().trim().is_empty(){"■"}else if data.get()["busy"].as_bool().unwrap_or(false){"↗"}else{"↑"}}</button></div></div>
     <footer><button on:click=open_api>{move||data.get()["settings"]["model"].as_str().unwrap_or("deepseek-flash").to_owned()}</button><button on:click=move |_|{panel.set("sessions".into());notice.set("正在读取…".into());spawn_local(async move {match call("conversations",json!({})).await {Ok(value)=>{sessions.set(value.as_array().cloned().unwrap_or_default());notice.set(String::new());},Err(e)=>notice.set(e)}})}>"会话"</button><span class="meter" title="本会话累计费用，按模型用量回执及价格表计算；≥ 表示仅部分调用可计价，实际结算以服务商账单为准。">{move||cost_label(&data.get())}</span><span class="meter" title="当前保留上下文的估算占用／模型上下文上限，按 1024 tokens = 1K；每轮结束后更新。">{move||context_label(&data.get())}</span><span class="spacer"></span><button on:click=open_api>{move||if data.get()["api_configured"].as_bool().unwrap_or(false){"API 已配置"}else{"配置 API"}}</button><button on:click=move |_|{notice.set(String::new());panel.set("login".into())}>"未登录"</button></footer>
    </div>
    <Show when=move||!panel.get().is_empty()>
    <div class="overlay" on:keydown=move|e|{if e.key()=="Escape"{panel.set(String::new());secret.set(String::new());}}><section class=move||if panel.get()=="login"{"dialog login"}else{"dialog"} role="dialog" aria-modal="true" aria-label="设置">
    <Show when=move||panel.get()=="sessions"><h2>"当前目录的会话"</h2><div class="session-list">{move||sessions.get().into_iter().map(|session|{let id=session["id"].as_str().unwrap_or("").to_owned();let active=id==data.get()["session_id"].as_str().unwrap_or("");let title=session["title"].as_str().unwrap_or("新会话").to_owned();view!{<button class=if active{"session-entry active"}else{"session-entry"} on:click=move |_|{let id=id.clone();spawn_local(async move{match call("select_conversation",json!({"id":id})).await{Ok(_)=>panel.set(String::new()),Err(e)=>notice.set(e)}})}><span>{title}</span><small>{if active{"当前"}else{""}}</small></button>}}).collect_view()}</div><Show when=move||sessions.get().is_empty()&&notice.get().is_empty()><p>"此目录暂无既往会话。"</p></Show><div class="row"><button class="primary" on:click=move |_|spawn_local(async move{match call("new_session",json!({})).await{Ok(_)=>panel.set(String::new()),Err(e)=>notice.set(e)}})>"新建会话"</button></div></Show>
    <Show when=move||panel.get()=="api"><h2>"连接 DeepSeek"</h2><p>"API 密钥仅保存在系统安全凭据库中。个人调用费用由你的 DeepSeek 账户承担。"</p><label for="base">"API Base URL"</label><input id="base" prop:value=move||base.get() on:input=move|e|base.set(event_target_value(&e))/><label for="model">"模型"</label><input id="model" prop:value=move||model.get() on:input=move|e|model.set(event_target_value(&e))/><label for="key">"API Key（留空保留已有密钥）"</label><input id="key" type="password" autocomplete="off" prop:value=move||secret.get() on:input=move|e|secret.set(event_target_value(&e))/><div class="row"><button on:click=move |_|action("delete_api",json!({}),error)>"删除密钥"</button><button on:click=move |_|spawn_local(async move{notice.set("正在连接…".into());notice.set(match call("test_api",json!({})).await{Ok(v)=>v.as_str().unwrap_or("").into(),Err(e)=>e})})>"测试已保存配置"</button><button class="primary" on:click=move |_|spawn_local(async move{let result=call("save_api",json!({"settings":{"schema_version":1,"base_url":base.get(),"model":model.get()},"secret":secret.get()})).await;secret.set(String::new());notice.set(match result{Ok(_)=>"配置已保存".into(),Err(e)=>e});})>"保存"</button></div></Show>
    <Show when=move||panel.get()=="login"><h2>"公司账号"</h2><p>"公司认证服务尚未接入，不影响使用个人 DeepSeek 密钥测试。"</p><label for="account">"公司账号"</label><input id="account" autocomplete="username"/><label for="password">"密码"</label><input id="password" type="password" autocomplete="off"/><div class="row"><button class="primary" on:click=move |_|spawn_local(async move{if let Err(e)=call("company_login",json!({})).await{notice.set(e)}})>"登录"</button></div></Show>
    <Show when=move||panel.get()=="permissions"><h2>"工作区与权限"</h2><p>"Full Access 已启用：Agent 可执行命令、访问网络及读写当前系统用户有权限访问的文件，不再等待操作审批。"</p><p>{move||data.get()["workspace"].as_str().unwrap_or("未选择工作区").to_owned()}</p><p>"此开发预览默认禁用 MCP Registry；本地工具优先。"</p></Show>
    <Show when=move||panel.get()=="network"><h2>"网络连接"</h2><p>"当前构建使用仅直连模式。系统代理、PAC 自动回退和手动代理正在开发，尚不能作为已通过验收的功能。"</p><button on:click=move |_|spawn_local(async move{notice.set(match call("test_api",json!({})).await{Ok(v)=>v.as_str().unwrap_or("").into(),Err(e)=>e})})>"测试 DeepSeek"</button></Show>
    <Show when=move||panel.get()=="agent"><h2>"Agent 与运行环境"</h2><p>"Codewhale 0.9.13 · stdio JSON-RPC · 本地 SQLite 会话"</p><p>{move||format!("共享 Python：{}",data.get()["python"].as_str().unwrap_or("未初始化"))}</p><button on:click=move |_|spawn_local(async move{notice.set("正在初始化，请稍候…".into());notice.set(match call("initialize_python",json!({})).await{Ok(v)=>v.as_str().unwrap_or("").into(),Err(e)=>e})})>"初始化共享 Python"</button><p>"当前版本会话可恢复 Agent 历史；旧版本仅保存的文字记录不含 Runtime 推理上下文。"</p><button on:click=move |_|action("restart",json!({}),error)>"重启 Agent"</button></Show>
    <Show when=move||!notice.get().is_empty()><p role="status">{move||notice.get()}</p></Show><div class="row"><button on:click=move |_|{panel.set(String::new());secret.set(String::new());notice.set(String::new());}>"关闭"</button></div>
    </section></div></Show>
    <Show when=move||data.get()["pending_workspace"].is_string()><div class="overlay"><section class="dialog" role="alertdialog" aria-modal="true"><h2>"切换工作区？"</h2><p>"当前任务正在执行。切换将停止任务并保存已有消息。"</p><div class="row"><button on:click=move |_|action("resolve_switch",json!({"allow":false}),error)>"取消"</button><button on:click=move |_|action("resolve_switch",json!({"allow":true}),error)>"停止并切换"</button></div></section></div></Show>
    }
}
#[wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(App)
}
