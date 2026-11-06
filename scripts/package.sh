#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET="${1:-$(rustc -vV | sed -n 's/host: //p')}"
PYTHON="$(command -v python3 || command -v python)"
BUILD="${CARGO_TARGET_DIR:-target}/$TARGET/release"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
cargo build --locked --release --target "$TARGET"
rm -rf dist
mkdir -p dist/stage
"$PYTHON" scripts/notices.py "$TARGET" > dist/stage/THIRD_PARTY_NOTICES.txt
cp LICENSE LEGAL.md assets/fonts/OFL.txt assets/notices/Vitesse-LICENSE.txt dist/stage/
case "$TARGET" in
  *apple-darwin)
    APP="dist/Nucli.app"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "$BUILD/nucli" "$APP/Contents/MacOS/nucli"
    cp dist/stage/* "$APP/Contents/Resources/"
    cp assets/icon/nucli.icns "$APP/Contents/Resources/AppIcon.icns"
    cp assets/icon/Assets.car "$APP/Contents/Resources/Assets.car"
    cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Nucli</string>
    <key>CFBundleDisplayName</key><string>Nucli</string>
    <key>CFBundleIdentifier</key><string>uk.polishch.nucli</string>
    <key>CFBundleExecutable</key><string>nucli</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIconName</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Denys Polishchuk. MIT License.</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>Folder</string>
            <key>CFBundleTypeRole</key><string>Viewer</string>
            <key>LSHandlerRank</key><string>None</string>
            <key>LSItemContentTypes</key><array><string>public.folder</string></array>
        </dict>
    </array>
</dict>
</plist>
PLIST
    codesign --force --deep --sign - "$APP"
    OUT="dist/Nucli-$TARGET.dmg"
    mkdir dist/dmg
    mv "$APP" dist/dmg/
    if ! command -v create-dmg >/dev/null || ! create-dmg --volname Nucli --volicon assets/icon/nucli.icns \
      --background assets/icon/dmg-background.tiff --window-pos 200 200 --window-size 660 400 --icon-size 128 \
      --text-size 13 --icon Nucli.app 180 190 --app-drop-link 480 190 --hide-extension Nucli.app \
      --no-internet-enable "$OUT" dist/dmg; then
      rm -f dist/rw.*.dmg "$OUT"
      ln -s /Applications dist/dmg/Applications
      hdiutil create -volname Nucli -srcfolder dist/dmg -ov -format UDZO "$OUT"
    fi
    rm -rf dist/dmg
    ;;
  *windows*)
    cp "$BUILD/nucli.exe" dist/stage/
    (cd dist/stage && 7z a "../Nucli-$TARGET.zip" . >/dev/null)
    ;;
  *)
    cp "$BUILD/nucli" assets/icon/nucli.png assets/icon/nucli.desktop dist/stage/
    tar -czf "dist/nucli-$TARGET.tar.gz" -C dist/stage .
    ;;
esac
rm -rf dist/stage
ls -la dist
