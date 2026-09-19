# Codewhale-MedSci Desktop GUI 内容呈现规范

文档名：`spec-GUI.md`  
状态：Draft v1  
适用分支：`desktop/preview`  
适用工程：`apps/desktop/ui`  
设计目标：在保留当前明亮、简约、克制视觉语言的前提下，引入CodeWhale TUI已经验证过的“结构化Agent会话”信息层级，使用户能快速区分回答、思考、工具执行、文件变化、错误与系统状态，同时避免把桌面GUI做成终端模拟器或高密度开发者控制台。

---

## 1.现状判断

当前GUI已经形成明确的视觉基线，不应重做整体框架。

现有设计的主要特征：

- 单窗口、无侧边栏。
- 背景图覆盖整个窗口，内容区保持通透。
- 主正文宽度约800px，居中排布。
- 用户消息使用浅蓝半透明气泡，右对齐。
- Assistant正文直接绘制在背景上，不使用大面积卡片。
- Composer使用高透明度白色圆角表面与Blur。
- 底部状态项弱化为小号文字按钮或Meter。
- 工具事件目前直接作为`.tool-event`文本块呈现。
- Reasoning已经作为独立`role="reasoning"`消息进入时间线，并显示“思考”标签。
- 所有主要正文目前基本使用相同的黑色，Reasoning与Assistant在颜色上几乎没有差别。
- GUI当前将Runtime的`messages`与部分`cards`按`sequence`合并为单一Timeline，这是正确方向。

当前最大问题不是布局，而是不同Agent语义在视觉上“长得太像”。

目标不是增加更多卡片、边框和标签，而是通过文字色、低强度背景、细轨、间距和折叠策略建立层级。

---

## 2.设计原则

### 2.1保持当前视觉语言

必须保留：

- 明亮背景。
- 大面积留白。
- Assistant回答不包裹在聊天气泡中。
- 用户消息仍是唯一明显的消息气泡。
- Composer仍是页面上最明显的固定表面。
- 不增加持续存在的右侧栏。
- 不复制TUI的ANSI字符风格、终端边框和高密度状态条。
- 不使用大量彩色Badge。
- 不用颜色填满整张工具卡来表达状态。

### 2.2语义优先于“消息角色”

桌面GUI不应只有：

```rust
User
Assistant
```

至少需要区分：

```rust
UserMessage
AssistantAnswer
Reasoning
ToolActivity
FileMutation
Review
SubAgent
Approval
UserInputRequest
SystemNotice
Error
```

同一Agent turn可以包含多个连续语义块。

### 2.3颜色只负责建立层级，不负责制造装饰

颜色使用原则：

1.最终回答是最深、最清晰的内容。
2.思考内容明显弱于最终回答。
3.工具活动弱于思考和回答，但其失败状态需要提升可见性。
4.系统提示比工具活动更弱。
5.错误是少数可以使用暖色强调的内容。
6.状态必须同时依赖文字、图标或结构，不允许仅用颜色表达。
7.所有色彩都应低饱和，适应当前背景图。

### 2.4主Timeline展示“发生了什么”，详情层展示“全部证据”

借鉴CodeWhale TUI：

- Timeline展示摘要。
- 长stdout、完整tool arguments、完整diff、完整错误堆栈不默认展开。
- 用户需要时再进入Detail。
- 失败信息比成功信息拥有更高的信息预算。
- 正常成功的连续工具活动允许聚合。
- 正在运行、失败、文件修改、Review等重要事件不得被静默折叠。

---

## 3.总体信息架构

主界面继续保持四层：

```text
Titlebar
↓
Conversation Timeline
↓
Composer
↓
Compact Footer
```

不新增永久Workbar。

对于CodeWhale TUI中Work Surface的设计，只借鉴其“长期工作状态不应消失”的思想，不直接复制布局。

桌面版可将长期状态通过以下方式表达：

- 当前正在执行的工具：保留在Timeline中并实时更新状态。
- 当前运行中的子Agent：以一个可展开的Activity Group留在Timeline。
- 当前计划/任务清单：只有Agent实际产生Plan时显示轻量Plan Block。
- 已完成的任务不立即从Timeline删除。
- 后续如需更完整的Tasks/Agents视图，再使用临时Drawer或Modal，不占用常驻空间。

