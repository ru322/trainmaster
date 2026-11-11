#!/bin/sh
# Buildroot のトップディレクトリで実行される。sdcard.img と展示用の sdcard-exhibit.img を BINARIES_DIR に作る(03 §6.1)
set -eu

fw=$BINARIES_DIR/rpi-firmware
# ファームウェアは cmdline.txt の 1 行目しか読まない
if [ "$(wc -l <"$fw/cmdline.txt")" -gt 1 ]; then
	echo "post-image.sh: $fw/cmdline.txt must be one line" >&2
	exit 1
fi
printf '%s trainmaster.save=off\n' "$(cat "$fw/cmdline.txt")" >"$fw/cmdline-exhibit.txt"

support/scripts/genimage.sh -c "$(dirname "$0")/genimage.cfg"
