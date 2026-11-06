#!/bin/sh
set -e
REPO=denipolis/nucli
ARCH="$(uname -m)"
[ "$ARCH" = arm64 ] && ARCH=aarch64
TMP="$(mktemp -d)"
trap 'hdiutil detach -quiet "$TMP/mnt" 2>/dev/null || true; rm -rf "$TMP"' EXIT
case "$(uname -s)" in
  Darwin)
    gh release download -R "$REPO" -p "Nucli-$ARCH-apple-darwin.dmg" -D "$TMP"
    hdiutil attach -nobrowse -quiet -mountpoint "$TMP/mnt" "$TMP"/Nucli-*.dmg
    rm -rf /Applications/Nucli.app
    cp -R "$TMP/mnt/Nucli.app" /Applications/
    xattr -dr com.apple.quarantine /Applications/Nucli.app || true
    TARGET=/Applications/Nucli.app
    ;;
  Linux)
    gh release download -R "$REPO" -p "nucli-$ARCH-unknown-linux-gnu.tar.gz" -D "$TMP"
    tar -xzf "$TMP"/nucli-*.tar.gz -C "$TMP"
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/512x512/apps"
    cp "$TMP/nucli" "$HOME/.local/bin/nucli"
    cp "$TMP/nucli.desktop" "$HOME/.local/share/applications/nucli.desktop"
    cp "$TMP/nucli.png" "$HOME/.local/share/icons/hicolor/512x512/apps/nucli.png"
    TARGET="$HOME/.local/bin/nucli"
    ;;
  *)
    echo "Download the Windows build from https://github.com/$REPO/releases" >&2
    exit 1
    ;;
esac
echo "Installed $(gh release view -R "$REPO" --json tagName -q .tagName) to $TARGET"
