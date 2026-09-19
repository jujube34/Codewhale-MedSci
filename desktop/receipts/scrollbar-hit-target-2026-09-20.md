# Windows scrollbar hit target — local verification

The inward 12px custom window resize grips covered the entire 8px main
scrollbar. Moved all eight grips into the existing outer shadow gutter.
Scrollbar thumb hover/active styles preserve visibility during interaction;
the existing 700ms idle hiding behavior remains.

Verified in the local browser with the actual frontend assets and bundled
WASM UI, using a temporary offline IPC fixture (12 user turns):

- Before: the scrollbar center hit `data-resize-direction="East"`;
  dragging from (1252, 573) to (1252, 270) left scrollTop at 7490.
- After: the same point hit `MAIN`; the same drag moved scrollTop from
  7490 to 2868.
- Clicking the idle/hidden track moved scrollTop from 2868 to 3327.
- Dark-theme dragging moved scrollTop from 3327 to 6071; dragging again
  after idle moved it to 1434. A screenshot confirmed the scrollbar was
  hidden again after moving the pointer away.
- Hit checks in the outer gutter resolved all eight expected resize
  directions. The scrollbar no longer overlaps their hit regions.

This verifies browser mouse hit testing, track clicks, thumb dragging, and
idle hiding. It does not verify native OS window resizing or an installed
WebView2 build. No installer was rebuilt.

Checks run: `node apps/desktop/copy-assets.mjs`, JavaScript module syntax
check via `Get-Content -Raw apps/desktop/ui/bridge.js | node --input-type=module --check`,
and `git diff --check -- apps/desktop/ui/bridge.js apps/desktop/ui/style.css`.
