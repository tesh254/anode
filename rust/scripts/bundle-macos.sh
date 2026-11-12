#!/bin/bash
# Build a self-contained, locally signed macOS app with a persistent Dock icon.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  echo "This script requires macOS." >&2
  exit 1
fi
cargo build --release
bundle="$PWD/target/release/Anode.app"
resources="$bundle/Contents/Resources"
mkdir -p "$bundle/Contents/MacOS" "$resources"
cp target/release/anode "$bundle/Contents/MacOS/anode"
icon_png=$(find target/release/build -path '*/anode-*/out/app-icon.png' -type f -exec stat -f '%m %N' {} \; | sort -nr | head -1 | cut -d' ' -f2-)
iconset=$(mktemp -d)/Anode.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$icon_png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$icon_png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$resources/Anode.icns"
rm -rf "$(dirname "$iconset")"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Anode</string>
<key>CFBundleDisplayName</key><string>Anode</string>
<key>CFBundleIdentifier</key><string>dev.anode.ide</string>
<key>CFBundleExecutable</key><string>anode</string>
<key>CFBundleIconFile</key><string>Anode</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$bundle"
echo "$bundle"
