#!/usr/bin/env bash
# 出荷バイナリ(tm-app の既定の feature、aarch64)に入る依存クレートのライセンス文を cargo-about で 1 つのテキストにする(03 §6.4)
# 依存クレートのソースは取得済みであること(CARGO_HOME の中を見るだけで、ネットワークは使わない)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=$(realpath -m -- "${1:?usage: $0 <output file>}")

cd "$root"
cargo about generate --frozen --fail \
  -c scripts/about.toml -m crates/tm-app/Cargo.toml \
  -o "$out" scripts/about.hbs
n=$(grep -c '^Used by:' "$out")
echo "crate licenses: $out ($n license texts)"
# about.toml の clarify が外れると、sdl2-sys の SDL のファイルが全部拾われて数百件になる
[[ $n -le 40 ]] || { echo "NG: too many license texts ($n); check the clarify entries in scripts/about.toml" >&2; exit 1; }
