#!/usr/bin/env bash
# 配布物を DIST_DIR に作る(03 §6.2、BD11。家用と展示用の 2 つのイメージ)。mise run br:dist から呼ぶ(先に make legal-info が走る)
# WITH_LEGAL_INFO=0: legal-info の tar.gz とソースの tar.gz を作らない(タグ以外の CI ビルド。03 §7.5)
# ALLOW_DIRTY=1: 作業ツリーに未コミットの変更があっても legal-info 付きの配布物を作る(ソースの tar は HEAD のまま)
# legal-info の tar には、Buildroot の legal-info の写しに extra/ を足して入れる(03 §6.4。中身は extra/README.txt)。
#   Buildroot の tarball、カーネルの .config、ツールチェインの glibc・gcc のソース(scripts/fetch-toolchain-sources.sh。
#   dl になければ Bootlin から取得する)、依存クレート(cargo-about)と Rust の標準ライブラリのライセンス文と著作権表示
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${BR_OUTPUT:-$root/build/rpi3-64}
dist=${DIST_DIR:-$root/build/dist}
dl=${BR2_DL_DIR:-$root/build/dl}
with_legal=${WITH_LEGAL_INFO:-1}
img=$out/images/sdcard.img
img_exhibit=$out/images/sdcard-exhibit.img
bin=$out/target/usr/bin/trainmaster
toolchain_list=$root/scripts/toolchain-sources.sha256
# GitHub Release の 1 ファイルの上限は 2 GiB 未満。超える tar は分割する
max_asset_bytes=${MAX_ASSET_BYTES:-$((2000 * 1024 * 1024))}

ng() {
  echo "NG: $*" >&2
  exit 1
}

for f in "$img" "$img_exhibit"; do
  [[ -f $f ]] || ng "$f not found (run mise run br:build first)"
done
image_licenses=$out/target/usr/share/licenses/trainmaster
for f in NOTICE THIRD-PARTY-LICENSES.txt rust-std/COPYRIGHT-library.html; do
  [[ -f $image_licenses/$f ]] || ng "$image_licenses/$f is not in the image (run mise run br:rebuild-app)"
done

tag=$(git -C "$root" describe --tags --always --dirty)
commit=$(git -C "$root" rev-parse HEAD)
last_installed=$(find "$out/build" -maxdepth 2 -path "$out/build/trainmaster-*/.stamp_target_installed" -printf '%T@ %h\n' |
  sort -n | tail -n 1 | cut -d ' ' -f 2-)
[[ $last_installed == "$out/build/trainmaster-$tag" ]] ||
  ng "the last installed trainmaster is '${last_installed##*/}', not trainmaster-$tag (run mise run br:rebuild-app)"
grep -aqF -- "TM_GAME_VERSION=$tag" "$bin" || ng "$bin does not contain the version $tag (run mise run br:rebuild-app)"
os_release=$out/target/usr/lib/os-release
br_version=$(sed -n 's/^VERSION_ID=//p' "$os_release")
[[ -n $br_version ]] || ng "no VERSION_ID in $os_release"

release_grade=0
if [[ $with_legal != 0 && ${ALLOW_DIRTY:-0} != 1 ]]; then release_grade=1; fi

# Buildroot の setlocalversion が外側の git の版(-g<hash>-dirty やアプリのタグ)を取り込んでいないか
os_version=$(sed -n 's/^VERSION=//p' "$os_release")
if [[ $os_version != "$br_version" ]]; then
  msg="os-release VERSION '$os_version' != '$br_version': Buildroot took its version from the outer git (set GIT_CEILING_DIRECTORIES as in mise.toml and run make again)"
  if [[ $release_grade == 1 ]]; then ng "$msg"; fi
  echo "WARNING: $msg" >&2
fi

legal=$out/legal-info
kernel_config=$out/build/linux-custom/.config
br_tarball=$dl/buildroot/buildroot-$br_version.tar.xz
toolchain_cache=$dl/toolchain-bootlin
rust_bin_dirs=()
rust_doc=rustc/share/doc/rust

