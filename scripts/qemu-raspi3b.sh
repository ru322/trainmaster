#!/usr/bin/env bash
# Buildroot のイメージを qemu-system-aarch64 の raspi3b で起動する(03 §8.3)。
# ファームウェアを通らないので config.txt は dtoverlay= だけを再現する。GPU(vc4)はない。カーネルと DTB を直接渡す。
# 使い方: scripts/qemu-raspi3b.sh [--dev] [--gui] [images ディレクトリ]
#   既定は配布用の build/rpi3-64/images。--dev で開発用の build/rpi3-64-dev/images
#   --gui: 画面(フレームバッファ)と USB キーボードを付ける
#   QEMU_APPEND: カーネルのコマンドラインに足す(例 QEMU_APPEND="-quiet" で quiet を外す)
#   QEMU_DTOVERLAYS: 当てる overlay を空白区切りで指定する(既定は config.txt の dtoverlay= すべて。
#     例 QEMU_DTOVERLAYS=miniuart-bt。vc4-kms-v3d を当てると --gui のフレームバッファが消える見込み)
#   SD は起動のたびに元の sdcard.img から写し直すので、QEMU の中で書いた内容は次の起動で消える
#   シリアル(ttyAMA0)は端末に出る。終了は Ctrl-A X
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
gui=0
out=${BR_OUTPUT:-$root/build/rpi3-64}
while [[ ${1:-} == --* ]]; do
  case $1 in
    --gui) gui=1 ;;
    --dev) out=$out-dev ;;
    *) echo "NG: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
images=${1:-$out/images}
qemu=${QEMU_SYSTEM_AARCH64:-qemu-system-aarch64}

if ! command -v "$qemu" >/dev/null; then
  echo "NG: $qemu not found (Ubuntu/Debian: package qemu-system-arm. 03 §2.4 (c))" >&2
  exit 1
fi
for f in Image bcm2710-rpi-3-b.dtb sdcard.img rpi-firmware/cmdline.txt; do
  [[ -f $images/$f ]] || { echo "NG: $images/$f not found (run mise run br:build first)" >&2; exit 1; }
done

# raspi3b の SD は大きさが 2 のべき乗でないと受け付けない(BV23)。元を壊さないよう、毎回写しを作って広げる
work=$(dirname "$images")/qemu
mkdir -p "$work"
sd=$work/sdcard.img
cp --sparse=always "$images/sdcard.img" "$sd"
size=$(stat -c %s "$sd")
pow2=1
while ((pow2 < size)); do pow2=$((pow2 * 2)); done
truncate -s "$pow2" "$sd"

# config.txt の dtoverlay= はファームウェアが DTB に当てるもの。QEMU では自分で当てる
# (miniuart-bt がないと PL011 が ttyAMA1 になり、console=ttyAMA0 とシリアルの getty が QEMU のシリアルに出ない)。パラメータは無視する
dtb=$images/bcm2710-rpi-3-b.dtb
if [[ -v QEMU_DTOVERLAYS ]]; then
  read -r -a overlays <<<"$QEMU_DTOVERLAYS"
else
  mapfile -t overlays < <(sed -n -E 's/^dtoverlay=([^,[:space:]]+).*/\1/p' "$images/rpi-firmware/config.txt")
fi
if ((${#overlays[@]})); then
  fdtoverlay=${FDTOVERLAY:-$(compgen -G "$(dirname "$images")/build/linux-*/scripts/dtc/fdtoverlay" | head -n 1 || command -v fdtoverlay || true)}
  if [[ ! -x $fdtoverlay ]]; then
    echo "NG: fdtoverlay not found; needed to apply dtoverlay= (${overlays[*]})" >&2
    echo "    install device-tree-compiler (Ubuntu/Debian), or set FDTOVERLAY=/path/to/fdtoverlay" >&2
    exit 1
  fi
  dtbos=()
  for o in "${overlays[@]}"; do dtbos+=("$images/rpi-firmware/overlays/$o.dtbo"); done
  "$fdtoverlay" -i "$dtb" -o "$work/qemu.dtb" "${dtbos[@]}"
  dtb=$work/qemu.dtb
fi

read -r -a cmdline <"$images/rpi-firmware/cmdline.txt"
for extra in ${QEMU_APPEND:-}; do
  if [[ $extra == -* ]]; then
    for i in "${!cmdline[@]}"; do
      if [[ ${cmdline[i]} == "${extra#-}" ]]; then unset 'cmdline[i]'; fi
    done
  else
    cmdline+=("$extra")
  fi
done

args=(
  -M raspi3b
  -kernel "$images/Image"
  -dtb "$dtb"
  -drive "file=$sd,if=sd,format=raw"
  -serial mon:stdio
)
if ((gui)); then
  args+=(-usb -device usb-kbd)
else
  args+=(-display none)
fi

exec "$qemu" "${args[@]}" -append "${cmdline[*]}"
