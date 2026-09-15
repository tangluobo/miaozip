#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "Usage: $0 <rust-target> <archive-arch> <appimage-arch> <version>" >&2
  exit 2
fi

target=$1
archive_arch=$2
appimage_arch=$3
version=$4
project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$project_root"

case "$target:$archive_arch" in
  x86_64-unknown-linux-gnu:x86_64)
    deb_arch=amd64
    rpm_arch=x86_64
    expected_appimage_arch=x86_64
    ;;
  aarch64-unknown-linux-gnu:arm64)
    deb_arch=arm64
    rpm_arch=aarch64
    expected_appimage_arch=aarch64
    ;;
  *)
    echo "Unsupported Linux target and architecture: $target / $archive_arch" >&2
    exit 1
    ;;
esac
[[ "$appimage_arch" == "$expected_appimage_arch" ]] || {
  echo "AppImage architecture $appimage_arch does not match $target" >&2
  exit 1
}

metadata_version=$(cargo metadata --locked --no-deps --format-version 1 |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["packages"][0]["version"])')
[[ "$metadata_version" == "$version" ]] || {
  echo "Cargo version $metadata_version does not match package version $version" >&2
  exit 1
}

binary="$project_root/target/$target/release/miaozip"
icon=$(find "$project_root/target/$target/release/build" -path '*/out/miaozip-icon.png' -print -quit)
[[ -x "$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
[[ -f "$icon" ]] || { echo "Missing application icon" >&2; exit 1; }
command -v appimagetool >/dev/null || { echo "appimagetool is required" >&2; exit 1; }

bundle_name="miaozip-${version}-linux-${archive_arch}"
dist_dir="$project_root/dist"
workspace=$(mktemp -d)
trap 'rm -rf -- "$workspace"' EXIT
bundle_dir="$workspace/$bundle_name"
mkdir -p "$bundle_dir" "$dist_dir"

install -m 0755 "$binary" "$bundle_dir/miaozip"
install -m 0644 README.md LICENSE THIRD_PARTY_NOTICES.md \
  packaging/linux/miaozip.desktop "$bundle_dir/"
install -m 0644 "$icon" "$bundle_dir/miaozip.png"

rm -f "$dist_dir/$bundle_name.zip" "$dist_dir/$bundle_name.tar.gz" \
  "$dist_dir/$bundle_name.deb" "$dist_dir/$bundle_name.rpm" \
  "$dist_dir/$bundle_name.AppImage"
(cd "$workspace" && zip -qr "$dist_dir/$bundle_name.zip" "$bundle_name")
tar -czf "$dist_dir/$bundle_name.tar.gz" -C "$workspace" "$bundle_name"

deb_root="$workspace/deb"
install -Dm0755 "$binary" "$deb_root/usr/bin/miaozip"
install -Dm0644 packaging/linux/miaozip.desktop \
  "$deb_root/usr/share/applications/miaozip.desktop"
install -Dm0644 "$icon" "$deb_root/usr/share/icons/hicolor/256x256/apps/miaozip.png"
install -Dm0644 README.md "$deb_root/usr/share/doc/miaozip/README.md"
install -Dm0644 LICENSE "$deb_root/usr/share/doc/miaozip/copyright"
install -Dm0644 THIRD_PARTY_NOTICES.md \
  "$deb_root/usr/share/doc/miaozip/THIRD_PARTY_NOTICES.md"
mkdir -p "$deb_root/DEBIAN"
installed_size=$(du -sk "$deb_root/usr" | cut -f1)
cat > "$deb_root/DEBIAN/control" <<EOF
Package: miaozip
Version: $version
Section: utils
Priority: optional
Architecture: $deb_arch
Installed-Size: $installed_size
Depends: libc6 (>= 2.31), libgcc-s1, libstdc++6, xdg-utils
Recommends: xdg-desktop-portal | zenity
Maintainer: MiaoZip Contributors <noreply@example.com>
Homepage: https://github.com/tangluobo/miaozip
Description: Cross-platform graphical archive manager
 MiaoZip creates, browses, tests and extracts common archive formats.
EOF
dpkg-deb --root-owner-group --build "$deb_root" "$dist_dir/$bundle_name.deb"

rpm_sources="$workspace/rpm-sources"
rpm_root="$workspace/rpmbuild"
mkdir -p "$rpm_sources" "$rpm_root/BUILD" "$rpm_root/BUILDROOT" \
  "$rpm_root/RPMS" "$rpm_root/SRPMS"
install -m 0755 "$binary" "$rpm_sources/miaozip"
install -m 0644 packaging/linux/miaozip.desktop "$rpm_sources/miaozip.desktop"
install -m 0644 "$icon" "$rpm_sources/miaozip.png"
install -m 0644 README.md LICENSE THIRD_PARTY_NOTICES.md "$rpm_sources/"
rpm_version=${version//-/_}
rpmbuild -bb packaging/linux/miaozip.spec \
  --target "$rpm_arch" \
  --define "_topdir $rpm_root" \
  --define "_sourcedir $rpm_sources" \
  --define "miaozip_version $rpm_version"
rpm_file=$(find "$rpm_root/RPMS" -type f -name '*.rpm' -print -quit)
[[ -f "$rpm_file" ]] || { echo "RPM package was not produced" >&2; exit 1; }
install -m 0644 "$rpm_file" "$dist_dir/$bundle_name.rpm"

app_dir="$workspace/MiaoZip.AppDir"
install -Dm0755 "$binary" "$app_dir/usr/bin/miaozip"
install -Dm0755 packaging/linux/AppRun "$app_dir/AppRun"
install -Dm0644 packaging/linux/miaozip.desktop "$app_dir/miaozip.desktop"
install -Dm0644 packaging/linux/miaozip.desktop \
  "$app_dir/usr/share/applications/miaozip.desktop"
install -Dm0644 "$icon" "$app_dir/miaozip.png"
install -Dm0644 "$icon" "$app_dir/usr/share/icons/hicolor/256x256/apps/miaozip.png"
ARCH="$appimage_arch" APPIMAGE_EXTRACT_AND_RUN=1 appimagetool \
  "$app_dir" "$dist_dir/$bundle_name.AppImage"

dpkg-deb --info "$dist_dir/$bundle_name.deb" >/dev/null
rpm -qip "$dist_dir/$bundle_name.rpm" >/dev/null
[[ "$(dpkg-deb -f "$dist_dir/$bundle_name.deb" Architecture)" == "$deb_arch" ]]
[[ "$(rpm -qp --queryformat '%{ARCH}' "$dist_dir/$bundle_name.rpm")" == "$rpm_arch" ]]
file "$dist_dir/$bundle_name.AppImage"
ls -lh "$dist_dir"