if [[ $with_legal != 0 ]]; then
  [[ -d $legal ]] || ng "$legal not found (run make -C $out legal-info)"
  [[ -f $legal/licenses/trainmaster-$tag/NOTICE ]] ||
    ng "legal-info has no license files of trainmaster-$tag (run make -C $out legal-info again)"
  if [[ -n $(git -C "$root" status --porcelain) && ${ALLOW_DIRTY:-0} != 1 ]]; then
    ng "the working tree is dirty; the source tar (git archive HEAD) would not match the image (ALLOW_DIRTY=1 to override)"
  fi
  [[ -f $kernel_config ]] || ng "$kernel_config not found"
  [[ -f $br_tarball ]] || ng "$br_tarball not found (run mise run br:fetch)"
  awk -v n="${br_tarball##*/}" '$2 == n { print $1 "  " f }' f="$br_tarball" "$root/scripts/buildroot.sha256" |
    sha256sum --check --status || ng "SHA-256 of $br_tarball does not match scripts/buildroot.sha256"
  shopt -s nullglob
  rust_bin_dirs=("$out"/build/host-rust-bin-*/)
  shopt -u nullglob
  [[ ${#rust_bin_dirs[@]} == 1 ]] || ng "expected one $out/build/host-rust-bin-*/ (got ${#rust_bin_dirs[@]})"
  for f in COPYRIGHT LICENSE-MIT LICENSE-APACHE "$rust_doc/COPYRIGHT-library.html"; do
    [[ -f ${rust_bin_dirs[0]}$f ]] || ng "${rust_bin_dirs[0]}$f not found"
  done
  cargo about --version >/dev/null || ng "cargo-about is not installed (mise install)"
  BR2_DL_DIR=$dl "$root/scripts/fetch-toolchain-sources.sh" "$legal/manifest.csv"
fi

name=trainmaster-$tag
mkdir -p "$dist"
rm -f "$dist"/trainmaster-* "$dist"/BUILDINFO.txt "$dist"/SHA256SUMS

files=("$name-rpi3-arm64.img.xz" "$name-rpi3-arm64-exhibit.img.xz")
echo "compressing $img"
xz -T0 -9 -c "$img" >"$dist/$name-rpi3-arm64.img.xz"
echo "compressing $img_exhibit"
xz -T0 -9 -c "$img_exhibit" >"$dist/$name-rpi3-arm64-exhibit.img.xz"

add_legal_extra() {
  local l=$1 x=$1/extra sum path rel
  mkdir -p "$x/buildroot" "$x/linux" "$x/toolchain" "$x/rust/rust-std"
  cp "$br_tarball" "$x/buildroot/"
  cp "$kernel_config" "$x/linux/kernel.config"
  while read -r sum path; do
    [[ -z $sum || $sum == \#* ]] && continue
    mkdir -p "$x/toolchain/$(dirname "$path")"
    cp -l "$toolchain_cache/$path" "$x/toolchain/$path" 2>/dev/null || cp "$toolchain_cache/$path" "$x/toolchain/$path"
  done <"$toolchain_list"
  CARGO_HOME=$dl/br-cargo-home "$root/scripts/rust-licenses.sh" "$x/rust/THIRD-PARTY-LICENSES.txt"
  cmp -s "$x/rust/THIRD-PARTY-LICENSES.txt" "$image_licenses/THIRD-PARTY-LICENSES.txt" ||
    ng "the crate licenses in the image are out of date (run mise run br:rust-licenses, then mise run br:rebuild-app)"
  cp "${rust_bin_dirs[0]}"{COPYRIGHT,LICENSE-MIT,LICENSE-APACHE,"$rust_doc/COPYRIGHT-library.html"} "$x/rust/rust-std/"
  rel=$(sed -n 's/^# toolchain: //p' "$toolchain_list")
  cat >"$x/README.txt" <<EOF
Additions to the Buildroot legal-info, made by scripts/make-dist.sh of Train Master $tag

buildroot/    Source tarball of Buildroot $br_version, the build system that made this image
              (answers "the Buildroot source code has not been saved" in ../README).
linux/kernel.config
              The configuration the Linux kernel in this image was built with
              (kernel source: ../sources/linux-custom/).
toolchain/    Source code and license texts of the toolchain parts installed in the image:
              glibc (libc, libm, ld-linux-aarch64, ...) and the gcc runtime (libgcc_s, libstdc++,
              libatomic, libgomp). Toolchain: $rel, from https://toolchains.bootlin.com/downloads/releases/
              (same relative paths). toolchain/toolchains/aarch64/summaries/$rel.csv lists all
              its components. ../sources/toolchain-external-bootlin-*/ is the prebuilt toolchain
              itself (answers "toolchain-external-bootlin: cannot save license" in ../README).
rust/THIRD-PARTY-LICENSES.txt
              License texts of the Rust crates linked into /usr/bin/trainmaster.
rust/rust-std/
              Copyright and license of the Rust standard library ($(sed -n 's/^release: //p' <<<"$("$out/host/bin/rustc" -vV)")), also linked
              into it: COPYRIGHT, LICENSE-MIT, LICENSE-APACHE, and COPYRIGHT-library.html with the
              copyright notices of the crates and files built into the standard library.

The source code of Train Master itself is $name-src.tar.gz, next to this archive.
EOF
  { cat "$l/README"; echo; echo "See extra/README.txt for the files added by Train Master (Buildroot, kernel .config, toolchain sources, Rust licenses)."; } >"$l/README.new"
  mv "$l/README.new" "$l/README"
}

if [[ $with_legal != 0 ]]; then
  # Buildroot の legal-info は変えず、ハードリンクの写しに足す(sed -i と mv は写しの側のリンクだけを置き換える)
  stage=$out/legal-info.dist
  rm -rf "$stage"
  mkdir -p "$stage"
  trap 'rm -rf "$stage"' EXIT
  cp -al "$legal" "$stage/legal-info"
  sl=$stage/legal-info
  # ビルドした機械のパスを配布物に残さない
  for f in manifest.csv host-manifest.csv buildroot.config; do
    [[ -f $sl/$f ]] || continue
    sed -i -e "s#$root#<repo>#g" "$sl/$f"
    if [[ ${#HOME} -gt 1 ]]; then sed -i -e "s#$HOME#<home>#g" "$sl/$f"; fi
  done
  add_legal_extra "$sl"
  rm -f "$sl/legal-info.sha256"
  (cd "$sl" && find ./* -type f -exec sha256sum {} + | sed 's#  \./#  #' | LC_ALL=C sort -k2 >.legal-info.sha256 && mv .legal-info.sha256 legal-info.sha256)
  tar -C "$stage" -czf "$dist/$name-legal-info.tar.gz" legal-info
  rm -rf "$stage"
  git -C "$root" archive --format=tar.gz --prefix="$name/" -o "$dist/$name-src.tar.gz" HEAD
  for f in "$name-legal-info.tar.gz" "$name-src.tar.gz"; do
    if (($(stat -c %s "$dist/$f") > max_asset_bytes)); then
      echo "splitting $f (larger than $max_asset_bytes bytes; join with: cat $f.part* > $f)"
      split -b "$max_asset_bytes" -d -a 2 "$dist/$f" "$dist/$f.part"
      rm "$dist/$f"
      for p in "$dist/$f".part*; do files+=("${p##*/}"); done
    else
      files+=("$f")
    fi
  done
fi

{
  echo "tag: $tag"
  echo "commit: $commit"
  echo "buildroot: $br_version"
  echo "sha256 of $name-rpi3-arm64.img (after xz -d): $(sha256sum <"$img" | cut -d ' ' -f 1)"
  echo "sha256 of $name-rpi3-arm64-exhibit.img (after xz -d): $(sha256sum <"$img_exhibit" | cut -d ' ' -f 1)"
  echo
  echo "buildroot rustc (shipped binary):"
  "$out/host/bin/rustc" -vV
  echo
  echo "mise rustc (tests):"
  if command -v rustc >/dev/null; then rustc -vV; else echo "(not found)"; fi
} >"$dist/BUILDINFO.txt"
files+=(BUILDINFO.txt)

(cd "$dist" && sha256sum "${files[@]}") >"$dist/SHA256SUMS"

ls -l "$dist"
