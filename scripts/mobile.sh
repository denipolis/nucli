#!/bin/sh
set -e
cd "$(dirname "$0")/.."
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
case "${1:-simulator}" in
    simulator) TARGET=aarch64-apple-ios-sim BUILD=debug PLATFORM=iphonesimulator ;;
    device) TARGET=aarch64-apple-ios BUILD=release PLATFORM=iphoneos ;;
    *) echo "usage: scripts/mobile.sh [simulator|device]" >&2; exit 1 ;;
esac
OUT="target/$TARGET/$BUILD"
APP="$OUT/Nucli Uplink.app"
if command -v rustup >/dev/null; then rustup target add "$TARGET"; fi
if [ "$BUILD" = release ]; then
    cargo build -p nucli-uplink --release --target "$TARGET"
else
    cargo build -p nucli-uplink --target "$TARGET"
fi
rm -rf "$APP"
mkdir -p "$APP"
cp "$OUT/nucli-uplink" "$APP/"
cp mobile/ios/Info.plist "$APP/"
xcrun actool assets/icon/AppIcon.icon --compile "$APP" --platform "$PLATFORM" --app-icon AppIcon \
    --minimum-deployment-target "$(plutil -extract MinimumOSVersion raw mobile/ios/Info.plist)" \
    --target-device iphone --target-device ipad --output-partial-info-plist "$OUT/icon.plist" >/dev/null
/usr/libexec/PlistBuddy -c "Merge $OUT/icon.plist" "$APP/Info.plist"

if [ "$TARGET" = aarch64-apple-ios-sim ]; then
    codesign --force --sign - "$APP"
    if [ -n "$SIMULATOR" ]; then
        xcrun simctl install "$SIMULATOR" "$APP"
        xcrun simctl launch --terminate-running-process "$SIMULATOR" uk.polishch.nucli.uplink
    fi
    echo "$APP"
    exit
fi

BUNDLE_ID="${BUNDLE_ID:-uk.polishch.nucli.uplink}"
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
PROFILE=""
for FILE in "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"/* "$HOME/Library/MobileDevice/Provisioning Profiles"/*; do
    [ -f "$FILE" ] || continue
    security cms -D -i "$FILE" > "$OUT/profile.plist" 2>/dev/null || continue
    ID="$(plutil -extract Entitlements.application-identifier raw "$OUT/profile.plist")"
    EXPIRES="$(plutil -extract ExpirationDate raw "$OUT/profile.plist")"
    case "$ID" in
        *."$BUNDLE_ID") if [ "$EXPIRES" \> "$NOW" ]; then PROFILE="$FILE"; break; fi ;;
    esac
done
if [ -z "$PROFILE" ]; then
    echo "No valid provisioning profile for $BUNDLE_ID. See mobile/README.md." >&2
    exit 1
fi
plutil -replace CFBundleIdentifier -string "$BUNDLE_ID" "$APP/Info.plist"
cp "$PROFILE" "$APP/embedded.mobileprovision"
plutil -extract Entitlements xml1 -o "$OUT/entitlements.plist" "$OUT/profile.plist"
codesign --force --sign "${IDENTITY:-Apple Development}" --entitlements "$OUT/entitlements.plist" "$APP"
if [ -n "$DEVICE" ]; then
    xcrun devicectl device install app --device "$DEVICE" "$APP"
fi
echo "$APP"
