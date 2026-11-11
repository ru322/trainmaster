#!/bin/sh
# 引数: $1 = TARGET_DIR
# inittab は Buildroot のもの(getty と remount の行は defconfig で決まる)に tty1 の tm-launch を足す(03 §5.3)。
# overlay に inittab を置くと、Buildroot が先に書いたシリアルの getty の行(開発用)が消えるので置かない
set -eu

target=$1
inittab=$target/etc/inittab

sed -i '/^tty1::/d' "$inittab"
if ! grep -q '# GENERIC_SERIAL$' "$inittab"; then
	echo "post-build.sh: no '# GENERIC_SERIAL' line in $inittab" >&2
	exit 1
fi
sed -i '/# GENERIC_SERIAL$/a\
tty1::respawn:/usr/libexec/tm-launch' "$inittab"

# crontab がないので crond は要らない
rm -f "$target/etc/init.d/S50crond"

# data パーティションのマウント点(03 §5.7)。git は空のディレクトリを持てないので overlay に置かない
mkdir -p "$target/data"
