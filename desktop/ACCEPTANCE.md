# Apple Silicon 内部测试构建验收记录

版本 0.1.0-preview.7；2026-09-18。此交付用于个人本机测试，不满足 spec.md 第16节完整完成定义。没有 Developer ID 签名或 Apple 公证；签名为 ad-hoc。

## 本次交付范围

- macOS 13+，Apple Silicon arm64。明亮不透明 GUI，右侧固定背景，动态高度输入框，居中工作目录名。
- 原生文件/编辑/工具菜单、单实例、真实 stdio Agent、API 系统凭据库、审批、Runtime 原生 session 历史。
- 独立 CPython 3.12.13、锁定离线 wheel、CPU RapidOCR 及内置模型。

## 规格第14节场景对应

| 场景 | 状态与已有证据 |
|---|---|
| 1–2 工作区与单实例 | 已实现；原生 Finder 启用、中文路径端到端操作待人工验收 |
| 3 未配置 API | 已实现；需原生点击验收 |
| 4 真实调用和密钥 | 本地模拟服务流式调用通过；真实用户密钥/服务未验收 |
| 5 停止与恢复 | 已实现；完整崩溃恢复测试待验收 |
| 6–7 办公文件 | Python 直接创建/读取/修改四类文件通过；模型自主调用待验收 |
| 8 拒绝危险写入 | 真实 sidecar + 模拟模型，运行中拒绝文件写入通过 |
| 9 公司账号 | 明确未启用，不伪造登录；界面已实现 |
| 10 恢复 | 会话列表、ID、历史和续接统一来自 Runtime；旧 GUI 数据库保留但不再参与交互 |
| 11 卸载入口 | Finder 扩展移除/系统登记待验收 |
| 12 异常处理 | 部分实现；完整异常矩阵待验收 |
| 13 离线初始化 | 已实现；包内全新环境验证结果见 receipts |
| 14–17 全局依赖事务 | 底层事务/锁/重建存在；Agent 安装审批与全平台保护未完成 |
| 18–19 OCR | 本地图片识别通过；包内图片及多页PDF测试结果见 receipts |
| 20 三平台 OCR | 此交付仅验证 Mac arm64；其他平台未验收 |
| 21–26 网络 | 仅直连；代理/PAC/统一回退未完成 |
| 27–31 Local-first | 桌面默认禁用 MCP Registry；完整能力索引、收据、审批回退未完成 |
| 32–36 视觉 | 路由迁移与禁止剥图重试已实现；真实服务及完整请求矩阵未验收 |

`receipts` 内记录实际执行结果。GUI 编译成功不等于原生窗口效果、Finder 授权或模型任务端到端验收成功。原生 UI 自动检查服务当前不可用。

## preview.4 用户指令覆盖

默认 Full Access，关闭操作审批并启用网络。原场景8的拒绝测试仍覆盖底层兼容接口，但不代表当前产品默认策略；当前默认策略以 `receipts/full-access-macos-arm64.txt` 的无审批真实写入回归为准。用户消息右对齐气泡；Agent 和工具消息无气泡，直接绘制在背景图上；新消息和流式更新自动滚动到底部。

## preview.5 流式思考

消费 Runtime 的 agent_reasoning 起始、增量、完成事件；实时展示且不混入最终回复，思考不占工具事件配额。按时间线保留工具前后的独立回复段。桌面测试 7 passed; 0 failed。离线模拟服务通过真实 sidecar 的 reasoning_content 流式链路，思考先于最终回复到达，无真实供应商调用。

## preview.6 原生 session 与不透明背景

移除 GUI 自建 ID、Conversation 数据类型及 SQLite 读写，显示状态仅是 Runtime ThreadDetail 的临时投影。每轮结束重新读取原生 items；切换、重启及续接使用同一个 Runtime thread ID。背景 alpha=1，Tauri transparent=false。旧 GUI 数据库不删除，不以其中孤立文本伪造原生会话。

