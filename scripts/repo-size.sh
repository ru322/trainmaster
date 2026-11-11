#!/usr/bin/env bash
# Informational only: never fails the job.
set -u

cd "$(dirname "$0")/.." || exit 1

generated=crates/tm-assets/generated
audio=$generated/audio

report() {
  echo "## Repository size"
  echo
  echo '```'
  git count-objects -vH
  for dir in "$generated" "$audio"; do
    if [[ -d $dir ]]; then du -sh "$dir"; else echo "$dir: not present"; fi
  done
  if [[ -d $audio ]]; then
    wav_total=$(find "$audio" -maxdepth 1 -name '*.wav' -printf '%s\n' | awk '{ s += $1 } END { print s + 0 }')
    manifest_total=$(sed -n 's/^total_bytes \([0-9][0-9]*\) .*/\1/p' "$audio/audio_manifest.txt" 2>/dev/null)
    echo "wav total bytes: ${wav_total}"
    echo "audio_manifest.txt total_bytes: ${manifest_total:-missing}"
    if [[ ${manifest_total:-} == "$wav_total" ]]; then echo "wav total matches manifest"; else echo "WARNING: wav total differs from manifest"; fi
  fi
  echo '```'
}

report | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
exit 0
