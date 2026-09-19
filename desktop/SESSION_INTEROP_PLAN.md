# Codewhale-MedSci 原生会话能力复用计划（Windows GUI）

状态：实施中。2026-09-19 基于当前工作区源码检查编写；本文不表示互通已经实现，也不执行迁移。

本次执行范围（用户于 2026-09-19 确认）：先完成 Windows 平台。macOS 的实机往返、打包与安装验收留待后续，不属于本次完成门槛；公共原生代码仍保持跨平台实现。

## 1. 产品目标与完成定义

2026-09-20 用户明确收敛范围：实际用户只使用 GUI，不使用 TUI。目标是让 GUI 复用 Codewhale 与 Codewhale-TUI 的原生能力，而不是交付双界面互通产品。此范围替代初版中的 GUI／TUI 多轮往返要求。

目标场景：

1. GUI 在目录 A 新建会话 S，使用原生 Engine 执行并保存。
2. 关闭并重启 GUI 后，原生列表中仍是同一个 S；选择历史并继续时不丢失、不重复消息。
3. 停止、切换会话及关闭窗口都经过原生停止／保存边界；失败明确展示，不能假装保存成功。
4. 相同目录可以同时打开多个不同会话。同一会话沿用原生执行所有权，其他窗口仍可只读浏览。
5. Windows 用户级安装包含匹配的 GUI 和 sidecar，仅为当前用户注册右键菜单。

TUI 的交互、会话切换、显式 fork、多轮宿主往返以及独立 Terminal CLI 不再作为本次验收门槛，也不为它们继续扩展协议、计费账本或兼容层。已完成的原生数据完整性修复保留；其测试是补充证据，不扩大产品范围。macOS 仍不属于本次范围。

## 2. 设计原则

- 应用默认采用用户级安装。GUI 与 sidecar 必须在同一普通用户权限下运行，不请求 UAC／sudo，不自动修改目录权限、所有者或全机配置，不接管其他用户的会话。
- Windows 右键菜单仅注册到当前用户的 `HKCU\Software\Classes`，安装器保持 `currentUser`，不写全机菜单或要求管理员运行。遇到无权访问的显式 home 或 Runtime 存储时明确报错，不自动提权。
- 特殊测试所需的符号链接权限等环境条件单独记录。管理员模式下的成功结果不能替代普通用户的运行及安装验收；模型工具的 Full Access 策略与操作系统管理员权限分别处理。
- `SavedSession` / `SessionMetadata.id` 是 GUI 复用的原生用户会话身份，`SessionManager` 是其持久化入口。
- `RuntimeThreadManager`、`RuntimeStoreBinding` 和 `Engine::run_turn` 继续承担原有职责。
- Runtime thread ID 可以是原生执行内部的不同身份；不得把它伪装成 SavedSession ID。允许原生生命周期需要的内部重新绑定，不允许因此增加用户可见的会话副本。
- 不新增 GUI 会话数据库、JSON 对话副本、session ID 映射表、同步服务或第二套 Runtime。
- 不从 GUI 的 Message / Snapshot / Markdown 重建历史，不以文本导出冒充完整续接。
- 不修改 TUI 的既有磁盘格式来迎合 GUI。确需补能力时，修改原生入口，GUI 只增加参数传递和结果展示。
- 共享原生 SavedSession 与 Runtime 的既有配合关系，不把二者合并成一份自创记录。会话快照与执行记录各有原生职责，保持原生校验。
- 新提取的公共函数必须同时替换现有重复调用；不留下“新框架 + 旧实现 + 以后迁移”的双轨结构。

## 3. 已确认的源码事实

