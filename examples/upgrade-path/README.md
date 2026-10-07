# Upgrade path: candidates installed through the contract's own upgrade function

By default Rehearse swaps a candidate's code straight in under the contract address. With an `upgrade` block in the manifest, it instead does what a real upgrade does:
1. install the candidate's Wasm;
2. call the deployed contract's own upgrade function with the candidate's hash;
3. run any `migrate` calls;
4. check that the contract really is running the candidate's code;
5. run the workflow.

[`manifest.json`](manifest.json) is the demo workflow plus:

```json
"upgrade": { "call": "upgrade", "args": [{ "wasm_hash": "candidate" }] }
```

`{"wasm_hash": "candidate"}` resolves to each candidate's own hash. Migration calls, if your contract has them, go in an optional `"migrate": [ … ]` list of steps inside the `upgrade` block.

## Result

Captured from the deployed demo token on testnet ledger 5,070,694 ([`capture/`](capture/)), with both candidates replayed through the upgrade path during capture ([`report.html`](report.html), [`report.json`](report.json)):

| Candidate | `upgrade(hash)` | Required authorization | Workflow |
|---|---|---|---|
| v2-compatible | ok, candidate code running | admin `GC6C…OA7T` authorizes `upgrade(0xfe2fef82…)` | no observed difference |
| v2-broken | ok, candidate code running | admin `GC6C…OA7T` authorizes `upgrade(0xfffa63ee…)` | 7 of 8 results change, as in the main demo |

The upgrade call and any migration calls are reported separately from the compared workflow steps.

If the upgrade call fails, or the contract is not running the candidate's code afterwards, the candidate gets the status `upgrade failed` and `--fail-on-divergence` exits 2. Otherwise the workflow would silently run on the old code and report "no observed difference". The CLI's integration tests cover this case.

## Limits

Authorization is still mocked, so this checks *who* the upgrade requires (here, the admin), not whether a real admin signature would be accepted.