---

## 4.核心颜色系统

当前CSS中：

```css
--text:#000;
--muted:#000;
--accent:#000;
```

过度统一，应改为语义Token。

推荐：

```css
:root {
  --text-primary: #111418;
  --text-secondary: #4f5963;
  --text-tertiary: #74808a;

  --text-answer: #111418;
  --text-reasoning: #596773;
  --text-tool: #465866;
  --text-system: #78838c;

  --text-success: #456b58;
  --text-warning: #8a652f;
  --text-error: #a54a43;

  --surface-user: #e1edf6f5;
  --surface-subtle: #ffffff38;
  --surface-hover: #ffffff55;
  --surface-error: #fff2eed9;

  --rail-reasoning: #7f98a8;
  --rail-tool: #91a3ad;
  --rail-file: #809b8d;
  --rail-error: #c47c74;

  --line-subtle: #ffffff70;
}
```

说明：

- 不建议使用纯黑`#000`作为全部正文。`#111418`在浅色背景上仍接近黑色，但层级更柔和。
- Reasoning采用冷灰蓝，不使用紫色。紫色会使“思考”变成强品牌视觉，不符合克制目标。
- Tool使用略偏蓝灰，比Reasoning更“机械”。
- 成功状态仅使用暗绿文字或小图标，不给整块绿色背景。
- Warning使用暗金棕，不使用鲜黄。
- Error沿用暖红，但降低饱和度。
- 所有颜色都必须在实际背景图最亮区域验证对比度。

---

## 5.内容类型与视觉规则

## 5.1用户消息

维持现状。

视觉：

- 右对齐。
- 最大宽度78%，窄屏86%。
- 浅蓝半透明气泡。
- 正文字色`--text-primary`。
- 不显示“你”Role Label。
- 附件显示在气泡内部或气泡顶部。

不需要引入TUI式rail。

---

## 5.2最终回答

这是主Timeline视觉权重最高的Agent内容。

视觉：

```css
.answer {
  color: var(--text-answer);
  font-size: 16px;
  line-height: 1.85;
}
```

规则：

- 保持无卡片、无背景。
- 默认不显示“CODEWHALE”字样。
- 不显示“回答”Label。
- Markdown正文直接呈现。
- 标题、列表、表格、引用和代码继续使用现有Markdown Renderer。
- 正文建议设置可读宽度上限，不必占满超宽窗口。
- 保留现有轻微白色text-shadow，但需弱于当前实现，避免文字边缘发虚。

推荐：

```css
.answer {
  text-shadow: 0 1px 1px rgba(255,255,255,.55);
}
```

最终回答必须始终比Reasoning、Tool、System更深。

---

## 5.3思考/Reasoning

Reasoning必须与最终回答有清晰但克制的区别。

### 默认形态

```text
思考了 8 秒
│ 正在分析文档结构并检查现有文件……
│ 需要先确认数据表中的字段关系……
```

桌面GUI不需要复制TUI的`╎`字符，但可以用真正的CSS左轨。

推荐：

```css
.reasoning {
  color: var(--text-reasoning);
  font-size: 15px;
  line-height: 1.72;
  margin: 8px 0 18px;
  padding-left: 14px;
  border-left: 2px solid color-mix(in srgb, var(--rail-reasoning) 55%, transparent);
}

.reasoning-header {
  color: var(--text-tertiary);
  font-size: 13px;
  font-weight: 500;
  margin-bottom: 5px;
}
```

### 状态

流式思考：

```text
思考中…
│ …
```

思考完成：

```text
思考了 8 秒
│ …
```

如果Runtime没有duration，可显示：

```text
思考
```

### 折叠规则

借鉴CodeWhale TUI：

- Reasoning运行中显示最多约8至12行。
- Reasoning完成后默认折叠为2至4行预览。
- 鼠标点击“思考了8秒”展开全文。
- 再次点击收起。
- 如果Reasoning总长度很短，不显示折叠按钮。
- 展开/收起不得导致页面突然跳到底部。
- 用户已经向上浏览时，新Reasoning token不得抢走滚动位置。

### 禁止

