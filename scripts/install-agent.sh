#!/bin/sh
# Installs tiling_window as a per-user macOS LaunchAgent so it runs in the
# background at login. Re-run it after rebuilding to refresh the installed copy.
set -eu

LABEL="com.rodrigopoblete.tiling-window"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$REPO_ROOT/target/release/tiling_window"

BIN_DIR="$HOME/Library/Application Support/tiling-window"
BIN="$BIN_DIR/tiling_window"
LOG="$HOME/Library/Logs/tiling-window.log"
AGENTS_DIR="$HOME/Library/LaunchAgents"
PLIST="$AGENTS_DIR/$LABEL.plist"
DOMAIN="gui/$(id -u)"

if [ ! -x "$SRC" ]; then
    echo "Release binary not found; building..."
    (cd "$REPO_ROOT" && cargo build --release)
fi

if [ ! -x "$SRC" ]; then
    echo "error: $SRC is missing and could not be built" >&2
    exit 1
fi

mkdir -p "$BIN_DIR" "$AGENTS_DIR" "$(dirname "$LOG")"

# Stop any previous instance before replacing the binary.
launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true

# Copy to a stable path so the Accessibility grant survives rebuilds.
cp -f "$SRC" "$BIN"
chmod +x "$BIN"

# Ad-hoc sign with a stable identifier so macOS remembers the Accessibility
# permission for this binary. Rebuilding changes the signature, so re-run this
# script (and re-grant if prompted).
if command -v codesign >/dev/null 2>&1; then
    codesign --force --sign - --identifier "$LABEL" "$BIN" >/dev/null 2>&1 \
        || echo "warning: ad-hoc codesign failed; proceeding unsigned" >&2
fi

cat > "$PLIST" <<PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$BIN</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>StandardOutPath</key>
    <string>$LOG</string>
    <key>StandardErrorPath</key>
    <string>$LOG</string>
</dict>
</plist>
PLIST_EOF

plutil -lint "$PLIST" >/dev/null

launchctl bootstrap "$DOMAIN" "$PLIST"

echo "Installed and started: $LABEL"
echo "  binary: $BIN"
echo "  plist:  $PLIST"
echo "  log:    $LOG"
echo
echo "If the manager does not tile yet, grant Accessibility to the binary above:"
echo "  System Settings > Privacy & Security > Accessibility > add:"
echo "  $BIN"
echo "then restart it with: launchctl kickstart -k $DOMAIN/$LABEL"
