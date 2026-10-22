#!/bin/sh
set -e
cd "$(dirname "$0")/.."
NAME=Nucli
ID=uk.polishch.nucli
ICON=AppIcon
CONFIG="${1:-release}"
if [ "$CONFIG" = dev ]; then
    NAME="Nucli Dev"
    ID=uk.polishch.nucli.dev
    ICON=AppIcon-Dev
    CONFIG=debug
fi
swift build -c "$CONFIG"
APP="build/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp ".build/$CONFIG/Nucli" "$APP/Contents/MacOS/Nucli"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
NOTICES="$APP/Contents/Resources/Notices"
mkdir -p "$NOTICES"
cp .build/checkouts/SwiftTerm/LICENSE "$NOTICES/SwiftTerm-LICENSE.txt"
cp .build/checkouts/swift-png/LICENSE "$NOTICES/swift-png-LICENSE.txt"
cp .build/checkouts/swift-png/NOTICE "$NOTICES/swift-png-NOTICE.txt"
cp .build/checkouts/h/LICENSE "$NOTICES/h-LICENSE.txt"
cp .build/checkouts/h/NOTICE "$NOTICES/h-NOTICE.txt"
ACTOOL="$(xcrun --find actool 2>/dev/null || echo /Applications/Xcode.app/Contents/Developer/usr/bin/actool)"
"$ACTOOL" "$PWD/Resources/$ICON.icon" --compile "$PWD/$APP/Contents/Resources" --platform macosx --minimum-deployment-target 14.0 --app-icon "$ICON" --output-partial-info-plist "$PWD/.build/icon-partial.plist" >/dev/null
cp LEGAL.md "$APP/Contents/Resources/LEGAL.md"
cp Resources/Vitesse-LICENSE.txt "$NOTICES/Vitesse-LICENSE.txt"
VERSION="$(git describe --tags --always 2>/dev/null || echo 0.1.0)"
SHORT_VERSION="$(git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//')"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$NAME</string>
    <key>CFBundleDisplayName</key><string>$NAME</string>
    <key>CFBundleIdentifier</key><string>$ID</string>
    <key>CFBundleExecutable</key><string>Nucli</string>
    <key>CFBundleIconFile</key><string>$ICON</string>
    <key>CFBundleIconName</key><string>$ICON</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${SHORT_VERSION:-0.1.0}</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Denys Polishchuk. MIT License.</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
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
codesign --force --sign - "$APP" >/dev/null
echo "$APP"