- 不用灰色半透明大卡片包裹整段Reasoning。
- 不使用紫色渐变。
- 不使用脑图标、星光动画等装饰。
- 不把Reasoning与最终答案合并成同一Markdown块。

---

## 5.4普通工具活动

工具调用不再全部使用和正文相同的黑色。

默认表现应类似“活动日志”，不是聊天卡片。

推荐：

```text
读取文件
report.docx
```

或：

```text
运行命令 · 已完成
python create_report.py
```

结构：

```html
<section class="activity tool">
  <div class="activity-header">…</div>
  <div class="activity-summary">…</div>
  <div class="activity-preview">…</div>
</section>
```

视觉：

```css
.activity {
  color: var(--text-tool);
  margin: 8px 0 14px;
  padding-left: 14px;
  border-left: 1px solid color-mix(in srgb, var(--rail-tool) 55%, transparent);
}

.activity-header {
  font-size: 14px;
  font-weight: 500;
}

.activity-summary {
  color: var(--text-secondary);
  font-size: 14px;
}

.activity-meta {
  color: var(--text-tertiary);
  font-size: 12px;
}
```

不要默认给工具事件圆角白卡。

### 工具状态文字

必须至少支持：

- 执行中
- 已完成
- 有警告
- 失败
- 已停止

状态使用“文字+轻微颜色”：

```text
运行命令 · 执行中
运行命令 · 已完成
运行命令 · 失败
```

不使用只有小圆点颜色而没有文字的状态。

---

## 5.5Shell/命令执行

借鉴TUI成功输出压缩策略。

成功：

```text
运行命令 · 已完成
python create_report.py
3 files written
```

默认只显示少量输出预览。

建议：

- 成功：最多6行。
- Warning：最多10行。
- 失败：最多20行或显示完整核心错误。
- 完整输出通过“查看输出”展开或详情层查看。

如果运行超过1秒，可在meta里显示耗时：

```text
3.2秒
```

正在执行时：

```text
运行命令 · 执行中
python create_report.py
```

避免持续滚动大量stdout冲刷Timeline。

---

## 5.6连续工具活动聚合

借鉴CodeWhale TUI的Tool Run Grouping，但使用GUI语言。

当连续出现3个及以上低风险、成功的读/搜类工具时，合并为：

```text
检查了 7 项内容
6项完成 · 1项仍在进行
```

展开后：

```text
✓ 读取 report.docx
✓ 读取 data.xlsx
✓ 搜索 “Methods”
… 
```

规则：

可聚合：

- 文件读取。
- 目录扫描。
- 搜索。
- 元数据查询。
- 已成功的低风险MCP读取。

不得自动吞并：

- 失败工具。
- 正在等待用户的工具。
- Shell命令。
- 文件写入。
- 删除或覆盖。
- Review。
- Plan。
- 子Agent。
- 明显包含重要输出的工具。

聚合块默认仅使用文字和细轨，不使用厚重Accordion卡片。

---

## 5.7并行探索

借鉴TUI`ExploringCell`的一眼可读性，但不要复制点阵字符。

GUI建议：

```text
正在检查 5 个文件
3完成 · 1进行中 · 1失败
```

下方最多显示3个最近或最重要条目。

也可使用极细的状态符号：

```text
✓ report.docx
✓ data.xlsx
○ methods.md
× references.csv
```

颜色必须克制：

- ✓可用`--text-success`
- ○使用`--text-tertiary`
- ×使用`--text-error`

不要显示彩色进度条。

---

## 5.8文件修改

文件变更是高价值信息，不应和普通工具混为一体。

推荐：

```text
修改文件 · 已完成
report.docx
更新 1 个文件
```

对于源码或文本类文件：

```text
修改文件 · 已完成
analysis.py
+28 −4
查看更改
```

对于DOCX/PPTX/XLSX/PDF：

```text
生成文件 · 已完成
研究报告.docx
打开文件
```

规则：

- 显示最终文件名。
- 显示Created/Updated/Renamed/Deleted语义。
- 源码文件有diff时，可显示`+N −N`。
- Office/PDF文件不要伪造文本diff。
- “查看更改”进入Detail或文件预览。
- 删除操作即使成功也保持较高可见性。

