#!/bin/sh
set -e
TARGET="${1:-aarch64-apple-ios-sim}"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
cargo build -p nucli-uplink --target "$TARGET"
APP="target/$TARGET/debug/Nucli Uplink.app"
rm -rf "$APP"
mkdir -p "$APP"
cp "target/$TARGET/debug/nucli-uplink" "$APP/"
cp mobile/ios/Info.plist "$APP/"
codesign --force --sign - "$APP"
if [ -n "$SIMULATOR" ]; then
    xcrun simctl install "$SIMULATOR" "$APP"
    xcrun simctl launch --terminate-running-process "$SIMULATOR" uk.polishch.nucli.uplink
fi
echo "$APP"