| 已有能力／现状 | 位置与符号 | 对计划的影响 |
| --- | --- | --- |
| 原生会话根目录解析、保存、加载、原子写入与生命周期合并 | `crates/tui/src/session_manager.rs`：`default_sessions_dir`、`SessionManager`、`SavedSession`、`merge_persisted_lifecycle` | 直接复用，不在 GUI 重新计算原生路径或读写格式 |
| 原生统一列表投影 | `crates/tui/src/session_projection.rs`：`SessionQuery`、`SessionSummary` | GUI 的工作区筛选、排序、归档语义采用同一实现 |
| 原生会话 HTTP 接口 | `crates/tui/src/runtime_api.rs`、`runtime_api/sessions.rs`：GET/POST/PUT `/v1/sessions`、GET `/v1/sessions/summary`、POST `/v1/sessions/{id}/resume-thread` | 复用其领域行为；需要 stdio 支持时只做薄传输适配 |
| 从引擎取完整快照并保存 | `save_current_session`、`EngineHandle::get_session_snapshot` | 正常保存优先使用该路径，不用 GUI 消息列表 |
| 从已持久化 Runtime 记录创建保存会话 | `create_session_from_thread` | 可作为旧历史导入基础，但它当前生成新 ID，不适合每轮自动调用 |
| TUI 保存元数据与 Runtime 所有权 | `crates/tui/src/tui/ui/frame.rs`：`build_session_snapshot` | GUI 保存也必须保留改名、归档、创建时间、路由与正确存储绑定 |
| 会话级 Runtime 目录与所有权 | `RuntimeThreadManagerConfig::for_session`、`RuntimeThreadManager::open_for_session`、`RuntimeStoreBinding`、`RuntimeProcessOwnerLock` | 复用每 session 的隔离方式，不按目录去重，不删除独占锁 |
| 保存会话与 Runtime 前缀校验 | `ThreadRecord.session_id`、`SavedSessionCheckpoint`、`set_thread_session_checkpoint`、`saved_session_prefix` | 复用恢复与前缀校验，不能跳过摘要校验 |
| 原生引擎可以接收宿主 session ID | `crates/tui/src/core/engine.rs`：`EngineConfig.session_id` | 不需要 GUI 发明另一个对话身份体系 |
| 当前 GUI 覆盖原生 home 并指定桌面 Runtime 路径 | `apps/desktop/src-tauri/src/agent.rs`、`main.rs` | 改为复用原生路径解析和会话库，保留 GUI 私有配置 |
| 本次多实例新增桌面历史扫描和存储占用保护 | `/v1/desktop/history`、`desktop_history`、`claim_runtime`、`agent/tasks/desktop/session-*` | 是待迁出的过渡实现，不是互通目标架构 |

### 不能直接“把现有接口连起来”就宣布完成的缺口

1. `save_current_session` 支持传入现有 session ID，但当前读取到的实现未像 TUI 保存路径一样显式写入 `metadata.runtime_store`；现有会话更新中的所有权、生命周期及补充字段需要逐项核实。
2. `resume_session_thread` 当前创建新的 Runtime thread；需要核实并补齐“同一 SavedSession 的后续执行”语义，不能让重新导入变成隐式 fork。
3. SavedSession 与 Runtime checkpoint 分歧时，保留 `saved_session_prefix` 的摘要校验及原生恢复规则；不为本次继续扩展 TUI 修改历史后的交接场景。
4. `session_checkpoint_guard` 的具体覆盖范围、磁盘写入锁和 Runtime 所有权锁不能混为一谈。原子写入只保证文件完整，不保证两个宿主不会先后覆盖彼此的有效历史。
5. 使用不同 `CODEWHALE_HOME` 的两个独立运行环境不会自动互通。需要共享同一个原生 home，不能靠相同 cwd 猜测历史库。

## 4. 目标接入方式

### 4.1 统一原生会话 home，保留 GUI 设置隔离

- GUI sidecar 使用 Codewhale 原生路径解析规则：普通安装使用当前用户的原生默认 home；用户显式指定原生 home 时采用该配置。
- 移除为了 GUI 自有历史而强制注入的 `CODEWHALE_HOME=<应用目录>/agent`。
- `env_clear` 后只传递经过原生规则校验的必要配置，不能为了互通把全部环境变量透传给 sidecar。
- GUI 自己的 UI 设置、系统凭据入口、私有临时 `agent.toml`、WebView 数据和共享 Python 仍留在应用目录。
- 审计所有原生 home 相关的写入消费者，确认 GUI 的临时路由配置不会覆盖用户 TUI 的配置、密钥或默认模型。继续通过明确的私有 `--config` 传入 GUI 配置。
- GUI 遵循原生显式 home 配置；不扫描全机其他用户或其他历史库。

### 4.2 以原生 session ID 驱动 GUI

- 新建：由原生能力分配／接纳 SavedSession ID，并按原生 `for_session` 规则取得执行存储。第一条用户内容和故障恢复点采用原生生命周期，不因打开历史面板而制造空会话。
- 列表：GUI 读取原生 `SessionSummary`，复用 `SessionQuery` 的工作区过滤、搜索、排序和归档规则。
- 读取：通过原生会话详情／恢复投影，保留已存的思考、工具、引用与附件信息。浏览历史不获取写所有权，不启动模型请求，不依赖 API 密钥。
- 发送：由原生接续入口取得 session 的正确执行所有权及 Runtime 绑定，再进入已有 Engine。
- 保存：使用完整引擎快照，更新同一个 SavedSession ID。GUI 不自行序列化对话内容。
- GUI 状态中的 SavedSession ID 与 Runtime thread ID 明确区分；绑定只使用原生数据关系，不能持久化桌面私有映射。

### 4.3 补齐原生“接续 → 保存 → 释放”契约

