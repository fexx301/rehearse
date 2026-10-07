# Authorization check: a signature that stops covering the amount

The candidate `token-v2-authscope` ([source](../../demo/contracts/token-v2-authscope/src/lib.rs)) differs from the deployed token in one line of `transfer`:

```diff
-        from.require_auth();
+        from.require_auth_for_args((to.clone(),).into_val(&e));
```

The sender still has to authorize the transfer, and still names the recipient. But the signature no longer includes the amount, so an authorization for "A pays B" no longer says how much. That is a security regression, and nothing in the usual checks catches it:
- it passes all 8 tests in the shared suite, which checks *who* authorized, not *what*;
- its contract spec is byte-identical to the deployed version;
- every call returns exactly what the deployed contract returns.

## What Rehearse shows

[`manifest.json`](manifest.json) reads two holders, moves 100 RHD from A to B, and reads both again. It was captured from the deployed demo token on testnet ledger 5,070,573 ([`capture/`](capture/)), with the candidate replayed during capture so nothing it reads falls outside the snapshot.

All five results match ([`report.html`](report.html), [`report.json`](report.json)). The report still flags `v2-authscope` as diverged, because of what call 3 required:

| | Required authorization for "A sends 100 RHD to B" |
|---|---|
| deployed | `GA5U…RAL3 authorizes CCFM…XIRI.transfer(GA5U…RAL3, GAA4…PJGI, 1000000000)` |
| v2-authscope | `GA5U…RAL3 authorizes CCFM…XIRI.transfer(GAA4…PJGI)` |

`v2-compatible`, the control, shows no observed difference. `replay --fail-on-divergence` exits 2 because of `v2-authscope`.

## How it works, and its limits

Replay mocks authorization, since it holds nobody's keys. While mocking, the Soroban host still records every authorization a call requires: the address, the function, the exact arguments, and any nested calls. Rehearse compares those records between the deployed code and each candidate, on every call where both succeed.

It does not verify signatures, and it does not run custom account logic (`__check_auth`), which needs real signatures. It shows *what* a candidate asks users to authorize, not whether a particular wallet would approve it.
