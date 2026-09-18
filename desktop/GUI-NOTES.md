# GUI revision — 2026-09-18

Implemented the annotated bright/minimal design in apps/desktop/ui. The repository-root background1.png is copied into the frontend at build time, covers the entire viewport, and is composited at 94% opacity. HTML/body and the native window are transparent; text is not faded. Composer uses a rounded translucent surface with CSS backdrop blur.

Removed branding, welcome/promotional text, example tasks, attachment buttons, provider/approval/network/agent footer labels. Actual actionable errors remain visible; browser-only previews no longer poll unavailable native IPC and therefore do not generate a spurious startup error. Paste/drop image support remains. Cmd/Ctrl+O opens a directory; Cmd/Ctrl+N starts a new session.

The workspace basename is centered in a 44px draggable title area. macOS uses an overlay titlebar and native traffic lights. Windows removes the standard frame and provides minimize/maximize/close controls on the same title row through Tauri window commands. Native OS title follows the basename. When launched without a directory argument, the OS user home becomes the workspace and its basename is shown. Explicit directory arguments take precedence. Reopening an existing instance without arguments preserves the active workspace.

The textarea starts at 28px (22px text line plus padding), measures wrapped content, expands to 180px, then scrolls. Resizing and programmatic draft updates trigger recalculation; IME Enter handling remains intact.

Verification: frontend release build passed; native cargo check passed. Browser preview confirms cover background, removed UI, 28px single-line input and 138px wrapped input. Native inspection service was unavailable: desktop transparency, macOS traffic lights, Windows controls and dragging still require native OS verification. A browser preview cannot show other desktop applications through its host window.

Configuration reference: https://v2.tauri.app/reference/config/

Preview.3 readability adjustment: background opacity 94%; user/assistant/tool surfaces 94%; composer 90%. Text remains fully opaque. Desktop detail contributes less than 1% under reading surfaces, while empty areas preserve a slight transparent effect. Background remains anchored to the right.

Latest preview.4 revision: removed the continuous white gradient entirely. Assistant and tool text is drawn directly over the image with dark ink and a light text shadow. Added workspace-scoped session picker, native Runtime history restoration, recorded-cost totals, and estimated retained-context meter.

Readability revision: shared macOS/Windows text is black, and all 17 explicit font sizes are increased by 2px (base 14px → 16px). Primary buttons use a light fill to retain contrast with black labels. Windows uses SimSun (宋体), including form controls and code. Scrollbar tracks/thumbs are transparent at rest; a scroll event reveals the affected container's thumb until 700ms after the last event, without changing its width. This also covers dialogs, session lists, code blocks, and expanded inputs.

Local verification: `node --check apps/desktop/ui/bridge.js`, a source-size comparison, and frontend asset equality checks passed. Browser preview confirmed black text, enlarged controls, and the settings layout. A temporary Windows-platform browser fixture confirmed the SimSun font stack and visible/hidden scrollbar thumb states with unchanged content width. Native Windows WebView2/font rendering and rebuilt installers have not been verified for this revision.
