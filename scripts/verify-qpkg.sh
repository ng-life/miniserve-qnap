#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 PACKAGE.qpkg" >&2
  exit 2
fi

package="$(readlink -f "$1")"
[[ -f "$package" ]] || { echo "Package not found: $package" >&2; exit 1; }
command -v qbuild >/dev/null || { echo "qbuild is required" >&2; exit 1; }

work_dir="$(mktemp -d /tmp/miniserve-qpkg-verify.XXXXXX)"
cleanup() {
  find "$work_dir" -depth -delete 2>/dev/null || true
}
trap cleanup EXIT

checksum_file="${package}.md5"
if [[ -f "$checksum_file" ]]; then
  expected_md5="$(awk 'NR == 1 { print $1 }' "$checksum_file")"
  [[ "$expected_md5" =~ ^[0-9a-fA-F]{32}$ ]] || {
    echo "Invalid MD5 file: $checksum_file" >&2
    exit 1
  }
  echo "${expected_md5}  ${package}" | md5sum -c -
fi

qbuild --query info "$package"
qbuild --query dump "$package" >"$work_dir/settings.txt"
grep -Fxq 'QPKG_USE_PROXY="1"' "$work_dir/settings.txt" || {
  echo "QPKG_USE_PROXY must be enabled" >&2
  exit 1
}
grep -Fxq 'QPKG_PROXY_PATH="/miniserve"' "$work_dir/settings.txt" || {
  echo "QPKG_PROXY_PATH must be /miniserve" >&2
  exit 1
}
sha256sum "$package"

mkdir "$work_dir/control" "$work_dir/data"
qbuild --extract "$package" "$work_dir/control" >/dev/null

data_archive="$work_dir/control/data.tar.gz"
[[ -f "$data_archive" ]] || { echo "Missing data.tar.gz" >&2; exit 1; }
gzip -cd "$data_archive" >"$work_dir/data.tar" 2>/dev/null || [[ -s "$work_dir/data.tar" ]]
tar -tf "$work_dir/data.tar" | sort | tee "$work_dir/manifest.txt"

tar --numeric-owner -tvf "$work_dir/data.tar" >"$work_dir/metadata.txt"
if awk '$2 != "0/0" { print; bad = 1 } END { exit bad }' "$work_dir/metadata.txt"; then
  :
else
  echo "Every packaged payload entry must be owned by UID:GID 0:0 (admin:administrators on QTS)" >&2
  exit 1
fi
if awk 'substr($1, 2) != "rwxr-xr-x" { print; bad = 1 } END { exit bad }' "$work_dir/metadata.txt"; then
  :
else
  echo "Every packaged payload entry must have mode 0755" >&2
  exit 1
fi

if grep -Eq '(^|/)\.gitkeep$' "$work_dir/manifest.txt"; then
  echo "Generated placeholder .gitkeep was packaged" >&2
  exit 1
fi

for required in \
  ./.qpkg_icon.gif \
  ./.qpkg_icon_80.gif \
  ./.qpkg_icon_gray.gif \
  ./bin/miniserve \
  ./bin/miniserve-qnap-manager \
  ./licenses/LICENSE \
  ./licenses/MINISERVE-LICENSE \
  ./licenses/ICON-ATTRIBUTION.md; do
  grep -Fxq "$required" "$work_dir/manifest.txt" || {
    echo "Missing required package file: $required" >&2
    exit 1
  }
done

tar -xf "$work_dir/data.tar" -C "$work_dir/data"
find "$work_dir/data" -type f -perm /111 -exec file {} +
sh -n "$work_dir/data/miniserve-qnap.sh"
echo "QPKG verification completed successfully."
