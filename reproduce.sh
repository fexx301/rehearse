#!/usr/bin/env bash
# Regenerate every report that has a committed capture (the demo, and the
# authorization, event and upgrade-path examples) and check each matches the
# committed copy byte for byte. Replay makes no network calls.
#
#   ./reproduce.sh            build (fetching crates if needed), replay, compare
#   OFFLINE=1 ./reproduce.sh  build from the local crate cache only
set -euo pipefail
cd "$(dirname "$0")"

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi; }

cargo build --release --locked ${OFFLINE:+--offline} --manifest-path cli/Cargo.toml

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
R=cli/target/release/rehearse
status=0

# check NAME DIR CANDIDATE|--FLAG...: replay DIR's committed capture, render, compare both reports.
check() {
  local name="$1" dir="$2"; shift 2
  local args=()
  for c in "$@"; do
    case "$c" in --*) args+=("$c") ;; *) args+=(--candidate "$c") ;; esac
  done
  mkdir -p "$out/$name"
  "$R" replay --manifest "$dir/manifest.json" --capture "$dir/capture" "${args[@]}" --out "$out/$name/report.json" >/dev/null
  "$R" render --report "$out/$name/report.json" --out "$out/$name/report.html" >/dev/null
  for f in report.json report.html; do
    if cmp -s "$out/$name/$f" "$dir/$f"; then
      echo "OK: $dir/$f is byte-identical to the committed copy ($(sha256 "$dir/$f" | cut -c1-16)…)"
    else
      echo "MISMATCH: regenerated $dir/$f differs from the committed copy" >&2
      diff "$dir/$f" "$out/$name/$f" | head -20 >&2 || true
      status=1
    fi
  done
}

W=demo/wasm
check demo         demo                  v2-compatible=$W/token-v2-compatible.wasm v2-broken=$W/token-v2-broken.wasm
check auth-scope   examples/auth-scope   v2-compatible=$W/token-v2-compatible.wasm v2-authscope=$W/token-v2-authscope.wasm v2-noauth=$W/token-v2-noauth.wasm --check-signatures
check events       examples/events       v2-swapped=$W/token-events-v2-swapped.wasm
check upgrade-path examples/upgrade-path v2-compatible=$W/token-v2-compatible.wasm v2-broken=$W/token-v2-broken.wasm
exit $status
