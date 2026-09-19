# Codewhale-MedSci Desktop 内部开发预览

版本：0.1.0-preview.7。依据仓库根目录 spec.md 0.4 开始实现。
这是可运行的内部开发构建，**尚未满足 spec.md 第 16 节的完整预览版完成定义**。
不应将未签名、未公证的安装包作为正式企业分发包。

## 使用

- 按用户最新要求，默认 Full Access：`approval_policy=auto`、`sandbox_mode=danger-full-access`、网络访问启用。Agent 的命令执行和文件操作不再等待确认，仍受当前用户的操作系统权限约束。此设定覆盖原 spec 的工作区审批默认策略。

- 每次启动独立进程和窗口，允许相同或不同目录重复打开，不限制窗口数量。明确传入目录时优先使用该目录，否则使用启动进程的工作目录（无法取得时回退用户主目录）。macOS Finder 右键扩展或 `open -n -a Codewhale-MedSci --args /绝对目录` 可请求新实例；Dock/启动台自身可能复用已有应用。

- macOS：将应用拖到 Applications。当前包仅为 ad-hoc 签名，没有 Developer ID 或公证；请通过组织批准的内部测试流程运行。
- Windows 10/11 x64：使用当前用户安装器安装，无需管理员权限。安装时自动注册文件夹空白处和文件夹图标的“在 Codewhale-MedSci 中打开”，传入该目录作为工作目录；卸载自动清除。Windows 11 位于“显示更多选项”，一级 IExplorerCommand 尚未实现。首次安装 WebView2 可能需要联网。
- 点击标题栏工作目录按钮、从菜单“文件 → 打开工作文件夹…”或按 Cmd/Ctrl+O 选择工作文件夹；从底栏“配置 API”保存自己的 DeepSeek 密钥，保存后测试连接。
- 第一次发送前离线建立共享 Python 环境，也可从“工具 → 初始化离线办公环境”主动初始化。
- 模型名右侧“会话”可查看当前目录的记录、新建或切换会话。列表和浏览历史直接来自原生 SavedSession，不要求配置 API 密钥；开始执行时取得原生 Runtime 所有权，切换前停止并保存任务。
- 底栏计费来自 Runtime 累计用量与价格收据；未知价格显示暂无数据，部分计费以 ≥ 标记。上下文是当前保留历史的估算，按 1024 tokens = 1K，每轮结束更新，鼠标悬停可查看口径。
- Enter 发送，Shift+Enter 换行；Cmd/Ctrl+O 打开文件夹，Cmd/Ctrl+N 新建会话。
- 公司登录明确返回“企业登录暂未启用”，不会保存公司密码或伪装登录。
- 从 Windows 文件管理器或 macOS Finder 复制文件/文件夹，在任务输入框粘贴，或将选中的文件/文件夹直接拖入 GUI（拖入时输入框高亮，松开添加引用）。上方逐行显示删除按钮、类型图标、文件/文件夹 A–J 和绝对路径，总计最多 10 项；重复路径去重，删除不会改变其余引用的编号。发送时按列表顺序将“文件A：绝对路径”等附加在用户指令前，也支持运行中的补充指令。发送失败保留草稿与引用。此功能替代原图片上传设计；直接粘贴截图不再作为附件。
- macOS Finder 扩展随包提供源码和编译载荷。需要在系统设置的扩展项中启用；未签名内部包的系统加载行为尚未验收。

## 已接入

Tauri 2 + Leptos 每进程独立窗口、明亮不透明主题、文件夹选择和多实例启动；系统凭据库；
真实 app-server stdio 握手与流式回复；工具及审批事件；允许一次/拒绝；
结构化用户输入回复；停止和重启；Codewhale Runtime 原生 session 列表、历史与续接；安全 Markdown；
文件引用数量、绝对路径、类型和存在性校验；独立 CPython、离线锁定 wheel 和共享环境事务管理；
CPU RapidOCR 与本地模型；两种旧 Flash ID 在官方路由迁移。

## 尚未完成的规格项

- 网络目前仅直连，系统代理/PAC/手动代理及三类统一出口尚未实现。
- Registry 在桌面配置中禁用。本地能力索引、缺失能力门禁和选择收据尚未实现。
- 共享依赖首次初始化可用；完整的 Agent 安装申请、变更计划审批、pip 拦截、全平台只读保护尚未完成。不要把底层 manager CLI 当成对 Agent 开放的无审批安装接口。
- preview.6 移除 GUI 自建会话 ID 和 SQLite 存储读写。本次源码改为以原生 SavedSession ID 标识用户会话；Runtime thread ID 仅作为执行身份。旧 GUI 数据库留在原处，不删除、不再读取；只有旧 GUI 文本而没有 Codewhale 持久历史的记录不冒充可续接 session。
- GUI 新建任务仍采用 Full Access；接续已有原生会话不再无条件覆盖其权限。应用和安装器本身不要求系统管理员权限。
- 未做真实 DeepSeek 付费调用、完整多模态矩阵、50 轮稳定性及三个干净系统安装/卸载验收。
- Finder 扩展授权、Windows 11 一级菜单、Developer ID、公证、Windows 代码签名、许可证审查待完成。
- 日志滚动、脱敏诊断导出、全量本地化资源、可访问性焦点管理和菜单细节仍需补齐。

## 源码与构建

桌面工程位于 `apps/desktop`，是独立 Cargo workspace，避免改动 Agent 依赖图。
基线 Agent 提交为 `e9b761c08accec7f2ca2a7202a5d710b72faf325`。
兼容补丁集中在 `crates/config/src/route`、模型目录、turn_loop 和 app-server。

