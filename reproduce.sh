#!/usr/bin/env bash
# Regenerate demo/report.json and demo/report.html from the committed capture and
# Wasm, and check both match the committed copies byte for byte. Replay makes no
# network calls.
#
#   ./reproduce.sh            build (fetching crates if needed), replay, compare
#   OFFLINE=1 ./reproduce.sh  build from the local crate cache only
set -euo pipefail
cd "$(dirname "$0")"

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi; }

cargo build --release --locked ${OFFLINE:+--offline} --manifest-path cli/Cargo.toml

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
cli/target/release/rehearse replay \
  --manifest demo/manifest.json \
  --capture demo/capture \
  --candidate v2-compatible=demo/wasm/token-v2-compatible.wasm \
  --candidate v2-broken=demo/wasm/token-v2-broken.wasm \
  --out "$out/report.json"

cli/target/release/rehearse render --report "$out/report.json" --out "$out/report.html"

status=0
for f in report.json report.html; do
  if cmp -s "$out/$f" "demo/$f"; then
    echo "OK: $f is byte-identical to the committed copy ($(sha256 "demo/$f" | cut -c1-16)…)"
  else
    echo "MISMATCH: regenerated $f differs from demo/$f" >&2
    diff "demo/$f" "$out/$f" | head -20 >&2 || true
    status=1
  fi
done
exit $status
