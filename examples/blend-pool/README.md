# Real-world check: Blend's testnet lending pool

Rehearse run read-only against a production protocol we did not write: Blend's testnet pool `TestnetV2` (`CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF`). Run on 2026-10-07 at testnet ledger 5,067,956, protocol 29.

## What was compared

| Version | Source | Wasm SHA-256 |
|---|---|---|
| deployed | The code the pool runs. It is byte-identical to Blend's published release `pool_v2.0.0.wasm` (release `v2.0.0_pool_cli22.0.1`). Built with soroban-sdk 22.0.7. | `a41fc53d…` |
| v2.0.0-rebuild | The `v2.0.0` tag, built from source | `5d2baf1c…` |
| pre-fix-96ecace | Commit `96ecace`, the last commit before `ac8cda17` ("fix: check reserve status on flash loan") | `23dfd7eb…` |

Both rebuilds followed Blend's Makefile (`cargo rustc --crate-type=cdylib` for pool-factory, backstop, then pool) with their pinned Rust 1.81, without the final `stellar contract optimize` step. That is why their hashes differ from the release.

## Result

[`manifest.json`](manifest.json) reads the pool config, the admin, the reserve list, and the USDC and XLM reserves. All five results are identical across all three versions ([`report.html`](report.html), [`report.json`](report.json)):
- Capture fetched 7 ledger entries in one round.
- Nothing any version read fell outside the capture.

**What this shows.**
- Rehearse runs on a contract built with an older SDK (22) against live protocol-29 state.
- A production code base's own history can serve as candidates.

**What it does not show.** The pre-fix commit differs only in the flash-loan path. A read-only workflow never exercises that path, so "no observed difference" is the expected and correct result: Rehearse compares only the calls listed. To test the fix itself, the manifest would need a flash loan against a disabled reserve.

## Fidelity check against the live network

Taken at the first capture, ledger 5,067,876. The live `get_reserve(USDC)` result, from `stellar contract invoke --send=no`, was compared with the offline baseline replay:
- Every stored field matched: reserve config, decimals, caps, supplies.
- The interest-rate fields (`b_rate`, `d_rate`, `ir_mod`, `backstop_credit`, `last_time`) differed. Blend accrues interest to the current time on every read. The offline replay accrued to the capture ledger's close time exactly (`last_time` = 1791362967 = the ledger close time). The live call ran 90 seconds later.

## Why the snapshot and Wasm are not committed

Blend's contracts are licensed AGPL-3.0, and this repository is Apache-2.0. The capture contains Blend's compiled code (in `snapshot.json` and `deployed.wasm`), and the candidates are builds of it. Only the manifest and the report outputs are kept here. To recreate everything:

```sh
# 1. Build the two candidates from Blend's repo (Rust 1.81 is pinned by their rust-toolchain.toml)
git clone https://github.com/blend-capital/blend-contracts-v2 && cd blend-contracts-v2
for rev in v2.0.0 96ecace; do
  git checkout -q $rev
  for c in pool-factory backstop pool; do
    cargo rustc --manifest-path=$c/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
  done
  cp target/wasm32-unknown-unknown/release/pool.wasm ../pool-$rev.wasm
done
cd ..

# 2. Capture (read-only; any funded testnet account as --source), then replay
C="--candidate v2.0.0-rebuild=pool-v2.0.0.wasm --candidate pre-fix-96ecace=pool-96ecace.wasm"
rehearse capture --manifest examples/blend-pool/manifest.json --source G… $C --out blend-capture
rehearse replay  --manifest examples/blend-pool/manifest.json --capture blend-capture $C --out report.json
rehearse render  --report report.json --out report.html
```

A fresh capture reads a later ledger, so the numbers will differ. The comparison should not.