原生构建示例（Rust、Node 22、wasm-pack、uv 0.11.8）：

```sh
rustup target add wasm32-unknown-unknown
python3 desktop/scripts/prepare-runtime.py
desktop/scripts/build-local.sh
```

`.github/workflows/desktop-preview.yml` 提供三平台原生构建矩阵。
工作流仅手动触发并上传内部构建产物，不发布 Release，不宣称已签名。
本次工作没有自动推送仓库或触发云端 CI。

`desktop/receipts` 记录实际测试证据；`desktop/ACCEPTANCE.md` 区分通过、未验证与未实现。
安装包位于 `desktop/artifacts`，附有 SHA-256。运行时清单保留在各包 resources/python 中。

- preview.5 起展示模型实际返回的流式思考，标注“思考”，与最终回复分开保存并随会话恢复；未返回思考内容的模型只显示回复。

- 背景图片完全不透明；原生窗口关闭透明，图片仍靠右对齐，左右调整窗口时从左侧裁切。

- 运行中可继续键入文字并按 Enter 或发送按钮 steer 当前任务；输入框仅一个按钮：空白时停止（未运行时置灰）、运行中有文字时 Steer、未运行有文字时发送。直接使用 Codewhale 原生 active turn steer，不开启新回合、不拼接自定义系统提示；当前原生 steer 接口仅支持文字。

- preview.7：停止仅中断当前回合并保留 Agent/session；继续输入使用同一原生会话。Runtime 复用 TUI 的工具历史修复，兼容旧版本停止后缺少结果的记录；不将中断冒充工具成功。

- Windows 安装程序修订版 0.1.0-preview.7.1（应用仍为 preview.7）：按微软文档检测 HKLM/HKCU 的 WebView2 Runtime 版本，已存在则跳过 bootstrapper；运行安装器后再次检测，避免把已安装状态的非零返回码误判为失败。右键菜单和 Agent 功能不变。

- Windows 安装程序修订版 0.1.0-preview.7.2：补齐 Windows 特有的 colorama/tzdata 离线依赖与哈希，修复首次初始化失败；失败时显示实际原因及子进程退出码。保留 preview.7.1 的 WebView2 安装检测。

- Windows preview.7.3：随包提供完整 x64 Visual C++ CRT（包括 MSVCP140），首次创建或升级共享环境时复制并验证 DLL。内部 Python 子进程统一 UTF-8；旧编码输出按 UTF-8、系统 ANSI 代码页、GB18030 依次解码，失败的原始字节保留在 `%LOCALAPPDATA%/Codewhale-MedSci/python/shared/logs`。不修改系统代码页。

- Windows preview.7.4：内置官方 PortableGit 2.55.0.windows.5（含 Git Bash）。Agent 默认使用包内 usr/bin/bash.exe；仅为应用进程设置 SHELL/PATH/MSYSTEM 与 UTF-8 locale，不修改系统 PATH，也不依赖用户安装 Git、PowerShell 7 或 WSL。启动时自检 Bash、当前目录和 Git；失败或超时会明确提示。安装时执行官方 post-install.bat，卸载移除私有 Git 运行时。

- Windows 安装程序修订版 preview.7.5：修复 Git 初始化成功自删后返回 1 被误判为安装失败的问题。使用系统 cmd 的绝对路径入口、明确工作目录和进程内包优先 PATH；检查批处理与 post-install 目录清理后再运行包内 Bash/Git 自检。初始化未完成、自检失败和超时仍中止安装，详情写入安装日志。应用与 Agent 二进制沿用 preview.7.4；原生 Windows 安装复验仍待完成。

## 多实例与共享数据

- 新启动默认新会话，Agent 按需启动。标题含完整目录、会话 ID 和进程号；手动切换忙碌工作区仍需确认。
- 运行配置和 WebView 缓存位于每实例临时目录；GUI 复用 Codewhale 的原生 home（包括显式 `CODEWHALE_HOME`）、SavedSession 和执行能力。用户无需使用 TUI。GUI 的私有配置和凭据不覆盖原生配置；旧 `agent/tasks/runtime`、`agent/tasks/desktop/` 保留原处，通过显式原生清点及导入入口处理，不混入日常列表。
- 每个会话存储只有一个窗口可续接；被占用时返回提示，原窗口不受影响。旧版共享存储中的会话作为一个整体占用，保留原始数据及 Runtime 独占保护。
- 全局设置采用跨进程锁及唯一临时文件原子替换；最后成功保存的配置成为新窗口默认值，其他窗口保留运行配置。系统密钥共享，其他窗口重启 Agent 后读取新凭据；删除凭据不会撤销已交给运行中进程的凭据。
- 共享 Python 使用期间持有读锁，初始化/更新获取独占锁。其他窗口仍有 Agent 使用该环境时，更新会失败并保留现有环境；可关闭那些窗口后重试。
- Windows 每个 Agent 树归属独立 Job Object，退出或崩溃仅回收本实例的子进程。同目录多个任务仍须避免同时改写同一文件或 Git 索引。
- 本改动要求 GUI 与支持 `native_session_interop` 的 sidecar 配套更新；旧 sidecar 会明确拒绝启动。右键菜单仅注册到当前用户 HKCU。原生能力复用的实现与验收见 `receipts/session-interop-windows-2026-09-19.md`，旧安装包不代表这些源码改动。2026-09-20 起按 Windows GUI 使用路径验收，GUI／TUI 往返不属于交付要求。
