#!/bin/sh
set -e
TMP="$(mktemp -d)"
trap 'hdiutil detach -quiet "$TMP/mnt" 2>/dev/null; rm -rf "$TMP"' EXIT
gh release download -R denipolis/nucli -p Nucli.dmg -D "$TMP"
hdiutil attach -nobrowse -quiet -mountpoint "$TMP/mnt" "$TMP/Nucli.dmg"
rm -rf /Applications/Nucli.app
cp -R "$TMP/mnt/Nucli.app" /Applications/
echo "Installed $(gh release view -R denipolis/nucli --json tagName -q .tagName) to /Applications/Nucli.app"
