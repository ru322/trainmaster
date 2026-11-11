#!/usr/bin/env bash
# scripts/toolchain-sources.sha256 の一覧(ツールチェインの glibc・gcc のソースとライセンス文)を Bootlin から
# $BR2_DL_DIR/toolchain-bootlin に取得し、SHA-256 で検証する(03 §6.4。make-dist.sh と CI の dl キャッシュの準備で使う)
# 引数に legal-info の manifest.csv を渡すと、イメージのツールチェインが一覧のものと同じかも確かめる
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cache=${BR2_DL_DIR:-$root/build/dl}/toolchain-bootlin
list=$root/scripts/toolchain-sources.sha256
url=https://toolchains.bootlin.com/downloads/releases
manifest=${1:-}

ng() {
  echo "NG: $*" >&2
  exit 1
}

want=$(sed -n 's/^# toolchain: //p' "$list")
[[ -n $want ]] || ng "no '# toolchain:' line in $list"
if [[ -n $manifest ]]; then
  have=$(awk -F '","' '$1 == "\"toolchain-external-bootlin" { sub(/\.tar\.[a-z0-9]+$/, "", $5); print $5 }' "$manifest")
  [[ $have == "$want" ]] || ng "the image uses toolchain '$have' but $list is for '$want' (rebuild the list from $url)"
fi

while read -r sum path; do
  [[ -z $sum || $sum == \#* ]] && continue
  f=$cache/$path
  if [[ -f $f ]] && echo "$sum  $f" | sha256sum --check --status; then continue; fi
  echo "downloading $url/$path"
  mkdir -p "$(dirname "$f")"
  curl --fail --location --retry 3 --silent --show-error -o "$f.part" "$url/$path" </dev/null
  mv "$f.part" "$f"
  if ! echo "$sum  $f" | sha256sum --check --status; then
    rm -f "$f"
    ng "SHA-256 mismatch: $url/$path"
  fi
done <"$list"
echo "toolchain sources of $want: OK ($cache)"