颜色：

- 文件活动正文可稍偏绿灰，但不可变成绿色卡片。
- 删除与失败使用暖红文字。

---

## 5.9Review

如果Agent执行Review，应使用独立语义块。

示例：

```text
检查结果
发现 3 个问题

高 · Results表格缺少单位
中 · Methods中的样本量与摘要不一致
低 · 参考文献格式不统一
```

不要直接打印结构化JSON。

Severity颜色仅用于Severity文字，不染整行。

---

## 5.10子Agent

如果后续暴露CodeWhale子Agent：

```text
子任务 · 文献检查
已完成 · 12秒
```

运行中：

```text
子任务 · 文献检查
正在工作…
```

默认不将子Agent完整Transcript嵌套在主Timeline里。

点击后使用Modal/Drawer显示详情。

这对应TUI中Work Surface“主视图看到状态，详情进入另一个world”的设计。

---

## 5.11审批

当前`EventCard`已有“等待审批”。

审批是少数应当拥有明确Surface的内容，因为它要求用户行动。

建议保留轻量白色半透明卡，但不要与普通工具共用视觉。

```text
需要你的确认

Agent准备覆盖：
研究报告.docx

[拒绝] [允许一次]
```

视觉：

```css
.approval {
  background: rgba(255,255,255,.62);
  border: 1px solid rgba(255,255,255,.72);
  border-radius: 14px;
  padding: 14px 16px;
}
```

审批卡必须比普通Tool Activity明显，但仍低于Composer。

---

## 5.12用户输入请求

与审批类似，属于交互性内容，可以使用轻量Surface。

多选项优先渲染成真正的选项按钮，而不是当前的：

```text
option1 / option2 / option3
[input]
```

推荐：

```text
Agent需要补充信息

统计分析应使用哪一个终点？

○ 总生存期
○ 无进展生存期
○ 其他…
```

---

## 5.13System Notice

普通系统事件使用最弱层级：

```text
上下文已整理
会话已恢复
Agent已重启
```

视觉：

```css
.system-notice {
  color: var(--text-system);
  font-size: 13px;
}
```

除非必须，否则不要使用背景。

---

## 5.14Error

Error保持当前暖色浅背景，但应分为：

- Inline Tool Error
- Turn Error
- Fatal Agent Error

普通工具失败：

```text
读取文件 · 失败
无法打开“data.xlsx”：文件被其他程序占用
```

不一定需要大面积`.error`Surface。

只有会导致当前turn无法继续的错误才使用现有`.error`块。

Fatal Agent Error可继续提供“重启Agent”。

---

## 6.时间线节奏

CodeWhale TUI值得借鉴的一点是：不同内容之间不是统一padding，而是根据语义确定间距。

桌面版建议：

|前一块→后一块|间距|
|---|---:|
|Reasoning→Answer|10px|
|Answer内部段落|8px|
|Tool→Tool|6px|
|Tool Group内部|4px|
|Tool→Answer|14px|
|User→Agent turn|18px|
|完整Turn之间|28px|
|System Notice→普通内容|10px|

这样可以让“同一Agent turn”视觉上连成一个整体，而不是每个HistoryCell都像一条独立聊天消息。

---

## 7.同一Turn的视觉组织

推荐把一次用户输入后的所有Agent活动视为一个Turn：

```text
[用户消息]

思考
│ ...

读取文件 · 已完成
运行命令 · 已完成
修改文件 · 已完成

最终回答……
```

不要给每个块重复显示“CODEWHALE”。

Assistant身份由上下文自然表达。

当用户下一条消息出现时，再产生明显Turn间距。

---

## 8.流式体验

### 8.1Sticky Tail

借鉴CodeWhale TUI Live Transcript：

- 用户位于Timeline底部时，流式内容自动跟随。
- 用户只要主动向上滚动，就暂停自动跟随。
- 页面底部显示一个小型“回到最新”按钮。
- 用户回到底部后自动恢复sticky tail。
- 不允许每个token都强制`scrollIntoView`。

### 8.2Reasoning和Answer独立Streaming

状态转换：

```text
Reasoning streaming
↓
Reasoning completed
↓
Answer streaming
↓
Answer completed
```