运行中补充指令由 stdio thread/steer 转发至 Runtime 现有 /v1/threads/{id}/turns/{turn_id}/steer，复用 reserve_steer 与原生 turn/item 持久化。GUI 以 turn.steered 回执展示补充文字；无活跃回合时报告拒绝而不偷偷新建回合。

## Windows x64 preview.7

本地交叉编译 Windows x64 主程序与同版本 Agent。NSIS 按当前用户安装，自动注册文件夹及文件夹空白处右键菜单，并在卸载时移除。共享钩子已在隔离 Wine 前缀验证注册、移除以及中文、空格和磁盘根目录参数；Windows 11 对应“显示更多选项”。原生 Windows 安装/WebView2/GUI 仍需实机验证，不能以 Wine 检查替代。包内附带 app-local VC runtime DLL，保留完整离线 CPython 和 wheelhouse。

## preview.7 停止后续接

GUI 停止按钮不再关闭 sidecar；切换目录/重启所需的进程关闭等待原生中断回执后进行。Runtime 异常工具记录保留 tool_result_for/is_error，恢复历史时直接调用 TUI 的 tool_history_repair。CommandExecution/FileChange 与 ToolCall 一并恢复工具配对。只读检查发现用户报错对应的失败 bash 记录缺少 tool_result_for；未直接修改用户数据。

验证：桌面单元测试 `7 passed; 0 failed`；原生历史恢复测试 `3 passed; 0 failed`（其中恢复用例覆盖三类工具记录 × 三种终止状态）。真实 Mac sidecar + 本地模拟模型通过运行中 bash 中断、同进程续接、重启续接、原生会话切换、流式思考及 steer。中断集成用例在旧 sidecar 的正常中断路径也通过，不能据此声称复现了旧版异常退出；旧数据缺失关联由只读诊断与原生恢复测试覆盖。未调用真实供应商，未修改用户历史文件。

Windows NSIS 完整性、SHA-256、3388 个提取文件与打包目录逐项一致、x64 PE、VC runtime 和离线 wheel 校验通过；包内 Windows sidecar 在 Wine 下协议启动通过。原生 Windows 安装、WebView2 与 GUI 仍待实机验收。

## Windows installer preview.7.1

修复 WebView2 bootstrapper 返回 -2147219416（0x80040828）时被旧脚本当作硬失败的问题。应用二进制沿用 preview.7，仅修订 NSIS 安装逻辑。遵循微软 distribution 文档检测 32 位 HKLM 与 HKCU 的 pv；有有效版本则不运行 bootstrapper，缺失时安装并复查，不盲目放过真实依赖失败。

`python3 desktop/tests/windows-webview2.py` 在专用 Wine prefix 中执行实际共享 NSIS 宏，并重放旧生产条件和截图中的精确 HRESULT；测试不安装真正的 WebView2，不代表原生 Windows GUI 实机验证。

来源：
- https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- https://raw.githubusercontent.com/microsoft/winget-pkgs/master/manifests/m/Microsoft/EdgeWebView2Runtime/146.0.3856.72/Microsoft.EdgeWebView2Runtime.installer.yaml

## Windows installer preview.7.2

使用包内 Windows Python 和 manager.py 在隔离 Wine 中复现初始化失败：colorlog 所需的 Windows 条件依赖 colorama 缺失；核查同时发现 pandas 的 Windows 条件依赖 tzdata 缺失。补齐两个 wheel，重新生成 43 个 wheel 的哈希清单，并将平台条件依赖加入 requirements.pinned。宿主错误提示保留脱敏后的实际原因；manager 在子进程没有输出时保留退出码。

初次补齐后的完整导入检查遇到 Wine 自带 ucrtbase.dll 缺少 crealf 的环境限制；测试前缀改用微软原生 UCRT，不将此 DLL 或 Wine 专用设置加入产品。桌面测试 7 passed; 0 failed。最终运行时测试和包校验见 receipts。

