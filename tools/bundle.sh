#!/usr/bin/env bash
# Builds target/Odeion.app: the release binary, Info.plist and the icon.
# The icon is generated from bundle/icon.svg (needs resvg and Apple's iconutil).
# ODEION_BINARY uses a binary built already (the release workflow's universal one);
# ODEION_VERSION sets the version the app shows (else Cargo.toml's).
set -euo pipefail
cd "$(dirname "$0")/.."

binary="${ODEION_BINARY:-target/release/odeion}"
if [ -z "${ODEION_BINARY:-}" ]; then
    cargo build --release
fi

app=target/Odeion.app
iconset=target/Odeion.iconset
rm -rf "$app" "$iconset"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$iconset"

for size in 16 32 128 256 512; do
    resvg -w $size -h $size bundle/icon.svg "$iconset/icon_${size}x${size}.png"
    resvg -w $((size * 2)) -h $((size * 2)) bundle/icon.svg "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Odeion.icns"
rm -rf "$iconset"

cp "$binary" "$app/Contents/MacOS/odeion"
cp bundle/Info.plist "$app/Contents/Info.plist"
version="${ODEION_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
for key in CFBundleShortVersionString CFBundleVersion; do
    /usr/libexec/PlistBuddy -c "Set :$key $version" "$app/Contents/Info.plist"
done
# Ad-hoc signature, so macOS treats it as one app (permissions, Gatekeeper on this Mac).
codesign --force --sign - "$app" >/dev/null
echo "$app"
