# GUI 文件引用：本地源码验证

本记录仅覆盖本地源码与本地构建，不代表安装包、CI 或发布验收。

- `cargo check --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop -p medsci-ui`：通过，Windows。
- `cargo test --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop file_references::tests`：`test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out`。
- 反向验证：临时去掉 `prepend` 的路径前缀输出，前缀测试得到 `test result: FAILED. 0 passed; 1 failed`；恢复实现后上述两项通过。
- `cargo fmt --manifest-path apps/desktop/Cargo.toml --all -- --check`：通过。
- `wasm-pack build ui --target web --out-dir ../dist/pkg --release --mode no-install -- --offline`（工作目录 `apps/desktop`）：通过；执行 `node copy-assets.mjs` 同步资源。
- JavaScript 模块语法检查与 `git diff --check`：通过。

在本地浏览器中加载实际 WASM UI、bridge.js 和样式，使用模拟的 Tauri 剪贴板与发送接口，12 项交互断言通过：五种文件/文件夹显示、仅有引用可发送、类型图标、重复路径去重、删除后编号不变、10 项上限、超限提示且不丢引用、发送失败保留内容、发送参数包含有序路径、发送成功清理、普通文本粘贴、发送等待期间新草稿和新引用不被清除。检查了默认窗口和 640×480 窗口的布局。临时测试页面已移除；未调用模型服务。

尚未完成：Windows Explorer 真实剪贴板到原生 WebView 的端到端实测；macOS 编译与 Finder 实机粘贴；重新打包安装。Windows 使用 CF_HDROP，macOS 使用 NSPasteboard 的 public.file-url。路径前缀只进入用户消息历史，不改变会话固定的系统提示词。

## 同日追加：原生文件拖拽

通过 Tauri `getCurrentWebview().onDragDropEvent` 接收 GUI 内的文件/文件夹拖拽。拖入时高亮输入框，离开时取消高亮，松开后将原生绝对路径交给与剪贴板共用的校验函数；前端复用引用列表、处理队列和发送入口。

本地 `cargo check --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop` 通过；上述两项 Rust 测试补充了拖入路径的有序分类、批量上限和无效路径校验，结果仍为 `test result: ok. 2 passed; 0 failed`。JavaScript 语法检查通过。

实际 WASM UI 配合模拟 Tauri 拖拽事件与路径解析接口，通过 11 项界面断言：拖入高亮、离开取消、解析期间禁止发送、文件与目录混合拖入、共用编号、重复路径去重、粘贴与拖拽顺序、无效路径保留现有引用、累计超限原子拒绝、删除不改变其他编号、发送携带引用并清空列表。临时验证页面已移除。

Explorer/Finder 到原生 WebView 的真实拖拽端到端测试仍未执行；本次未重新打包或安装。

## 同日追加：供用户测试的 Windows 开发产物

已通过 `tauri build --no-bundle -- --offline` 生成最新 Windows x64 release GUI，并组装到 `desktop/artifacts/Codewhale-MedSci-dev-20260919-file-references-windows-x64/`。前端先执行前述离线 WASM 构建并同步资源。该目录为免安装开发版，保留整个目录后直接运行 `medsci-desktop.exe`；不修改现有安装，沿用本机配置与会话。启动前应退出旧版，以免被单实例机制切回旧窗口。

Agent、Python、离线依赖与 Git Bash 复用已有本地载荷；附带 VC runtime DLL。已完成 GUI 源/目标文件 SHA-256 一致性及 x64 PE 检查、43 个离线 wheel 哈希校验、Python 3.12.13 启动、Git Bash 初始化与自检、`protocol-smoke.py` 的 Agent 握手和会话测试。无模型请求。目录中的 `build-provenance.json` 记录源码输入及二进制哈希，`medsci-desktop.exe.sha256` 提供校验值。

此产物未签名，未执行安装，也未宣称 Explorer/Finder 原生交互端到端测试通过。

## 同日追加：标题栏工作目录按钮

标题栏目录名改为原生 button，增加悬停/按下样式、完整路径提示和键盘焦点；点击调用现有 `open_folder`。对话框从当前工作目录打开，取消不改变目录，运行中切换继续使用原有确认流程。按钮在选择期间禁用，防止重复打开。

离线 WASM 构建、Windows release 构建、格式检查及 diff 检查通过；实际 WASM 配合模拟目录选择接口的 4 项交互检查通过：按钮脱离窗口拖动区、连续点击只请求一次、选择后目录名称与完整路径更新、取消后保留目录。原生系统选择对话框未自动化实测。开发版目录的 EXE 已更新，SHA-256 和构建记录同步更新。
