#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET="${1:-$(rustc -vV | sed -n 's/host: //p')}"
PYTHON="$(command -v python3 || command -v python)"
cargo build --locked --release --target "$TARGET"
rm -rf dist
mkdir -p dist/stage
"$PYTHON" scripts/notices.py "$TARGET" > dist/stage/THIRD_PARTY_NOTICES.txt
cp LICENSE LEGAL.md assets/fonts/OFL.txt assets/notices/Vitesse-LICENSE.txt dist/stage/
case "$TARGET" in
  *apple-darwin)
    APP="dist/Nucli.app"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "target/$TARGET/release/nucli" "$APP/Contents/MacOS/nucli"
    cp dist/stage/* "$APP/Contents/Resources/"
    cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Nucli</string>
    <key>CFBundleIdentifier</key><string>uk.polishch.nucli</string>
    <key>CFBundleExecutable</key><string>nucli</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>2.0.0</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    codesign --force --deep --sign - "$APP"
    ditto -c -k --keepParent "$APP" "dist/Nucli-$TARGET.zip"
    rm -rf "$APP"
    ;;
  *windows*)
    cp "target/$TARGET/release/nucli.exe" dist/stage/
    (cd dist/stage && 7z a "../Nucli-$TARGET.zip" . >/dev/null)
    ;;
  *)
    cp "target/$TARGET/release/nucli" dist/stage/
    tar -czf "dist/nucli-$TARGET.tar.gz" -C dist/stage .
    ;;
esac
rm -rf dist/stage