禁止把Reasoning完成后复制到Answer区域。

### 8.3流式光标

不建议使用明显打字机动画。

可以只在当前Assistant Answer最后增加非常轻的caret：

```css
.streaming-caret {
  width: 2px;
  opacity: .55;
}
```

Reasoning可以使用更弱的caret。

---

## 9.展开与详情

统一定义两个层级：

### Inline Expand

用于：

- Reasoning全文。
- Tool stdout。
- 聚合工具列表。
- 小型diff。

操作：

```text
展开
收起
```

文字按钮默认只在hover或内容被截断时出现。

### Detail View

用于：

- 完整Tool input/output。
- 完整Shell输出。
- 大型diff。
- Review原始数据。
- 子Agent Transcript。
- Turn Inspector。

Desktop建议采用居中的大Modal或右侧临时Drawer。

当前产品无侧边栏，因此Detail Drawer必须是临时的，不可成为常驻结构。

---

## 10.复制语义

必须借鉴CodeWhale TUI“Presentation≠Clipboard Content”的思想。

### Assistant Answer

复制原始Markdown，而不是复制渲染后的HTML文字布局。

### Reasoning

默认单独复制Reasoning正文，不包含“思考了8秒”等UI标签。

### Tool

“复制”应复制结构化可读文本，例如：

```text
运行命令
python create_report.py

输出：
...
```

不要复制CSS装饰或截断标记。

### Timeline多块选择

后续如实现多块选择，应按语义序列化，而不是复制DOM视觉文本。

---

## 11.Markdown呈现

继续使用现有安全流程：

```text
pulldown-cmark
→ HTML
→ ammonia sanitize
```

视觉调整：

- `h1-h3`不要过大。
- 表格继续允许横向滚动。
- Code Block使用轻透明Surface。
- Inline code可使用比正文略深的半透明白底。
- Blockquote使用细左轨，颜色与Reasoning轨不同。
- Link建议使用低饱和蓝灰，而不是当前`--accent:#000`。

推荐：

```css
a {
  color: #315f78;
  text-decoration-color: rgba(49,95,120,.35);
}
```

---

## 12.代码块

当前：

```css
pre {
  background:#ffffff65;
}
```

方向正确。

建议增加顶部语言/复制操作，但默认隐藏操作按钮：

```text
python                                  Copy
────────────────────────────────────────────
...
```

只有hover时显示`Copy`。

不要引入IDE式深色代码块，避免破坏明亮统一风格。

Windows目前使用宋体覆盖Code。这会降低代码可读性。建议Windows正文可继续按产品要求使用宋体，但代码恢复等宽字体：

```css
.windows pre,
.windows code {
  font-family: "Cascadia Mono", "Consolas", ui-monospace, monospace;
}
```

除非“所有Windows文字必须宋体”是不可变产品要求。

---

## 13.Footer

当前Footer已经足够轻量，不建议引入TUI两层状态条。

保留：

```text
模型 · 会话 · 计费 · 上下文                     API · 登录
```

但建议：

- 模型、计费、上下文保持现有轻量级。
- Agent执行中不需要持续显示“thinking/tool”等phase，因为Timeline已经明确表达。
- Footer只保留全局状态，不重复Timeline状态。
- 如果未来加入Provider或权限，应优先放在设置或Hover信息中，避免Footer持续膨胀。

这对应TUI的“One owner per fact”原则：同一个事实只在一个地方长期展示。

---

## 14.建议的数据模型

目前`timeline()`将`messages`和少量`cards`直接合并，逻辑仍偏“后端事件→UI”。

建议增加一层UI语义模型。

```rust
enum TimelineItem {
    UserMessage(UserMessageView),
    Answer(AnswerView),
    Reasoning(ReasoningView),
    Activity(ActivityView),
    ActivityGroup(ActivityGroupView),
    FileMutation(FileMutationView),
    Review(ReviewView),
    SubAgent(SubAgentView),
    Approval(ApprovalView),
    UserInput(UserInputView),
    SystemNotice(NoticeView),
    Error(ErrorView),
}
```

工具活动：

```rust
enum ActivityKind {
    Shell,
    Read,
    Search,
    Web,
    Mcp,
    Python,
    Generic,
}
```

