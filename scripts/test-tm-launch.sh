#!/usr/bin/env bash
# tm-launch の模擬テスト(16 APP-T-32、03 §5.3)。mise run test:launch
# TM_LAUNCH_ROOT を一時ディレクトリにし、決まった終了コードで終わる偽の trainmaster と、呼ばれたことを記録する偽の sync・poweroff・sleep を使う。
# 偽のコマンドは関数で差し込む(BusyBox の ash は sleep などのアプレットを PATH より先に使う設定がありうるため)。
# sh(CI では dash)と、あれば BusyBox の ash(実機と同じ)で回す
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
launch=$root/br2-external/board/rpi3-64/rootfs-overlay/usr/libexec/tm-launch
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# shellcheck disable=SC2016
fakes='rec() { printf "%s\n" "$*" >>"$TM_LAUNCH_ROOT/calls"; }
sync() { rec sync "$@"; }
poweroff() { rec poweroff "$@"; }
sleep() { rec sleep "$@"; }
. "$1"'

mounted_data='/dev/root / ext4 ro,relatime 0 0
/dev/mmcblk0p3 /data ext4 rw,nosuid,nodev,noexec,noatime 0 0'
no_data='/dev/root / ext4 ro,relatime 0 0'
cmdline='root=/dev/mmcblk0p2 rootfstype=ext4 rootwait ro console=tty3 console=ttyAMA0,115200 quiet'
poweroff_calls='sync
poweroff
sleep 60'

failed=0
r=
shell=

ng() {
  echo "NG - $shell $name: $*"
  failed=$((failed + 1))
}

