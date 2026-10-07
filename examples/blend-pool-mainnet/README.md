# Real-world check on mainnet: Blend's FixedV2 lending pool

Rehearse run read-only against a live production protocol on Stellar mainnet: Blend's `FixedV2` lending pool (`CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD`, from Blend's published [mainnet.contracts.json](https://github.com/blend-capital/blend-utils/blob/main/mainnet.contracts.json)). Captured on 2026-10-07 at mainnet ledger 64,817,493, protocol 29, through the public RPC `https://soroban-rpc.mainnet.stellar.gateway.fm`.

Capture is read-only. `--source` was Circle's USDC issuing account (`GA5ZSEJY…KZVN`), used only as the simulation source because simulation needs an existing account. Nothing was signed or submitted.

## What was compared

| Version | Source | Wasm SHA-256 |
|---|---|---|
| deployed | The code the mainnet pool runs. It is byte-identical to Blend's published release `pool_v2.0.0.wasm` (release `v2.0.0_pool_cli22.0.1`), the same as the testnet pool in [`../blend-pool`](../blend-pool). | `a41fc53d…` |
| v2.0.0-rebuild | The `v2.0.0` tag, built from source | `5d2baf1c…` |
| pre-fix-96ecace | Commit `96ecace`, the last commit before `ac8cda17` ("fix: check reserve status on flash loan") | `23dfd7eb…` |

The builds are the same as in the testnet example. See its README for the build steps.

## Result

[`manifest.json`](manifest.json) reads the pool config, the admin, the reserve list, and the USDC and XLM reserves. All five results are identical across all three versions ([`report.html`](report.html), [`report.json`](report.json)):
- Capture fetched 7 ledger entries in one round.
- Nothing any version read fell outside the capture.
- `replay --fail-on-divergence` exits 0.

The same caveat as on testnet applies. The flash-loan fix is not exercised by read-only calls, so "no observed difference" is the expected result for this workflow, not a statement about the fix.

## Why the snapshot and Wasm are not committed

Blend's contracts are AGPL-3.0. The capture holds Blend's compiled code, and the candidates are builds of it, so only the manifest and the report outputs are kept here. To recreate everything, build the candidates as in [`../blend-pool`](../blend-pool), then:

```sh
C="--candidate v2.0.0-rebuild=pool-v2.0.0.wasm --candidate pre-fix-96ecace=pool-96ecace.wasm"
rehearse capture --manifest examples/blend-pool-mainnet/manifest.json --source <any existing mainnet G… account> $C --out mainnet-capture
rehearse replay  --manifest examples/blend-pool-mainnet/manifest.json --capture mainnet-capture $C --out report.json
rehearse render  --report report.json --out report.html
```

A fresh capture reads a later ledger, so the reserve figures will differ. The comparison should not.
