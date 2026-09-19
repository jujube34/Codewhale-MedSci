# 多实例启动：本地实现与验证

范围：Windows 本地源码、实际 GUI 开发构建及配套 sidecar；没有替换已安装程序、修改右键注册表、发布或调用模型服务。已有未提交改动保留。

## 实现

- 移除 Tauri 单实例插件及转发回调。每次启动独立窗口，同一目录也不去重。无目录参数时使用启动工作目录；窗口内主动切换忙碌工作区的确认流程保留。
- 每实例独立临时配置、WebView 数据目录及私密浏览上下文。空窗口不启动 Agent；原生标题含完整目录、会话和 PID。
- 沿用原生 `CODEWHALE_RUNTIME_DIR` 隔离执行存储，保存于 `agent/tasks/desktop/session-*`。没有删除 Runtime 进程所有权锁，没有新增执行循环或 GUI 会话数据库。
- 原生 Runtime 只读历史接口汇总新存储和旧 `agent/tasks/runtime`。同一原生存储只能被一个 GUI 续接；切换失败恢复原窗口状态。旧存储作为整体占用，不搬移旧历史。
- 全局设置通过唯一临时文件和跨进程锁原子替换。最后成功保存的值作为新窗口默认值；既有窗口保留配置。共享凭据在其他窗口重启 Agent 后读取。
- Python 更新锁与 Rust 共享使用锁互通，更新不能进入其他实例正在使用的环境。Windows 每个 Agent 树拥有独立、关闭即回收的 Job Object。
- 窗口内会话切换、重启、初始化、设置保存和发送的准入串行化。实例信息只用于宿主状态与环境变量，不插入模型固定前缀，KV-cache 前缀不变。

## 已运行的证据

工具使用本机现有 `C:/Users/YFZ/anaconda3/envs/docapp/Library/bin` Rust 工具链及本地 Python；构建均带 `--offline`。

- `cargo check --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop -p medsci-ui`：通过。
- `cargo check --offline -p codewhale-app-server -p codewhale-tui`：通过。
- `cargo test --offline --manifest-path apps/desktop/Cargo.toml -p medsci-desktop`：`test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`。
- 其中跨进程存储/设置、Windows 子进程归属、Rust/Python 锁互通定向测试：`test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out`（包含由父测试再次执行的子进程探针）。
- 反向验证：临时仅移除 `manager.py` 的 `usage.lock` 后，更新保护测试为 `test result: FAILED. 0 passed; 1 failed`，证明更新会越过保护进入恢复流程；随后恢复实现，完整桌面测试通过。
- `cargo build --offline -p codewhale-cli`：通过，配套 sidecar 已同步到开发资源。
- `python desktop/tests/multi_instance.py target/debug/codewhale.exe`：3 组真实进程场景通过。覆盖同目录/不同目录独立会话、跨存储只读历史、旧历史、重复获取 Runtime 所有权被拒绝、单独退出、持久保存和续接；未发起推理请求。
- `python desktop/tests/runtime_encoding.py`：`5 checks passed; 0 failed`。
- `wasm-pack build ui --target web --out-dir ../dist/pkg --release --mode no-install -- --offline`，随后 `node copy-assets.mjs`：通过。
- `tauri build --debug --no-bundle -- --offline`：通过。
- `desktop/tests/windows-multi-instance.ps1 -Binary apps/desktop/target/debug/medsci-desktop.exe`：三个真实 GUI 同时打开，两个显式指定同一个中文含空格目录，第三个无参数从另一个目录启动；标题各自正确，关闭第一个后其余两个继续运行。未启动 Agent 或发起模型请求。
- 桌面 workspace 格式检查、三个修改的原生 Rust 文件格式检查及 `git diff --check`：通过。

## 开发产物

`apps/desktop/target/debug/medsci-desktop.exe`，必须保留相邻 `resources` 目录。

- GUI SHA-256：`18ECFF364BAA4F6ED0A50E5F1211A44E6ACEE9CD3D3EB2258ECBB528B22EE942`
- 配套 `resources/codewhale.exe` SHA-256：`AFE4FD677DBFD31328E60548956567C56A6BB8FAE97DE86B84BD9B4421A98F2F`
- `manager.py` 源码与产物 SHA-256 一致：`AF4B27D6A7ADE1F916512084E2CBC0FF9E5B50AE618A73D123D44C1885B4E433`

未验收：macOS 实机、多实例真实模型并发任务、重新打包安装后的 Explorer 右键交互。当前系统右键入口仍指向已安装旧版；开发 EXE 与新 sidecar 已配套，本次不修改系统安装。
