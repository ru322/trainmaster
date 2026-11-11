#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

status=0
ng() {
  echo "NG: $*" >&2
  status=1
}

mise_rust_version() {
  awk '/^\[/ { in_tools = ($0 == "[tools]") } in_tools && /^rust[[:space:]]*=/ { print; exit }' mise.toml |
    sed -E 's/^rust[[:space:]]*=[^"]*"([^"]+)".*/\1/'
}

toml_value() {
  awk -v section="$1" -v key="$2" '
    /^\[/ { in_section = ($0 == "[" section "]") }
    in_section && $1 == key && $2 == "=" { gsub(/"/, "", $3); print $3; exit }
  ' Cargo.toml
}

mise_version=$(mise_rust_version)
if [[ ! $mise_version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "NG: cannot read tools.rust version from mise.toml (got '${mise_version}')" >&2
  exit 1
fi

check_decl() {
  local require_buildroot=$1
  local expected_msrv=${mise_version%.*}

  echo "mise.toml tools.rust: ${mise_version}"

  local msrv resolver
  msrv=$(toml_value workspace.package rust-version)
  resolver=$(toml_value workspace resolver)
  echo "Cargo.toml workspace.package.rust-version: ${msrv}"
  echo "Cargo.toml workspace.resolver: ${resolver}"
  [[ $msrv == "$expected_msrv" ]] || ng "workspace.package.rust-version '${msrv}' != '${expected_msrv}' (major.minor of mise.toml)"
  [[ $resolver == 3 ]] || ng "workspace.resolver '${resolver}' != '3'"

  local rust_bin_mk="${BR_SRC:-}/package/rust-bin/rust-bin.mk"
  if [[ -n ${BR_SRC:-} && -f $rust_bin_mk ]]; then
    local br_version
    br_version=$(sed -n -E 's/^RUST_BIN_VERSION[[:space:]]*=[[:space:]]*//p' "$rust_bin_mk")
    echo "Buildroot RUST_BIN_VERSION: ${br_version}"
    [[ $br_version == "$mise_version" ]] || ng "Buildroot RUST_BIN_VERSION '${br_version}' != '${mise_version}'"
  elif [[ $require_buildroot == 1 ]]; then
    ng "Buildroot rust-bin.mk not found (BR_SRC='${BR_SRC:-}'); run br:fetch first"
  else
    echo "Buildroot RUST_BIN_VERSION: skipped (not fetched)"
  fi

  command -v jq >/dev/null || { echo "NG: jq is required" >&2; exit 1; }
  local packages bad
  packages=$(cargo metadata --format-version 1 --no-deps --locked)
  bad=$(jq -r --arg v "$msrv" '.packages[] | select(.rust_version != $v) | "\(.name) rust_version=\(.rust_version)"' <<<"$packages")
  echo "cargo metadata packages: $(jq '.packages | length' <<<"$packages")"
  [[ -z $bad ]] || ng "packages whose rust_version is not '${msrv}':"$'\n'"${bad}"
}

check_active() {
  local info release
  info=$(rustc -vV)
  echo "$info"
  release=$(sed -n 's/^release: //p' <<<"$info")
  [[ $release == "$mise_version" ]] || ng "active rustc release '${release}' != '${mise_version}' (mise.toml)"
}

# C-3: 出荷バイナリをビルドした Buildroot の rustc と mise の rustc の release / commit-hash(03 §2.2)
check_buildroot() {
  local out=${BR_OUTPUT:-build/rpi3-64}
  local br_rustc=$out/host/bin/rustc
  if [[ ! -x $br_rustc ]]; then
    ng "$br_rustc not found (run mise run br:build first)"
    return
  fi
  local br_info mise_info key a b
  br_info=$("$br_rustc" -vV)
  mise_info=$(rustc -vV)
  echo "Buildroot rustc ($br_rustc):"
  echo "$br_info"
  echo "mise rustc:"
  echo "$mise_info"
  for key in release commit-hash; do
    a=$(sed -n "s/^$key: //p" <<<"$br_info")
    b=$(sed -n "s/^$key: //p" <<<"$mise_info")
    [[ -n $a && $a == "$b" ]] || ng "$key: Buildroot '${a}' != mise '${b}'"
  done
  a=$(sed -n 's/^release: //p' <<<"$br_info")
  [[ $a == "$mise_version" ]] || ng "Buildroot rustc release '${a}' != '${mise_version}' (mise.toml)"

  local bin=$out/target/usr/bin/trainmaster line
  line=$("$br_rustc" --version)
  if [[ ! -f $bin ]]; then
    ng "$bin not found"
  elif ! grep -aqF -- "$line" "$bin"; then
    ng "$bin does not embed '${line}' (TM_RUSTC_VERSION); it was not built by this rustc"
  else
    echo "$bin embeds '${line}'"
  fi
}

usage() {
  echo "usage: $0 decl [--require-buildroot] | active | buildroot" >&2
  exit 2
}

case "${1:-}" in
  decl)
    [[ $# -le 2 ]] || usage
    case "${2:-}" in
      "") check_decl 0 ;;
      --require-buildroot) check_decl 1 ;;
      *) usage ;;
    esac
    ;;
  active)
    [[ $# -eq 1 ]] || usage
    check_active
    ;;
  buildroot)
    [[ $# -eq 1 ]] || usage
    check_buildroot
    ;;
  *) usage ;;
esac

exit "$status"
