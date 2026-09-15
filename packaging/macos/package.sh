#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <rust-target> <archive-arch> <version>" >&2
  exit 2
fi

target=$1
archive_arch=$2
version=$3
project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$project_root"

case "$target:$archive_arch" in
  x86_64-apple-darwin:x86_64|aarch64-apple-darwin:arm64) ;;
  *)
    echo "Unsupported macOS target and architecture: $target / $archive_arch" >&2
    exit 1
    ;;
esac

metadata_version=$(cargo metadata --locked --no-deps --format-version 1 |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["packages"][0]["version"])')
[[ "$metadata_version" == "$version" ]] || {
  echo "Cargo version $metadata_version does not match package version $version" >&2
  exit 1
}

binary="$project_root/target/$target/release/miaozip"
icon=$(find "$project_root/target/$target/release/build" \
  -path '*/out/miaozip-icon-1024.png' -print -quit)
[[ -x "$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
[[ -f "$icon" ]] || { echo "Missing application icon" >&2; exit 1; }

bundle_name="miaozip-${version}-macos-${archive_arch}"
dist_dir="$project_root/dist"
workspace=$(mktemp -d)
trap 'rm -rf -- "$workspace"' EXIT
app="$workspace/MiaoZip.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$dist_dir"

iconset="$workspace/miaozip.iconset"
mkdir -p "$iconset"
while read -r pixels filename; do
  sips -z "$pixels" "$pixels" "$icon" --out "$iconset/$filename" >/dev/null
done <<'SIZES'
16 icon_16x16.png
32 icon_16x16@2x.png
32 icon_32x32.png
64 icon_32x32@2x.png
128 icon_128x128.png
256 icon_128x128@2x.png
256 icon_256x256.png
512 icon_256x256@2x.png
512 icon_512x512.png
1024 icon_512x512@2x.png
SIZES
iconutil -c icns "$iconset" -o "$app/Contents/Resources/miaozip.icns"

install -m 0755 "$binary" "$app/Contents/MacOS/miaozip"
install -m 0644 LICENSE "$app/Contents/Resources/LICENSE.txt"
install -m 0644 THIRD_PARTY_NOTICES.md \
  "$app/Contents/Resources/THIRD_PARTY_NOTICES.md"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

zip_output="$dist_dir/$bundle_name.zip"
tar_output="$dist_dir/$bundle_name.tar.gz"
dmg_output="$dist_dir/$bundle_name.dmg"
pkg_output="$dist_dir/$bundle_name.pkg"
rm -f "$zip_output" "$tar_output" "$dmg_output" "$pkg_output"
ditto -c -k --sequesterRsrc --keepParent "$app" "$zip_output"
COPYFILE_DISABLE=1 tar -czf "$tar_output" -C "$workspace" MiaoZip.app

dmg_root="$workspace/dmg-root"
mkdir -p "$dmg_root"
ditto "$app" "$dmg_root/MiaoZip.app"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -quiet -volname "妙压" -srcfolder "$dmg_root" -ov -format UDZO \
  "$dmg_output"

pkg_root="$workspace/pkg-root"
mkdir -p "$pkg_root/Applications"
ditto "$app" "$pkg_root/Applications/MiaoZip.app"
pkgbuild --quiet --root "$pkg_root" --identifier com.tangluobo.miaozip \
  --version "$version" --install-location / "$pkg_output"

hdiutil imageinfo "$dmg_output" >/dev/null
pkgutil --payload-files "$pkg_output" >/dev/null
ls -lh "$dist_dir"
