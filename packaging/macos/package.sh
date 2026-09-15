#!/usr/bin/env bash
set -euo pipefail

target="${1:?Rust target triple is required}"
dist_root="${2:-dist}"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$project_root"

case "$target" in
  x86_64-apple-darwin|aarch64-apple-darwin) ;;
  *) echo "Unsupported macOS target: $target" >&2; exit 1 ;;
esac

version="$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
binary="$project_root/target/$target/release/miaozip"
icon="$(find "$project_root/target/$target/release/build" -path '*/out/miaozip-icon-1024.png' -print -quit)"
[[ -x "$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
[[ -f "$icon" ]] || { echo "Missing application icon" >&2; exit 1; }

dist="$project_root/$dist_root/$target"
workspace="$(mktemp -d)"
trap 'rm -rf -- "$workspace"' EXIT
mkdir -p "$dist"
install -m 0755 "$binary" "$dist/miaozip-$target"

iconset="$workspace/miaozip.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  doubled=$((size * 2))
  sips -z "$doubled" "$doubled" "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$workspace/miaozip.icns"

app="$workspace/MiaoZip.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 0755 "$binary" "$app/Contents/MacOS/miaozip"
install -m 0644 "$workspace/miaozip.icns" "$app/Contents/Resources/miaozip.icns"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

ditto -c -k --sequesterRsrc --keepParent "$app" "$dist/MiaoZip-$target.app.zip"
portable="$workspace/miaozip-$target"
mkdir -p "$portable"
install -m 0755 "$binary" "$portable/miaozip"
install -m 0644 README.md LICENSE "$portable/"
tar -czf "$dist/miaozip-$target.tar.gz" -C "$workspace" "miaozip-$target"

dmg_root="$workspace/dmg"
mkdir -p "$dmg_root"
ditto "$app" "$dmg_root/MiaoZip.app"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -volname "妙压" -srcfolder "$dmg_root" -ov -format UDZO "$dist/MiaoZip-$target.dmg"

pkgbuild \
  --component "$app" \
  --identifier com.tangluobo.miaozip \
  --version "$version" \
  --install-location /Applications \
  "$dist/MiaoZip-$target.pkg"

pkgutil --check-signature "$dist/MiaoZip-$target.pkg" || true
hdiutil imageinfo "$dist/MiaoZip-$target.dmg" >/dev/null
ls -lh "$dist"