核心 Windows 初始化链路已通过首次初始化、check、再次初始化。额外运行 document_smoke.py 在 Wine 中出现退出码 3221226505，未定位且不能据此宣称 Windows 办公文件功能通过；保留失败收据 windows-document-wine-preview7.2.txt。默认 windows-runtime.py 专门验证初始化；加 --documents 可重跑额外文档检查，该失败没有被忽略为成功。

## Windows preview.7.3: native DLLs and output encoding

用户提供 `_extra` DLL load failure 后，检查 PyMuPDF PE imports 确认 MSVCP140 依赖；原包只部署 VCRUNTIME。禁用 Wine 内建 MSVCP，原始 Python 导入 `_extra` 复现相同 ImportError；包内 app-local 微软 CRT 使相同导入通过（2 checks passed; 0 failed）。共享 venv 的 Scripts 也部署相同 CRT，manifest 哈希变化触发旧环境事务重建。

Python 主进程与所有 Python 子进程显式 -X utf8，兼容 -I 忽略环境变量的情况。二进制输出先严格 UTF-8 解码，再用系统 ANSI 代码页及 GB18030；无法解码的字节以反斜杠形式保留，失败日志另存原始字节。UTF-8、GBK、CP1252、失败原文保留、隔离子进程编码测试 5 checks passed; 0 failed。不修改用户系统 locale。

Windows Python 下重复执行编码检查：5 checks passed; 0 failed。禁用 Wine 内建 MSVCP 的初始化、check、再次初始化均通过，原生 `_extra` 对照导入通过。额外 document_smoke.py 仍在 Wine 下以 3221226505 退出；保留完整失败日志，未声称文档读写或原生 Windows 实机全部通过。

## Windows preview.7.4: bundled Git Bash

使用官方 PortableGit 2.55.0.windows.5，验证 GitHub release asset SHA-256，保留全部 9584 个原始文件与许可证。复用 ShellDispatcher 的 SHELL=Bash 检测及原生 bash 工具；usr/bin 在应用 PATH 首位，直接执行 usr/bin/bash.exe 避免 bin/bash.exe 包装进程；原生环境提示会显示 bash，无新增执行循环。启动探测限时 30 秒，kill_on_drop 清理直接启动的 Bash。安装时按 PortableGit README 运行官方 post-install 脚本。

当前 macOS Wine 上 Git Bash 在 FAST_CWD 警告后挂起：bin 包装器和直接 usr/bin/bash.exe --version 两条路径均未完成，测试进程已清理。不能以编译或包校验代替 Windows 命令执行、中文目录、停止续接和官方 post-install 的实机验证；以上功能需原生 Windows 验收。

## Windows installer preview.7.5: Git initialization

依据 2026-09-18 公司 Windows 10 诊断报告修复初始化成功后返回 1 被 NSIS 当作失败的问题。当前源码已有 SetOutPath；尚不能将报告中的错误 cwd 链路归因于缺失该行。新 hook 通过系统 cmd 调用绝对路径 helper，在 helper 内检查切换目录，优先使用包内 Git，仅改变子进程环境。官方批处理无论成功与否都可能自删，因此还检查 post-install 目录已清理，并执行包内 Bash、mtab 和 Git 自检；不直接放行所有返回码 1。

生产 NSIS 宏 + 真实 Wine cmd + 可控 Git/MSYS 可执行替身覆盖 8 个场景。相同回归测试对 HEAD 中旧 hook 的首个场景失败：self-delete-exit-1，实际安装退出码 2，期望 0；修复版结果见 receipts/windows-git-init-preview7.5.txt。替身测试不证明真实 MSYS 初始化；此前本机 Wine 的 FAST_CWD 挂起限制未解决。中文路径下原生 Windows 10 安装、Bash/Git 启动与卸载仍须实机复验。
