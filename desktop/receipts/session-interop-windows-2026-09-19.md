# Windows 原生会话互通：实施中

本次按用户要求只执行 Windows。尚未达到整个计划的完成条件；无发布、推送、安装替换或用户历史迁移。

## 当前实现及原生归属

- `SessionManager` / `SessionQuery` 提供 keyless 的 `sessions list --json`、`read`、`allocate`；用户身份为 SavedSession ID，Runtime thread ID 单独返回。
- GUI 使用原生 home 和单独的私有配置。日常路径移除了桌面 Runtime 扫描接口、`desktop-owner.lock` 和新建 `agent/tasks/desktop` 的逻辑。
- Runtime 原生 owner lock 负责执行互斥。TUI 在取得 owner 后重新读取保存会话，检查未发布 Runtime 尾部；API 校验 store/workspace，并拒绝用未关联线程覆盖已有 session ID。
- 保存使用 Engine 完整消息快照，保持原生绑定、生命周期字段及 Work 状态。复用原生 Runtime 消息恢复实现，删除另一套导入消息重建代码。
- checkpoint 仍验证摘要；原生 TUI、API 保存及导入共用先写 pending 摘要、原子写 SavedSession、再确认 checkpoint 的发布边界。兼容旧版可证明的追加前缀；压缩用精确的发布摘要验证，不跳过摘要保护。
- 旧 Runtime 导入使用稳定来源 ID 和既有 checkpoint，原记录不移动。CLI `sessions inventory --task-root` 只做显式清点。
- GUI 停止、切换、关闭执行先排空并保存；保存失败显示错误并保留恢复记录。新面板和历史浏览不启动推理。
- GUI 临时窗口信息不进入系统提示或工具目录；无新增 KV-cache 固定前缀内容。

## 已获得的本地证据

- `cargo build --offline -p codewhale-cli` 通过；当前开发 sidecar 包含 store 所有权检查、stdio session/workspace 校验。
- `cargo check --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop -p medsci-ui` 通过。
- `cargo test --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop`：`test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s`。
- `cargo test --offline -p codewhale-app-server --lib`：`test result: ok. 103 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.27s`。
- `python desktop/tests/session_interop.py target/debug/codewhale.exe --tui`：`3 scenario groups passed; 0 failed`。实际通过四轮 sidecar → 交互式 TUI → sidecar → 交互式 TUI，随后 sidecar 再读；还通过 TUI 新建后 sidecar 接续保存。使用 Windows PTY、临时 home、中文空格工作区及 localhost fixture；会话 ID 和原 Runtime ID 稳定，历史无重复。覆盖不同会话并行、同会话拒绝第二写者、无密钥只读。将 sidecar 配置改为 Full Access 后接续 TUI 新建会话，仍确认 `permission_posture != full_access`、`auto_approve == false`。
- `python desktop/tests/mock_conversation.py target/debug/codewhale.exe --native-sessions --reasoning` 通过：流式思考、工具调用及结果、重启后的模型上下文、跨工作区拒绝；仅临时 fixture 文件操作，无真实 provider 调用。
- 对当前应用旧目录的清点输出在被忽略的本地 `desktop/artifacts/session-interop-legacy-inventory.json`，发现 3 个记录，清点时均显示被占用。未导入、移动或改写它们；报告含本地会话信息，不作为公开附件。
- `wasm-pack build ui --target web --out-dir ../dist/pkg --release --mode no-install -- --offline`、`node copy-assets.mjs`、`tauri build --debug --no-bundle -- --offline` 通过。
- `desktop/tests/windows-multi-instance.ps1 -Binary apps/desktop/target/debug/medsci-desktop.exe` 通过三个真实窗口的同/不同目录启动及独立正常关闭；此检查不执行会话或模型请求。
- 2026-09-20 开发 GUI `apps/desktop/target/debug/medsci-desktop.exe` SHA-256：`3BE0FB282CDB07BF668790AEAC2185E2B248369E41A1DB6003971F3EF36FF3B1`。
- CLI、源资源和开发 GUI 的 `resources/codewhale.exe` 三处 sidecar SHA-256 一致：`495808B0EBE22BAD7CD67B79A0746D028208D7DBD99A040D699A010D08E7222C`。构建时发现开发资源仍为旧 sidecar，已显式更新并重新核对；不能仅凭 GUI 编译成功推断资源同步成功。

## 2026-09-20：压缩与发布中断验证

