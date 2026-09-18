#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
cargo build --release --locked -p codewhale-cli --target "$TARGET"
mkdir -p apps/desktop/src-tauri/resources
SUFFIX=""; [[ "$TARGET" == *windows* ]] && SUFFIX=".exe"
cp "target/$TARGET/release/codewhale$SUFFIX" "apps/desktop/src-tauri/resources/codewhale$SUFFIX"
python3 desktop/scripts/protocol-smoke.py "target/$TARGET/release/codewhale$SUFFIX"
npm --prefix apps/desktop ci
npm --prefix apps/desktop run build:ui
cd apps/desktop
npm exec -- tauri build --target "$TARGET"