状态：

```rust
enum ActivityStatus {
    Running,
    Success,
    Warning,
    Failed,
    Interrupted,
}
```

Reasoning：

```rust
struct ReasoningView {
    text: String,
    streaming: bool,
    duration_ms: Option<u64>,
    expanded: bool,
}
```

文件变更：

```rust
struct FileMutationView {
    path: String,
    operation: FileOperation,
    added: Option<u32>,
    deleted: Option<u32>,
    preview: Option<String>,
    detail_id: Option<String>,
}
```

关键原则：

Runtime事件模型和UI视觉模型不要成为同一个类型。

---

## 15.现有代码的具体修改建议

### `apps/desktop/ui/src/lib.rs`

当前：

```rust
let user=entry["role"]=="user";
let reasoning=entry["role"]=="reasoning";
```

应逐步替换为：

```rust
match classify_timeline_item(&entry) {
    TimelineItem::UserMessage(..) => ...
    TimelineItem::Reasoning(..) => ...
    TimelineItem::Answer(..) => ...
    TimelineItem::Activity(..) => ...
}
```

短期可先不重构Runtime协议，只在UI层新增：

```rust
fn classify_timeline_item(entry: &Value) -> TimelineKind
```

### `EventCard`

当前所有工具事件都进入：

```html
<section class="tool-event">
```

需要拆为：

- `ToolActivity`
- `ApprovalCard`
- `UserInputCard`
- 后续`FileMutation`
- 后续`Review`

审批和工具日志不能继续共用同一个视觉组件。

### Reasoning

当前：

```rust
class="assistant reasoning"
```

应去掉`assistant`视觉耦合：

```rust
class="reasoning"
```

并增加：

- Header。
- Duration。
- Expand/collapse。
- Streaming状态。
- 左侧细轨。

### Answer

当前：

```rust
class="assistant"
```

可改为：

```rust
class="answer"
```

UI语义更加明确。

### `.role`

Assistant回答不再显示Role。

Reasoning仅显示：

```text
思考
思考中…
思考了8秒
```

### `.tool-event`

现有定义：

```css
.tool-event {
  color:var(--text);
}
```

应替换为语义活动样式。

---

## 16.建议的CSS骨架

```css
:root {
  --text-primary:#111418;
  --text-secondary:#4f5963;
  --text-tertiary:#74808a;
  --text-answer:#111418;
  --text-reasoning:#596773;
  --text-tool:#465866;
  --text-system:#78838c;
  --text-success:#456b58;
  --text-warning:#8a652f;
  --text-error:#a54a43;

  --rail-reasoning:#7f98a8;
  --rail-tool:#91a3ad;
  --rail-file:#809b8d;

  --surface-user:#e1edf6f5;
  --surface-interactive:#ffffff9c;
}

.answer {
  color:var(--text-answer);
  padding:4px 0 20px;
  margin-bottom:10px;
  line-height:1.85;
}

.reasoning {
  color:var(--text-reasoning);
  font-size:15px;
  line-height:1.72;
  margin:6px 0 14px;
  padding-left:14px;
  border-left:2px solid color-mix(
    in srgb,
    var(--rail-reasoning) 50%,
    transparent
  );
}

.reasoning-header {
  color:var(--text-tertiary);
  font-size:13px;
  margin-bottom:5px;
}

.activity {
  color:var(--text-tool);
  font-size:14px;
  line-height:1.65;
  margin:6px 0 10px;
  padding-left:14px;
  border-left:1px solid color-mix(
    in srgb,
    var(--rail-tool) 50%,
    transparent
  );
}

.activity[data-status="failed"] {
  color:var(--text-error);
}

.activity[data-status="warning"] {
  color:var(--text-warning);
}

.activity-meta {
  color:var(--text-tertiary);
  font-size:12px;
}

.system-notice {
  color:var(--text-system);
  font-size:13px;
  margin:8px 0;
}

.inline-action {
  color:var(--text-tertiary);
  border:0;
  padding:2px 0;
  font-size:12px;
}

.inline-action:hover {
  color:var(--text-primary);
  background:transparent;
}
```

---

## 17.不要借鉴TUI的部分

以下TUI设计不适合当前GUI：