- 原生保存边界已由 API 保存、Runtime 导入与 TUI persistence actor 共同使用；TUI actor 保留本实例的 Runtime owner，取得保存准入后发布完成快照。队列中省略的 messages 复用原生存储投影重建；进行中的 crash checkpoint 仍保留原来语义，不冒充完成快照。
- Runtime checkpoint 新增可选 pending 摘要、消息数及覆盖 turn。在 SavedSession 原子写入之前失败时仍按原摘要恢复；写入之后确认之前失败时只接受精确匹配的 pending 摘要。未知分歧仍返回冲突，待恢复 checkpoint 的显式 fork 被拒绝直到完成保存。
- 真实 `/compact` 暴露出 `Engine::SyncSession` 的旧恢复逻辑会删除已经位于消息历史中的新摘要。修复前集成测试在恢复后消息比较处失败；修复后保留最新历史摘要原位置，仅在旧历史无摘要时迁移 legacy system-prompt carrier。摘要不进入模型固定前缀。
- `python desktop/tests/session_interop.py target/debug/codewhale.exe --tui --compact`：`4 scenario groups passed; 0 failed`。新增真实交互式 TUI `/compact`、两个原子发布中断窗口的临时存储故障注入、精确摘要恢复及错误摘要拒绝。比较会话消息/思考/摘要，同时验证原生 Agent topology 从 live 状态降为 historical checkpoint 的既定语义。
- `python desktop/tests/mock_conversation.py target/debug/codewhale.exe --native-sessions --reasoning` 在本轮二进制再次通过。
- `python desktop/tests/mock_conversation.py target/debug/codewhale.exe --interrupt` 通过：真实 bash 工具启动后中断，持久化 failed 工具结果，原进程和重启后继续均保持调用/结果配对；严格 localhost provider 会拒绝缺失结果。
- 本轮 `cargo build --offline -p codewhale-cli`、`cargo check --offline -p codewhale-tui`、GUI 开发构建及 `git diff --check` 通过；桌面测试仍为 `test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s`。
- GUI 仅补传必要 Windows `SystemDrive` 环境变量；保持 `env_clear` 白名单，不透传整个宿主环境。此修复避免受限环境里产生字面量 `%SystemDrive%` 工作目录。

## 权限与验证限制

安装配置为 `currentUser`，右键注册/清理均为 `HKCU\Software\Classes`。不申请 UAC，不使用全机右键菜单。

本任务宿主 `WindowsPrincipal.IsInRole(Administrator)` 返回 True。桌面 shell 的令牌测试也返回 True；尝试以去除管理员组及权限的 medium-integrity 受限子进程运行 Python 未能获得有效测试结果，已结束该测试子进程。因此上述运行结果仍属于管理员宿主，**不能作为普通用户或干净系统安装验收**。`session-interop-standard-user.txt` 是失败探针输出，不是通过凭据。

TUI Rust 单元测试尚未执行成功：离线缓存缺少 `corcovado v0.5.26`，Cargo 在执行任何测试前退出；无 TUI 单测通过声明。GUI 的 18 个测试、app-server 的 103 个测试与真实二进制集成测试是不同证据。

## 2026-09-20：用户收敛为 Windows GUI 原生能力复用

用户明确实际只使用 GUI，不使用 TUI。本次完成门槛改为 GUI 新建、历史读取、重启续接、停止／切换／关闭及用户级打包。不继续扩展 TUI 会话切换、显式 fork、独立 Terminal CLI 或跨宿主费用账本；前述 TUI 测试仅作为已经获得的补充证据。

- 删除尚在开发中的跨宿主用量来源去重逻辑，复用 Runtime 既有累计费用。GUI 保存 token 改为原生累计用量与已保存值的最大值，避免进程重启后由 Engine 局部 token 数覆盖历史累计值；保留旧记录的未知计费标记。
- `cargo build --offline -p codewhale-cli` 通过。
- `python desktop/tests/session_interop.py target/debug/codewhale.exe`：`2 scenario groups passed; 0 failed`。四次独立 sidecar 进程续接，fixture 每轮返回 100 input / 10 output tokens；依次断言 110、220、330、440，逐次重复保存费用/token 均不变。另覆盖无密钥读取、不同会话并行和同会话占用拒绝。这里验证 GUI 使用的真实 stdio 原生入口，不声称点击过 GUI 控件。
- `python desktop/tests/mock_conversation.py target/debug/codewhale.exe --native-sessions --reasoning` 再次通过。
- `python desktop/tests/mock_conversation.py target/debug/codewhale.exe --interrupt` 再次通过。
- 打包脚本支持显式选择本机构建的 GUI、sidecar 与 NSIS；保留既有构建默认路径，不再删除同版本的旧 staging 目录。

## 当前范围内尚需核实

1. GUI 原生恢复中路由/reasoning、工作状态及附件的必要字段；复用已有原生语义，不继续扩大为 TUI 全功能保真审计。
2. 旧桌面存储兼容接入与重试的本地证据；真实旧记录仅做清点，未改写。
3. GUI 实际会话控件交互、安装包核验及普通用户安装/卸载与右键入口。普通用户环境限制见上文；不能用当前管理员宿主结果冒充。

macOS、TUI 操作与跨宿主往返均不属于本次完成门槛。
