#!/usr/bin/env bash
set -euo pipefail

target="${1:?Rust target triple is required}"
dist_root="${2:-dist}"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$project_root"

version="$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
binary="$project_root/target/$target/release/miaozip"
icon="$(find "$project_root/target/$target/release/build" -path '*/out/miaozip-icon.png' -print -quit)"
[[ -x "$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
[[ -f "$icon" ]] || { echo "Missing application icon" >&2; exit 1; }

case "$target" in
  x86_64-unknown-linux-gnu)
    deb_arch="amd64"
    rpm_arch="x86_64"
    ;;
  aarch64-unknown-linux-gnu)
    deb_arch="arm64"
    rpm_arch="aarch64"
    ;;
  *)
    echo "Unsupported Linux target: $target" >&2
    exit 1
    ;;
esac

dist="$project_root/$dist_root/$target"
workspace="$(mktemp -d)"
trap 'rm -rf -- "$workspace"' EXIT
mkdir -p "$dist"

install -m 0755 "$binary" "$dist/miaozip-$target"

portable="$workspace/miaozip-$target"
mkdir -p "$portable"
install -m 0755 "$binary" "$portable/miaozip"
install -m 0644 README.md LICENSE packaging/linux/miaozip.desktop "$portable/"
install -m 0644 "$icon" "$portable/miaozip.png"
tar -czf "$dist/miaozip-$target.tar.gz" -C "$workspace" "miaozip-$target"

deb_root="$workspace/deb"
install -Dm0755 "$binary" "$deb_root/usr/bin/miaozip"
install -Dm0644 packaging/linux/miaozip.desktop "$deb_root/usr/share/applications/miaozip.desktop"
install -Dm0644 "$icon" "$deb_root/usr/share/icons/hicolor/256x256/apps/miaozip.png"
install -Dm0644 README.md "$deb_root/usr/share/doc/miaozip/README.md"
install -Dm0644 LICENSE "$deb_root/usr/share/doc/miaozip/copyright"
mkdir -p "$deb_root/DEBIAN"
installed_size="$(du -sk "$deb_root/usr" | cut -f1)"
cat > "$deb_root/DEBIAN/control" <<EOF
Package: miaozip
Version: $version
Section: utils
Priority: optional
Architecture: $deb_arch
Installed-Size: $installed_size
Depends: libc6, libgcc-s1
Recommends: xdg-desktop-portal | zenity
Maintainer: MiaoZip Contributors <noreply@example.com>
Homepage: https://github.com/tangluobo/miaozip
Description: Cross-platform graphical archive manager
 MiaoZip creates, browses, tests and extracts common archive formats.
EOF
dpkg-deb --root-owner-group --build "$deb_root" "$dist/miaozip-$target.deb"

rpm_sources="$workspace/rpm-sources"
rpm_root="$workspace/rpmbuild"
mkdir -p "$rpm_sources" "$rpm_root/BUILD" "$rpm_root/BUILDROOT" "$rpm_root/RPMS" "$rpm_root/SRPMS"
install -m 0755 "$binary" "$rpm_sources/miaozip"
install -m 0644 packaging/linux/miaozip.desktop "$rpm_sources/miaozip.desktop"
install -m 0644 "$icon" "$rpm_sources/miaozip.png"
install -m 0644 README.md LICENSE "$rpm_sources/"
rpm_version="${version//-/_}"
rpmbuild -bb packaging/linux/miaozip.spec \
  --target "$rpm_arch" \
  --define "_topdir $rpm_root" \
  --define "_sourcedir $rpm_sources" \
  --define "miaozip_version $rpm_version"
rpm_file="$(find "$rpm_root/RPMS" -type f -name '*.rpm' -print -quit)"
[[ -f "$rpm_file" ]] || { echo "RPM package was not produced" >&2; exit 1; }
install -m 0644 "$rpm_file" "$dist/miaozip-$target.rpm"

dpkg-deb --info "$dist/miaozip-$target.deb" >/dev/null
rpm -qip "$dist/miaozip-$target.rpm" >/dev/null
[[ "$(dpkg-deb -f "$dist/miaozip-$target.deb" Architecture)" == "$deb_arch" ]]
[[ "$(rpm -qp --queryformat '%{ARCH}' "$dist/miaozip-$target.rpm")" == "$rpm_arch" ]]
file "$dist/miaozip-$target"
ls -lh "$dist"