在 `session_manager`、`runtime_threads`、`runtime_api/sessions` 现有模块中实现最小补齐；不新建 `session_bridge`、`gui_session_runtime` 等平行模块。

顺序必须明确：

1. 原生只读解析 SavedSession，校验版本、内容和工作区。
2. 在读取会被执行的最终快照前取得原生执行所有权，再次读取并校验绑定。
3. 检查 `RuntimeStoreBinding` 与实际 store owner 是否一致；保留路径边界、符号链接和缺失存储恢复规则。
4. 根据已有 `ThreadRecord.session_id` 和 checkpoint 选择合法执行上下文。
5. GUI 重启恢复时，在取得所有权后读取最新 SavedSession，由原生逻辑恢复执行前缀／checkpoint；已保存内容不得重复追加或丢失。
6. 对无法证明关系的历史分歧沿用原生错误，不静默覆盖或自动 fork。本次不新增 GUI fork 功能。
7. 保存成功后才能标记可供另一个宿主接续；保存失败时保留原生恢复证据，不显示“已保存”。
8. 先停止并排空当前执行、完成保存，再释放所有权及子进程。不能通过杀死其他宿主抢占。

核实可直接复用的原生所有权与保存入口，GUI 路径的必要缺口在原生处补齐。不得另加 GUI 独占锁替代原生执行所有权。

正常轮次结束、用户停止、切换会话、关闭窗口以及崩溃恢复都需要有明确持久化边界。不能只在正常退出时导出一次历史。保存时检查任务是否已到原生允许的稳定边界，不能把仍在执行的历史伪装成完整快照。

### 4.4 保真范围

逐项对照原生 SavedSession 和快照，而不是只核对可见文本：

- 用户／助手／工具消息、工具调用 ID 与结果配对、思考块、附件与文件引用。
- 模型、provider 身份、命名路由、reasoning 选择、系统提示与现有上下文恢复信息。
- 用量／费用收据、未知计费标记、压缩边界、消息顺序、原生支持的消息时间信息。
- 原生保存的目标、工作状态、离线队列等：沿用原生的可恢复语义，不由 GUI 自行推断恢复。
- 标题、归档、创建时间和 fork 来源不能被自动保存回滚。

GUI 恢复历史不自动升级原有权限，沿用原生权限语义。API 凭据通过 GUI 的配置／凭据机制提供，不写入 SavedSession；缺少原 provider 路由时明确报错，不静默切换服务商。

GUI 的窗口 ID、PID、启动时间不进入系统提示或工具目录。会话固定前缀保持原生规则；确需进入模型上下文的动态事实走原生追加历史路径。

## 5. 旧桌面历史处理与过渡代码退出

旧来源包括应用私有 home 下的 `agent/tasks/runtime` 和 `agent/tasks/desktop/session-*`；原生 TUI 已有的 SavedSession 不是待重写的迁移源。

迁移先只读清点并输出本地报告：工作区、原生 thread ID、可恢复性、是否仍被占用、是否已有 SavedSession 关联。执行阶段不在本文中自动发生。

- 使用已有原生快照／导入导出和 `SessionManager` 能力迁移，保留原始 Runtime 数据。GUI 不解析 turn JSON 重组对话。
- 运行中的 store 不迁移；保持原生所有权锁。
- 幂等性优先使用原生 `ThreadRecord.session_id`、checkpoint 和 SavedSession 既有来源关系。核实保存与回写绑定之间崩溃的处理；需要扩展时只在原生导入事务中补最小能力，不创建 GUI migration 映射数据库。
- 不直接移动 Runtime 目录：`execution_scope` 包含路径相关身份，机械搬目录会破坏绑定。先验证原生迁移／恢复路径；若没有安全迁移能力，保留旧 store，只做经过校验的原生会话接入。
- 一次失败不能破坏已存在的原生会话。部分完成后重跑不重复导入、不覆盖较新的历史。
- 旧记录如果缺少完整快照，按原生恢复规则给出可恢复性结论；不能宣称恢复了源记录中不存在的信息。

新建和日常列表全部切到原生 session 路径后，同一交付中清理：

1. GUI 使用 `/v1/desktop/history` 的调用。
2. `RuntimeThreadManager::desktop_history` 及该专用扫描 HTTP 路由（检查真实消费者后移除）。
3. GUI 正式路径中的 `new_runtime` / `claim_runtime` 和 `desktop-owner.lock`；由原生会话所有权接替。历史锁文件无需破坏性清扫。
4. 新建 `agent/tasks/desktop/session-*` 的生产代码。
5. UI 将 Runtime thread ID 当作用户会话 ID 的逻辑。