# 使い方: launch_case 名前 終了コード /proc/cmdline /proc/mounts [/etc/default/trainmaster の中身]
launch_case() {
  name=$1
  r=$work/${shell// /-}/$name
  mkdir -p "$r"/{run,tmp,dev,proc,etc/default,usr/bin}
  echo '123.45 100.00' >"$r/proc/uptime"
  printf '%s\n' "$3" >"$r/proc/cmdline"
  echo "$4" >"$r/proc/mounts"
  : >"$r/dev/console"
  : >"$r/calls"
  if [[ -n ${5:-} ]]; then echo "$5" >"$r/etc/default/trainmaster"; fi
  cat >"$r/usr/bin/trainmaster" <<'EOF'
#!/bin/sh
{
  echo "TM_SAVE_DIR=${TM_SAVE_DIR-<unset>}"
  echo "TM_EXIT_ACTION=${TM_EXIT_ACTION-<unset>}"
} >"$TM_LAUNCH_ROOT/env"
exit "$FAKE_EXIT"
EOF
  chmod +x "$r/usr/bin/trainmaster"
  # shellcheck disable=SC2086
  TM_LAUNCH_ROOT=$r FAKE_EXIT=$2 $shell -c "$fakes" tm-launch "$launch" </dev/null || ng "tm-launch exited with $?"
  [[ $(cat "$r/run/tm-launch.exit" 2>/dev/null) == "$2" ]] || ng "/run/tm-launch.exit is not $2"
  grep -q "tm-launch: trainmaster exited with status $2 " "$r/tmp/trainmaster.log" || ng "no exit status $2 in the log"
}

expect_env() {
  local want
  want=$(printf 'TM_SAVE_DIR=%s\nTM_EXIT_ACTION=%s' "$1" "$2")
  [[ $(cat "$r/env") == "$want" ]] || ng "env: want [$want], got [$(cat "$r/env")]"
}

expect_calls() {
  [[ $(cat "$r/calls") == "$1" ]] || ng "calls: want [$1], got [$(cat "$r/calls")]"
}

expect_console() {
  grep -qF -- "$1" "$r/dev/console" || ng "console has no '$1'"
}

expect_no_console() {
  if grep -qF -- "$1" "$r/dev/console"; then ng "console has '$1'"; fi
}

check() {
  local before=$failed
  "$@"
  if ((failed == before)); then echo "ok - $shell $name"; fi
}

case_poweroff() {
  launch_case exit10-poweroff 10 "$cmdline" "$mounted_data"
  expect_env /data/trainmaster poweroff
  expect_calls "$poweroff_calls"
  expect_console 'tm-launch: power off requested'
}

case_exit0() {
  launch_case exit0 0 "$cmdline" "$mounted_data"
  expect_calls ''
  expect_no_console 'power off requested'
}

case_exit1() {
  launch_case exit1 1 "$cmdline" "$mounted_data"
  expect_calls ''
}

case_exit10_unset_action() {
  launch_case exit10-action-unset 10 "$cmdline" "$mounted_data" 'unset TM_EXIT_ACTION'
  expect_env /data/trainmaster '<unset>'
  expect_calls ''
}

case_exit10_action_exit() {
  launch_case exit10-action-exit 10 "$cmdline" "$mounted_data" 'TM_EXIT_ACTION=exit'
  expect_env /data/trainmaster exit
  expect_calls ''
}

case_action_case_insensitive() {
  launch_case action-mixed-case 10 "$cmdline" "$mounted_data" 'TM_EXIT_ACTION=PowerOff'
  expect_env /data/trainmaster PowerOff
  expect_calls "$poweroff_calls"
}

case_action_spaces() {
  launch_case action-spaces 10 "$cmdline" "$mounted_data" 'TM_EXIT_ACTION=" poweroff "'
  expect_env /data/trainmaster ' poweroff '
  expect_calls "$poweroff_calls"
}

case_action_inner_space() {
  launch_case action-inner-space 10 "$cmdline" "$mounted_data" 'TM_EXIT_ACTION="power off"'
  expect_env /data/trainmaster 'power off'
  expect_calls ''
}

case_action_tab_newline() {
  launch_case action-tab-newline 10 "$cmdline" "$mounted_data" "TM_EXIT_ACTION='"$'\t'"POWEROFF"$'\n'"'"
  expect_calls "$poweroff_calls"
}

case_no_data() {
  launch_case no-data 0 "$cmdline" "$no_data"
  expect_env off poweroff
  expect_console 'tm-launch: /data is not mounted, saving is off'
}

case_exhibit() {
  launch_case exhibit 0 "$cmdline trainmaster.save=off" "$mounted_data"
  expect_env off poweroff
  expect_no_console '/data is not mounted'
}

case_exhibit_cr() {
  launch_case exhibit-crlf 0 "$cmdline trainmaster.save=off"$'\r' "$mounted_data"
  expect_env off poweroff
}

case_exhibit_tab() {
  launch_case exhibit-tab 0 "$cmdline"$'\t'"trainmaster.save=off"$'\t'"quiet" "$mounted_data"
  expect_env off poweroff
}

case_exhibit_token() {
  launch_case exhibit-token 0 "$cmdline trainmaster.save=offx" "$mounted_data"
  expect_env /data/trainmaster poweroff
}

case_defaults_override() {
  launch_case defaults-override 10 "$cmdline" "$no_data" 'TM_SAVE_DIR=/srv/save
TM_EXIT_ACTION=exit'
  expect_env /srv/save exit
  expect_calls ''
  expect_no_console '/data is not mounted'
}

case_exhibit_over_defaults() {
  launch_case exhibit-over-defaults 0 "$cmdline trainmaster.save=off" "$mounted_data" 'TM_SAVE_DIR=/srv/save'
  expect_env off poweroff
}

case_defaults_syntax_error() {
  launch_case defaults-syntax-error 10 "$cmdline" "$mounted_data" 'TM_EXIT_ACTION=exit; if'
  expect_env /data/trainmaster poweroff
  expect_calls "$poweroff_calls"
  expect_console 'syntax error in /etc/default/trainmaster'
}

shells=(sh)
if command -v busybox >/dev/null; then
  shells+=("busybox ash")
else
  echo "busybox not found: testing with sh only"
fi

for shell in "${shells[@]}"; do
  for c in case_poweroff case_exit0 case_exit1 case_exit10_unset_action case_exit10_action_exit \
    case_action_case_insensitive case_action_spaces case_action_inner_space case_action_tab_newline case_no_data case_exhibit case_exhibit_cr case_exhibit_tab \
    case_exhibit_token case_defaults_override case_exhibit_over_defaults case_defaults_syntax_error; do
    check "$c"
  done
done

if ((failed)); then
  echo "test-tm-launch: $failed failure(s)"
  exit 1
fi
echo "test-tm-launch: all passed"
