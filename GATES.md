# Validation log

The checks Rehearse had to pass before release, how each was run, and the results. A check that was not run counts as failed. Numbering follows the original plan; G3 and G8 covered submission logistics and are not part of this repository.

| Gate | Result | Recorded (UTC) |
|---|---|---|
| G1 Prior-art runtime check | **Pass** (outcome: soroban-fork refused the SDK-28 fixture; informational) | 2026-10-06 16:16 |
| G2 Regression on captured state | **Pass on 6 of 7 criteria; ShippedLabs criterion inconclusive** (the tool cannot parse protocol-28 contracts). Fallback not triggered. | 2026-10-06 16:49 |
| G4 Offline replay | **Pass** | 2026-10-06 |
| G5 HTML report | **Pass** | 2026-10-06 |
| G6 Clean-machine build | **Pass** (Linux arm64 container; Linux x86-64 in CI) | 2026-10-06 |
| G7 Honest README | **Pass** | 2026-10-06 |

## G1: soroban-fork against live protocol-29 testnet

**Question.** Can the closest prior art, [soroban-fork](https://github.com/lobotomoe/soroban-fork), execute the Rehearse fixture against current testnet state?

**Method.** Run soroban-fork as it is designed to be used: library-mode `ForkConfig::new(testnet RPC).build()`, then invoke the deployed fixture. This differs from the gate's original wording, which fed it the spike's `snapshot.json`. That file is in `soroban-ledger-snapshot` format, so a refusal there would only have tested the file format. Read-only: forks testnet, never submits a transaction. A built-in native-XLM Stellar Asset Contract call is the control.

**Versions.**
- soroban-fork 0.9.5, the latest release on crates.io (2026-06-03) and repository HEAD `4c42410d`.
- Resolved dependencies: soroban-env-host 25.2.2, soroban-sdk 25.3.2, soroban-ledger-snapshot 25.3.2.
- Rust 1.95.0.
- Fixture `CAZVNRBBQAUZI4KZFYRKQUGI53PPDEHCUGSDWQ5G4HHAQ6CFXSMPGILB`, built with soroban-sdk 28.0.0. All three spike Wasm files declare `contractenvmetav0` interface protocol 28, pre-release 0.

**Result.**

```text
FORK ledger=5055959 reported_protocol=25 host_max=25
RESULT CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC.decimals() -> ok U32(7)
RESULT CAZVNRBBQAUZI4KZFYRKQUGI53PPDEHCUGSDWQ5G4HHAQ6CFXSMPGILB.balance() -> error Err(Ok(Error(Context, InvalidAction)))
[Diagnostic Event] topics:[error, Error(WasmVm, InvalidInput)], data:["contract protocol number is newer than host", 28]
```

**What this shows.**
- soroban-fork does not refuse a protocol-29 network. It silently caps the reported protocol to its host maximum, 25 (`src/lib.rs`, `cap_protocol_version`), and runs under protocol-25 semantics. The control call succeeds.
- It refuses to execute any contract whose Wasm declares a protocol above 25. Tested with soroban-sdk 28, which covers all three Rehearse fixture builds. SDK 26–27 builds should fail the same check by their declared protocol, but only SDK 28 was run.
- `fork_etch` was not exercised. It installs the same Wasm and goes through the same host check, so an SDK-28 candidate fails the same way.

**What it does not show.**
- soroban-fork still runs contracts built with soroban-sdk 25 or older against current state, at protocol-25 semantics.
- One soroban-env-host bump in soroban-fork removes this gap.
- Rehearse's own runtime (env-host 28.0.2 with the experimental `next` feature) is not exact protocol-29 production parity either.

**README line this earns (G7):** "soroban-fork 0.9.5 (soroban-env-host 25) caps forks to protocol 25 and refuses contracts built with soroban-sdk 28 (`contract protocol number is newer than host`, checked 2026-10-06; SDK 26–27 fail the same check by declared protocol). Rehearse runs SDK 28 contracts on env-host 28.0.2 `next` against protocol-29 state."

**Reproduce.** `cargo run --release` in [gates/g1](gates/g1/), with network access to `soroban-testnet.stellar.org`. Ledger numbers will differ. Saved output: [gates/g1/run.log](gates/g1/run.log), SHA-256 `e0896b731c0c8c3993d51c9ff4e167918cd47684b30e62cf9c371ddfe735fd72`.

## G2: regression on captured state

**Setup.**
- Token v1 deployed on testnet as `CCFM7DDZ3DFKKLDJ335J47WDEVZJALLIVNVDJZYXNUB5T75YWZK6XIRI` (Wasm `d42b3a62…`), admin `rehearse-deployer` (`GC6CMA3N…`).
- Four holders seeded: A 1,240 RHD, B 500, C 75.25, D 3,000 (7 decimals).
- Candidates: `token-v2-compatible` (helper refactor, Wasm `fe2fef82…`) and `token-v2-broken` (only change: `DataKey::Balance` renamed to `DataKey::BalanceOf`, Wasm `fffa63ee…`). Source in [demo/contracts](demo/contracts/); built Wasm in [demo/wasm](demo/wasm/).

| Criterion | Result | Evidence |
|---|---|---|
| Three or more seeded holders, each balance entry captured by key | **Pass**: 4 holders. Keys came from live `simulateTransaction` footprints, not hand-listed. Capture then replayed the candidates locally and refetched every key they read. It converged in 2 rounds, with everything read at ledger 5,056,347. | [demo/capture/provenance.json](demo/capture/provenance.json) |
| Baseline reads seeded balances | **Pass**: all 8 manifest expectations met | [demo/report.json](demo/report.json) |
| Compatible build reads seeded balances | **Pass**: no observed difference in any step or final state | same |
| Storage-key candidate reads 0 for all holders | **Pass**: 0 for A, B, C and D. A's transfer fails with `InsufficientBalance (Error(Contract, #2))`. The `BalanceOf(…)` keys it reads are verified absent on-chain at the capture ledger, so the zeros are real post-upgrade behavior, not missing capture. | same; `keys_verified_absent` in provenance |
| Candidate's own fresh-state unit tests pass | **Pass**: 8 of 8 on all three builds, with one shared test file | [gates/g2/cargo-test.txt](gates/g2/cargo-test.txt) |
| `contractspecv0` byte-identical after `stellar contract build` | **Pass**: spec SHA-256 `b55fdb75…` identical for all three builds (stellar-cli 28.1.0) | [gates/g2/spec-check.txt](gates/g2/spec-check.txt) |
| ShippedLabs installed, run, no CRITICAL finding | **Inconclusive**. Details below. | [gates/g2/shippedlabs-v1-vs-v2-broken.txt](gates/g2/shippedlabs-v1-vs-v2-broken.txt) |

**ShippedLabs detail.**
- Repository HEAD (`d0a24bb`, 2026-10-03) does not compile. There are two upstream errors, in `src/logging.rs` and in `src/main.rs`.
- The newest first-parent commit that builds is `4a0c6ad` (2026-10-01). It refuses both candidate builds: `Unsupported interface version: no decoder supports protocol 28; registered decoders: [… 'soroban-v0-protocol-20-23' …]`.
- HEAD registers the same 20–23 decoder.
- So it gave no verdict. That neither satisfies the criterion nor triggers the backup regression. Never say the candidate "passes ShippedLabs".

**Caveats that stay attached to this result.**
- The rename is invisible to a spec diff but not to every static check. The variant names sit as plain strings in the Wasm data section (`…SymbolBalance` vs `…SymbolBalanceOf`).
- Replay mocks all authorization (`auth_mocked: true`). The nonce keys in the capture are artifacts of that mocking.
- The runtime is soroban-env-host 28.0.2 with experimental `next` (not exact production parity). Snapshot state-archival settings are SDK defaults.
- The demo token is a minimal SEP-41-style subset (no allowances or events), written for this demo.

**Determinism.** A second replay produced a byte-identical `report.json` (SHA-256 `5cd0a069…`). The report later gained two fields (`ledger_close_time` and per-candidate `reads_absent_on_chain`), and G4 below pins the current hash.

## G4: offline replay

**Command.** `OFFLINE=1 sandbox-exec -p '(version 1)(allow default)(deny network*)' ./reproduce.sh`

**Result: pass.** Inside a macOS sandbox profile that denies every network operation (checked first: `curl` to the testnet RPC fails to connect under the same profile), the script:
- built the CLI with `cargo build --release --locked --offline`,
- replayed the committed capture against both candidates,
- produced `report.json` and `report.html`, both byte-identical to the committed copies: SHA-256 `e1882aff…` (JSON) and `944d4c09…` (HTML, current renderer).

Re-run after the manifest gained display units and the HTML step was added.

## G5: HTML report

**Command.** `rehearse render --report demo/report.json --out demo/report.html` (also run by `reproduce.sh`).

**Result: pass.** One self-contained file, [demo/report.html](demo/report.html), about 16 KB.
- No `<script>`, no `<link>`, no `@import`, no `http(s)://` reference anywhere in the file. It opens offline and uses system font stacks.
- Shows a headline verdict, a verdict per code version, and a step-by-step table with divergent cells shaded. Each shaded cell also shows the deployed value.
- Explains the cause: storage after the workflow, plus the keys only the candidate reads, marked absent on-chain.
- Coverage figures: entries captured, keys verified absent, reads outside capture, network calls during replay.
- Provenance footer: ledger, snapshot SHA-256, all three Wasm hashes, runtime, authorization mode.
- Amounts render in RHD from the manifest's `display` hint.

**Checked.** Rendered in headless Chrome in light and dark at 1280 px. At 320, 375 and 414 px (via fixed-width iframes, since headless windows clamp to 500 px) the page has no horizontal overflow. On narrow screens the step column is pinned and diverged candidates come first, so the evidence column sits next to the baseline. Design per the Hallmark skill: modern-minimal, Cobalt palette, Workbench structure.


## G6: clean-machine build

**Command.**
```
docker build --no-cache -t rehearse:g6 .
docker run --rm --network none rehearse:g6
```

**Result: pass.**
- From an empty build cache, the image (`rust:1.95-slim-bookworm`, Docker 29.5.3, linux/arm64) fetched crates, built the CLI with `--locked`, and stopped at the replay step.
- Run with `--network none`, it regenerated `report.json` and `report.html` byte-identical to the committed copies (`e1882aff…` and `90773bd1…`) and exited 0.
- The committed files were produced on macOS arm64, so the output is identical across those two operating systems.
- Image `sha256:890e903e…`, about 590 MB.

**x86-64, 2026-10-07.** The GitHub Actions `ci` workflow (ubuntu-latest, x86-64, Rust 1.95.0) runs `./reproduce.sh` on every push. Run [37595319651](https://github.com/fexx301/rehearse/actions/runs/37595319651) passed: both reports byte-identical. So the committed output reproduces on macOS arm64, Linux arm64 and Linux x86-64.

## G7: honest README

**Result: pass**, against each criterion below. See [README.md](README.md).
- **Supported runtime stated:** soroban-sdk 28.0.0, soroban-env-host 28.0.2 `next`, protocol-29 state. It says this is not exact protocol-29 parity and that the snapshot uses SDK-default state-archival settings.
- **Claim boundary:** "the observed differences for the calls you listed, on the state you captured", plus "not a safety certification". A grep for certification, safety, guarantee and audit wording finds only that disclaimer and ShippedLabs' product name.
- **G1 result included**, with version numbers and the exact error text. The SDK 26–27 point is marked as following from the declared protocol, not as tested.
- **soroban-fork and ShippedLabs credited.** ShippedLabs' protocol 20–23 limit is stated with a date. Both gaps are described as closable by an upgrade, not presented as a moat.
- **Limits listed:** mocked authorization, code replacement instead of the upgrade entrypoint, only the listed calls, a single ledger, cross-contract calls not exercised by the demo, and the data-section caveat on the rename.


## Beyond return values: authorization, events, upgrade path (2026-10-07)

Each was added behind the same gates: the demo reports stay byte-identical, CI is green, and every committed example reproduces byte for byte (`reproduce.sh` now checks the demo plus the three examples below, locally and in Docker).

| Check | What replay records and compares | Example and result | Tests |
|---|---|---|---|
| Authorization | Every authorization each call requires, recorded while signatures are mocked: address, function, exact arguments, nested calls. Compared on calls where both versions succeed. | [`examples/auth-scope`](examples/auth-scope). `token-v2-authscope` narrows `transfer`'s signature to `require_auth_for_args((to,))`. All 5 results match, but the authorization differs at call 3, so it is flagged and exits 2. | `authorizations_are_recorded_per_call_without_carry_over`, `a_narrower_signature_scope_is_visible_even_when_results_match` |
| Events | Contract events each call emits (contract, topics, data), sliced per call. Compared on calls where both versions succeed. | [`examples/events`](examples/events). `token-events-v2-swapped` publishes its transfer event with from/to swapped. All 5 results match, but the event differs at call 3, so it is flagged and exits 2. | `events_are_recorded_per_call_and_a_swapped_event_shows` |
| Upgrade path (opt-in) | With an `upgrade` block: install the candidate's Wasm, call the contract's own upgrade function with `{"wasm_hash": "candidate"}`, run `migrate` calls, check the contract runs the candidate's code, then run the workflow. A failed upgrade or migration, or code not installed, gives the status `upgrade failed`. | [`examples/upgrade-path`](examples/upgrade-path). Both candidates go in through `upgrade(hash)`, authorized by the admin. Results match the code-swap demo. | `candidates_go_in_through_the_contracts_own_upgrade_function`, `migrate_steps_run_after_the_upgrade_and_are_recorded`, `a_failed_upgrade_is_reported_and_the_old_code_keeps_running`, plus command-line tests `a_failed_upgrade_fails_the_check`, `a_failed_migration_fails_the_check`, `a_clean_upgrade_passes_the_check` |

| Signed authorizations (`--check-signatures`) | The deployed contract's own authorization requests, signed with test keys written into a copy of the state as the accounts' signers. Every version replays with signature checking on. The check is valid only if the deployed contract accepts its own signed requests (otherwise it is reported as unavailable). | [`examples/auth-scope`](examples/auth-scope). The deployed contract accepts its own signed requests. `v2-compatible` accepts them, `v2-authscope` **rejects** them, and `v2-noauth` (`require_auth` deleted) accepts them because nothing asks, but the requirement comparison flags it. | `signatures_for_the_deployed_contract_work_on_compatible_and_fail_on_authscope`, `the_check_is_unavailable_when_the_deployed_contract_rejects_its_own_requests`, `the_signed_check_is_deterministic`, plus three command-line tests of the exit codes |

**Still not checked.** Real users' keys, and smart-wallet accounts (`__check_auth`): each wallet verifies signatures its own way, so calls that need one are listed as unchecked. The signed check shows whether signatures of the deployed contract's requests still verify on a candidate, not that a candidate checks authorization; the requirement comparison covers that.

**Also in this batch.** The `update_current_contract_wasm` deprecation warning is silenced with `#[allow(deprecated)]`; all Wasm hashes are unchanged.

## Release and GitHub Action (2026-10-07)

[v0.1.0](https://github.com/fexx301/rehearse/releases/tag/v0.1.0) ships prebuilt binaries for macOS (arm64, x86-64) and Linux (x86-64, arm64), each with a SHA-256 file. They are built by [`release.yml`](.github/workflows/release.yml).
- **The action:** the same workflow then ran [`action.yml`](action.yml) on Linux and macOS against the published binaries. It asserted that the action installed the release (not a source build), that the compatible upgrade exits 0, and that the broken one exits 2 with the HTML report written.
- **The README one-liner:** installing from the release on an Apple-silicon Mac, the archive matched its SHA-256 file, and the binary reproduced the demo `report.json` byte for byte (`e1882aff…`).
- **The Intel macOS build:** run under Rosetta, it produced the same `report.json` and `report.html` bytes.
