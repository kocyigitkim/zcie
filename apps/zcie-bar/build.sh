#!/bin/sh
# Builds "ZCIE Bar.app" (ADR-022, D-086) with swiftc and starts it.
#   --login     also start it at login (LaunchAgent com.zcie.bar)
#   --no-open   build only
# ZCIE_BAR_APP sets the app path (default: ~/Applications/ZCIE Bar.app).
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
app="${ZCIE_BAR_APP:-$HOME/Applications/ZCIE Bar.app}"
login=0
open_app=1
for arg in "$@"; do
    case "$arg" in
        --login) login=1 ;;
        --no-open) open_app=0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

# The app has no shell PATH, and macOS asks before an app runs a program in Desktop, Documents or
# Downloads: copy the zcie program of this shell (ZCIE_BIN overrides) to ~/.zcie/bin.
source_bin="${ZCIE_BIN:-$(command -v zcie || true)}"
zcie_bin="$HOME/.zcie/bin/zcie"
if [ -n "$source_bin" ] && [ "$source_bin" != "$zcie_bin" ]; then
    mkdir -p "$(dirname "$zcie_bin")"
    cp "$source_bin" "$zcie_bin.new" && mv "$zcie_bin.new" "$zcie_bin"
fi
# Record the project roots once from this shell; the app runs `zcie usage --no-disk`.
[ -x "$zcie_bin" ] && "$zcie_bin" usage >/dev/null 2>&1 || true
mkdir -p "$app/Contents/MacOS"
swiftc -O -parse-as-library -target "$(uname -m)-apple-macos14.0" \
    -o "$app/Contents/MacOS/ZCIEBar" "$here"/Sources/*.swift
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>com.zcie.bar</string>
    <key>CFBundleName</key><string>ZCIE Bar</string>
    <key>CFBundleExecutable</key><string>ZCIEBar</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSUIElement</key><true/>
    <key>ZCIEPath</key><string>$zcie_bin</string>
</dict>
</plist>
PLIST
# Ad hoc signature: macOS runs a local app with no developer certificate.
codesign --force --sign - "$app" >/dev/null 2>&1 || true

if [ "$login" -eq 1 ]; then
    agent="$HOME/Library/LaunchAgents/com.zcie.bar.plist"
    mkdir -p "$(dirname "$agent")"
    cat > "$agent" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.zcie.bar</string>
    <key>ProgramArguments</key><array><string>$app/Contents/MacOS/ZCIEBar</string><string>--background</string></array>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
PLIST
    echo "login item: $agent"
fi

if [ "$open_app" -eq 1 ]; then
    # A running copy keeps the old program: stop it first.
    pkill -x ZCIEBar 2>/dev/null || true
    open "$app"
fi
echo "built: $app"
