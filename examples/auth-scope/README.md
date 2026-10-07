# Authorization checks: what a candidate requires, and whether signatures still work

Three candidates, all with the same contract spec as the deployed token and all returning exactly the same results for this workflow:

| Candidate | Change to `transfer` | Its tests |
|---|---|---|
| `token-v2-compatible` | none (refactor) | passes the shared suite |
| `token-v2-authscope` ([source](../../demo/contracts/token-v2-authscope/src/lib.rs)) | `from.require_auth()` → `from.require_auth_for_args((to,))`: the signature no longer covers the amount | passes the shared suite, which checks *who* authorized, not *what* |
| `token-v2-noauth` ([source](../../demo/contracts/token-v2-noauth/src/lib.rs)) | `from.require_auth()` deleted: anyone can move anyone's balance | deliberately insecure; its own test demonstrates the hole (the shared suite would fail) |

## What Rehearse shows

[`manifest.json`](manifest.json) reads two holders, moves 100 RHD from A to B, and reads both again. It was captured from the deployed demo token on testnet ledger 5,071,086, with all three candidates replayed during capture. The report ([`report.html`](report.html), [`report.json`](report.json)) is produced with `replay --check-signatures`, and it runs two different checks.

**1. Who must authorize.** Rehearse records every authorization each call requires (address, function, exact arguments) and compares it with the deployed contract:

| | "A sends 100 RHD to B" requires |
|---|---|
| deployed | `GA5U… authorizes transfer(GA5U…, GAA4…, 1000000000)` |
| v2-authscope | `GA5U… authorizes transfer(GAA4…)`, flagged |
| v2-noauth | nothing, flagged |

**2. Signatures made for the deployed contract.** In a copy of the captured state, account A is given a test signer. The deployed contract's own request is signed with it, the way a wallet signs what simulation returns, and every version replays with signature checking on:
- **Sanity check:** the deployed contract accepts its own signed request, so the check is valid for this workflow.
- **v2-compatible:** accepts it.
- **v2-authscope:** **rejects** it. A wallet's signature for today's contract would fail after this upgrade.
- **v2-noauth:** accepts it. Nothing asks for the signature any more, and the host ignores an unused one.

The second check never catches a missing authorization; the first always does. Use both.

`replay --fail-on-divergence` exits 2 for `v2-authscope` and `v2-noauth`, and 0 for `v2-compatible`. The CLI's command-line tests check all three.

## Limits

- **Test keys stand in for real signers.** The signed check uses keys Rehearse generates, written into a copy of the state as the accounts' signers. It shows whether signatures *of the deployed contract's requests* still verify, not that any real wallet would sign.
- **Ordinary accounts only.** Accounts that don't exist on-chain (like the demo holders) are created in the copy. A contract that reads native balances or account existence could behave differently there.
- **Smart wallets are not covered.** Calls where a contract (smart-wallet) account must authorize are not checked, and the report lists them. Each wallet design verifies signatures its own way.
