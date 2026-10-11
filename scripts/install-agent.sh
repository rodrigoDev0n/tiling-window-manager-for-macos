#!/bin/sh
# Installs tiling_window as a per-user macOS LaunchAgent so it runs in the
# background at login. Re-run it after rebuilding to refresh the installed copy.
set -eu

LABEL="com.rodrigopoblete.tiling-window"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$REPO_ROOT/target/release/tiling_window"

BIN_DIR="$HOME/Library/Application Support/tiling-window"
APP="$BIN_DIR/TilingWindow.app"
APP_BIN="$APP/Contents/MacOS/tiling_window"
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

# Stop any previous instance before replacing the bundle.
launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true

# Package as a minimal .app bundle: macOS TCC tracks Accessibility grants for
# app bundles reliably and persistently, unlike a bare executable.
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp -f "$SRC" "$APP_BIN"
chmod +x "$APP_BIN"

cat > "$APP/Contents/Info.plist" <<INFO_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>$LABEL</string>
    <key>CFBundleName</key><string>TilingWindow</string>
    <key>CFBundleDisplayName</key><string>TilingWindow</string>
    <key>CFBundleExecutable</key><string>tiling_window</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleVersion</key><string>0.1.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>
INFO_EOF

# Sign with a stable self-signed identity so the Accessibility grant persists
# across rebuilds. An ad-hoc signature is keyed to the binary hash, which
# changes every build and forces a re-grant; a certificate leaf requirement is
# stable. The key/cert live beside the bundle, never in the repository.
CERT_CN="tiling_window self-signed"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
CERT_KEY="$BIN_DIR/tw-codesign.key"
CERT_PEM="$BIN_DIR/tw-codesign.pem"

if [ ! -f "$CERT_KEY" ] || [ ! -f "$CERT_PEM" ]; then
    echo "Creating self-signed code-signing certificate..."
    openssl req -new -newkey rsa:2048 -x509 -days 3650 -nodes \
        -keyout "$CERT_KEY" -out "$CERT_PEM" -subj "/CN=$CERT_CN" \
        -addext "keyUsage=critical,digitalSignature" \
        -addext "extendedKeyUsage=codeSigning" >/dev/null 2>&1
fi

if ! security find-identity -p codesigning -v "$KEYCHAIN" 2>/dev/null | grep -q "$CERT_CN"; then
    cat "$CERT_KEY" "$CERT_PEM" > "$BIN_DIR/tw-codesign-bundle.pem"
    security import "$BIN_DIR/tw-codesign-bundle.pem" -k "$KEYCHAIN" -T /usr/bin/codesign >/dev/null
    security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$CERT_PEM" >/dev/null 2>&1 || true
fi

codesign --force --deep --sign "$CERT_CN" --keychain "$KEYCHAIN" "$APP" >/dev/null 2>&1 \
    || echo "warning: codesign failed; proceeding with existing signature" >&2

cat > "$PLIST" <<PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$APP_BIN</string>
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
echo "  app:   $APP"
echo "  plist: $PLIST"
echo "  log:   $LOG"
echo
echo "Grant Accessibility to the app (once):"
echo "  System Settings > Privacy & Security > Accessibility > add:"
echo "  $APP"
echo "then restart it with: launchctl kickstart -k $DOMAIN/$LABEL"
