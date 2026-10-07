# Event check: a transfer event that names the wrong direction

The demo token emits no events, so this example uses a variant that does: the events token ([source](../../demo/contracts/token-events-v1/src/lib.rs)), deployed on testnet as `CDPWRTCRXSBC56KLVRHGALWH24NVG67Q2VHACXEP2OLLX4OBM5J2FDED`. Every `transfer` publishes a SEP-41-style `Transfer` event.

The candidate `token-events-v2-swapped` ([source](../../demo/contracts/token-events-v2-swapped/src/lib.rs)) changes one line:

```diff
-        Transfer { from: from.clone(), to: to.clone(), amount }.publish(&e);
+        Transfer { from: to.clone(), to: from.clone(), amount }.publish(&e);
```

Balances still move correctly, so every call returns what the deployed contract returns. It passes all 8 tests in the shared suite, and its contract spec is byte-identical. But a wallet history or indexer reading the event records the transfer going the wrong way.

## What Rehearse shows

[`manifest.json`](manifest.json) reads two holders (800 and 250 RHE), moves 100 RHE from A to B, and reads both again. It was captured on testnet ledger 5,070,643 ([`capture/`](capture/)), with the candidate replayed during capture. All five results match ([`report.html`](report.html), [`report.json`](report.json)), and the report flags call 3:

| | Event emitted by "A sends 100 RHE to B" |
|---|---|
| deployed | `transfer, GA5U…RAL3, GAA4…PJGI` with `{amount: 1000000000}` |
| v2-swapped | `transfer, GAA4…PJGI, GA5U…RAL3` with `{amount: 1000000000}` |

`replay --fail-on-divergence` exits 2.

Events are compared on every call where both versions succeed: contract address, topics and data, in order. A failed call is already reported as a result difference.