- 不复制ASCII/Unicode rail glyph。
- 不复制底部两行Posture Bar+Metrics Line。
- 不复制大量快捷键提示。
- 不把Tool Detail做成Pager。
- 不把Tasks/Agents作为常驻Bottom Workbar。
- 不显示Provider、权限、Agent状态等所有技术事实。
- 不使用终端式“●/◐/✕”作为唯一状态语言。
- 不在每个工具块中显示原始工具名和JSON。
- 不为了“可审计”而默认展开全部中间过程。
- 不把Runtime内部handoff、turn_meta等内容呈现给普通用户。

要借鉴的是信息架构，不是终端皮肤。

---

## 18.优先实施顺序

### P0：必须完成

1.建立新的语义颜色Token。
2.Answer改为`--text-answer`。
3.Reasoning使用`--text-reasoning`、左轨和Header。
4.Tool Activity使用`--text-tool`。
5.Error/Warning/Success获得独立低饱和色。
6.去掉Reasoning对`.assistant`样式的继承。
7.普通Tool与Approval/User Input拆组件。
8.成功Tool输出默认截断，失败输出给予更高预算。

完成P0后，即使没有复杂Activity Group，界面信息层级也会明显改善。

### P1：推荐完成

1.Reasoning折叠与“思考了N秒”。
2.Tool output展开/收起。
3.连续Read/Search聚合。
4.Sticky Tail与“回到最新”。
5.File Mutation专用呈现。
6.代码块Hover Copy。
7.复制Markdown源内容，而不是DOM渲染文本。

### P2：后续增强

1.SubAgent详情。
2.Review专用UI。
3.Plan/Checklist。
4.Turn Inspector。
5.完整Tool Detail Drawer。
6.多块选择与结构化复制。

---

## 19.验收标准

### 视觉

- 用户消息、思考、回答、工具、系统、错误六类内容在不阅读文字的情况下也能通过颜色和结构区分。
- 最终回答始终具有最高正文视觉权重。
- Reasoning明显弱于Answer，但在普通背景图上仍可舒适阅读。
- Tool Activity不抢正文注意力。
- 页面不存在连续多张白色大卡。
- 页面仍然保持当前明亮、通透、克制风格。

### 行为

- Reasoning与Answer分别流式更新。
- Reasoning完成后可折叠。
- 长成功工具输出默认不超过约6行。
- 失败工具不会被聚合隐藏。
- 用户上滚后流式输出不强制抢滚动位置。
- Approval与User Input在视觉上明确可交互。
- 完整输出仍有可发现的查看入口。

### 语义

- GUI不直接依赖字符串判断工具显示样式。
- Runtime事件先被映射成UI语义类型，再渲染。
- 内部runtime metadata不进入可见Transcript。
- 同一事实不在Timeline和Footer重复长期展示。

### 可访问性

- 颜色不是唯一状态信号。
- Reasoning、Tool、Error文字在最亮背景区域仍符合可读性要求。
- 所有展开/收起可通过键盘操作。
- `prefers-reduced-motion`下不依赖动画表达状态。
- 屏幕阅读器可识别Approval、Question和Error的语义。

---

## 20.最终建议的页面气质

目标不是把CodeWhale TUI“搬进GUI”。

目标应该是：

```text
用户消息
        ↓
低调的思考过程
        ↓
低调的Agent活动轨迹
        ↓
清晰、完整、权重最高的最终回答
```

在普通对话里，用户主要看到用户消息和回答。

只有Agent真正执行了文件、命令、搜索、Review或子任务时，活动轨迹才自然出现在两者之间。

因此，最终界面应更接近“一个有执行能力的干净文档式AI界面”，而不是“一个带背景图的终端日志”，也不是“每一步都装进卡片的Dashboard”。

CodeWhale TUI最值得保留的三个思想是：

1.不同Agent语义必须是不同UI对象。
2.主Timeline展示摘要，完整证据按需展开。
3.成功过程应安静，失败和需要用户行动的内容才提高视觉权重。

这三个原则与当前`desktop/preview`的明亮、简约、克制设计并不冲突，反而可以在几乎不增加视觉噪声的情况下显著提升Agent会话的可读性。