保留旧格式接入时，应放在明确的原生兼容入口，并仅用于迁移／读取旧记录；不允许新会话继续写旧桌面体系。只有原生会话锁已经覆盖 GUI 的执行路径后，才能移除 GUI 过渡锁。

## 6. 实施顺序

### P0：核对原生契约并确定最小变更

沿上表读取实际调用链，确认 TUI 的创建、恢复、自动保存、执行所有权和 Engine session ID 流程。记录现有 HTTP/stdio 入口可直接使用的部分及缺口，特别是完整快照字段、stable session ID、绑定保持和重启恢复的 checkpoint。

退出条件：每个目标行为都有原生归属，不以“共享一个目录即可”作为方案。原生 home 配置与 GUI 私有设置边界明确。

### P1：原生接续闭环

在原模块补齐并复用接续、稳定 ID 保存、绑定及所有权契约。现有 HTTP 保存／续接与 TUI 的对应调用采用共同实现；薄 stdio 接口只传递已有类型和错误。

先实现并编译运行，再选择性补保护数据完整性的测试。退出条件是 GUI 使用的原生入口可以重启并续接同一 SavedSession，不依赖 GUI 自制数据。

### P2：GUI 全面接入

修改 `apps/desktop/src-tauri/src/main.rs`、`agent.rs` 和 UI 会话列表：共享原生 home、原生列表、SavedSession 身份、稳定保存、合法接续、占用提示。保留多窗口、私有运行配置、Python 使用锁和本实例进程回收。

退出条件：GUI 新建、历史浏览、重启续接、停止、切换和关闭可用；浏览历史不启动推理；关闭某窗口不影响其他不同会话。

### P3：旧历史接入与删除过渡路径

完成只读清点、原生兼容接入、幂等恢复和第 5 节清理。原始数据保留，不能为通过验收而删除旧历史。

退出条件：旧用户会话可按原生恢复能力接入，新旧列表不双轨，不重复显示同一会话。

### P4：Windows GUI 端到端与打包

本次在 Windows 用匹配源码构建的 GUI 与 sidecar 验证 GUI 使用闭环；macOS 留待后续。确认发行包包含匹配 sidecar；旧版或不兼容原生 CLI 明确提示版本问题，不把失败伪装成空历史。

更新桌面说明、架构说明和本地验收记录。代码、本地构建、安装包及真实安装结果分别报告；不自动推送或发布。

安装与右键入口验收使用普通用户和用户级安装，验证仅为该用户注册、卸载该用户的菜单。不得为完成验收改为全机安装或提升 GUI／sidecar 权限。

## 7. Windows GUI 验收矩阵

| 场景 | 必须观察到的结果 |
| --- | --- |
| GUI 新建、保存、重启、选择历史并继续 | 同一 SavedSession ID，消息不丢失、不重复，列表不产生副本 |
| 无 API 密钥浏览历史 | 原生只读列表与详情可用，不启动模型请求 |
| 同目录两个不同 session | 可同时运行，关闭一个不影响另一个 |
| 两个 GUI 窗口接续同一 session | 原生所有权拒绝第二个写入者，允许只读；不杀进程、不抢锁 |
| 停止、切换、关闭、保存失败 | 排空执行并保存后释放；错误可见，不伪装成功 |
| Runtime 所有权或路径绑定不匹配 | 原生拒绝，原始数据不变 |
| 重启／崩溃／写入失败 | 保留原生恢复证据，不静默覆盖历史 |
| 改名／归档后自动保存 | 生命周期字段不回滚 |
| 工具、思考、附件、压缩、用量和工作状态 | 沿用原生保存／恢复语义，不从 GUI 展示文本重建；缺失材料明确报错 |
| 旧桌面历史只读清点与兼容接入 | 原始数据保留，运行中的 store 不迁移；兼容入口重试不重复导入 |
| 中文、空格、显式 home | 遵循原生路径与工作区筛选规则 |
| Windows 用户级安装与卸载 | 匹配 sidecar，当前用户右键入口正常；仅 HKCU，无提权 |

优先复用现有原生测试和本地模拟 provider，不发起付费模型请求。只补保护 GUI 使用路径中数据完整性的测试，不扩大为全面 TUI 行为验证。分别记录源码、测试、构建产物、安装包和实际普通用户验收；环境无法提供的验收必须如实列出。

## 8. 交付边界

完成后，GUI 是原生会话能力的调用者和展示层；删除的桌面专用路径要与新增的原生复用能力一起交付。不会因这项任务重写完整 TUI、实现远程同步，或承诺不同系统用户／不同 home／任意旧版本之间自动互通。

如果原生契约审计发现无法完整保存某类上下文，应先在原生保存／恢复路径修复，或明确记录阻塞与受影响范围。不能用另建 GUI session runtime 的方式绕过。
