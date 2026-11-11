#!/usr/bin/env bash
# Buildroot のビルドの時間・ディスク・キャッシュ・配布物の大きさを出す(03 BV10 の記録)。失敗はさせない。
# GITHUB_STEP_SUMMARY があれば、同じ内容をジョブの要約にも書く
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${BR_OUTPUT:-$root/build/rpi3-64}
dl=${BR2_DL_DIR:-$root/build/dl}
dist=${DIST_DIR:-$root/build/dist}
ccache_dir=$HOME/.buildroot-ccache

report() {
  echo "## Buildroot image build (BV10)"
  echo
  echo '```'
  echo "# disk"
  df -h / "$root" /mnt 2>/dev/null | awk '!seen[$0]++'
  echo
  echo "# sizes"
  for d in "$out" "$out/build" "$out/host" "$out/images" "$out/legal-info" "$dl" "$ccache_dir" "$dist"; do
    [[ -d $d ]] && du -sh "$d/" 2>/dev/null
  done
  if [[ -d $dist ]]; then
    echo
    echo "# dist"
    ls -l "$dist"
    for f in SHA256SUMS BUILDINFO.txt; do
      [[ -f $dist/$f ]] || continue
      echo
      echo "# $f"
      cat "$dist/$f"
    done
  fi
  local log=$out/build/build-time.log
  if [[ -f $log ]]; then
    echo
    echo "# build time (from $log)"
    awk -F: '
      { t = $1 + 0; if (!first || t < first) first = t; if (t > last) last = t }
      END { printf "total %.0f min (first to last step; includes earlier builds if the output was reused)\n", (last - first) / 60 }
    ' "$log"
    awk -F: '
      { gsub(/^ +| +$/, "", $4) }
      $2 ~ /^start/ { s[$4] = $1 }
      $2 ~ /^end/ { sum[$4] += $1 - s[$4] }
      END { for (p in sum) printf "%8.1f min  %s\n", sum[p] / 60, p }
    ' "$log" | sort -rn | head -15
  fi
  if [[ -x $out/host/bin/ccache ]]; then
    echo
    echo "# ccache"
    "$out/host/bin/ccache" -s 2>&1
  fi
  echo '```'
}

report | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
exit 0
