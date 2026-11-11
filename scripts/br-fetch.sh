#!/usr/bin/env bash
# Buildroot の固定版の tarball を取得し、scripts/buildroot.sha256 で検証して $BR_SRC に展開する(03 §7.3、BD27)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
version=${BUILDROOT_VERSION:?BUILDROOT_VERSION is not set (run via mise)}
src=${BR_SRC:-$root/build/buildroot-$version}
dl=${BR2_DL_DIR:-$root/build/dl}/buildroot
name=buildroot-$version.tar.xz
url=https://buildroot.org/downloads/$name
hash_file=$root/scripts/buildroot.sha256

expected=$(awk -v n="$name" '$2 == n { print $1 }' "$hash_file")
if [[ ! $expected =~ ^[0-9a-f]{64}$ ]]; then
  echo "NG: no SHA-256 for $name in $hash_file" >&2
  echo "    verify the hash against $url.sign before adding it" >&2
  exit 1
fi

stamp=$src/.tm-fetched
if [[ -f $stamp && $(cat "$stamp") == "$expected" ]]; then
  echo "Buildroot $version: already extracted at $src"
  exit 0
fi

mkdir -p "$dl"
tarball=$dl/$name
if [[ ! -f $tarball ]] || ! echo "$expected  $tarball" | sha256sum --check --status; then
  echo "downloading $url"
  curl --fail --location --retry 3 --silent --show-error -o "$tarball.part" "$url"
  mv "$tarball.part" "$tarball"
fi
if ! echo "$expected  $tarball" | sha256sum --check --status; then
  echo "NG: SHA-256 mismatch for $tarball (expected $expected)" >&2
  rm -f "$tarball"
  exit 1
fi
echo "SHA-256 OK: $name"

parent=$(dirname "$src")
mkdir -p "$parent"
tmp=$(mktemp -d "$parent/.br-extract.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
tar -xJf "$tarball" -C "$tmp"
rm -rf "$src"
mv "$tmp/buildroot-$version" "$src"
echo "$expected" > "$stamp"
echo "Buildroot $version: extracted at $src"
