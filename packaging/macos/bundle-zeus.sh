#!/bin/bash
# Build Zeus.app from a GUI binary on macOS, as bundle.sh does for ZapFast.
#
#   packaging/macos/bundle-zeus.sh <binary> <output.app> <version>
#
# Ad-hoc signed, with the microphone and camera entitlements: Zeus has no
# Developer ID, so Gatekeeper asks to be told once (right-click, Open).
set -euo pipefail

binary="$1"
app="$2"
version="$3"
here="$(cd "$(dirname "$0")" && pwd)"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Library/LaunchAgents"

cp "$binary" "$app/Contents/MacOS/zeus"
chmod 755 "$app/Contents/MacOS/zeus"
sed "s/__VERSION__/${version%%-*}/g" "$here/Info-zeus.plist" > "$app/Contents/Info.plist"

# Starting in the tray at login: SMAppService registers this agent.
cp "$here/autostart-zeus.plist" "$app/Contents/Library/LaunchAgents/app.zeus.Zeus.autostart.plist"

iconset="$(mktemp -d)/zeus.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size "$here/../icons/zeus-1024.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$here/../icons/zeus-1024.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/zeus.icns"

# Notification sounds are played from the bundle by these names (src/notify.rs).
cp "$here/../../assets/sounds/receive.wav" "$app/Contents/Resources/zapfast-receive.wav"
cp "$here/../../assets/sounds/alert.wav" "$app/Contents/Resources/zapfast-alert.wav"

codesign --force --entitlements "$here/entitlements.plist" --sign - "$app"
codesign --verify --strict "$app"
echo "$app"
