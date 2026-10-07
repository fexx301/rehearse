//! Offline execution of a manifest against captured ledger state, once per code version.
//!
//! Every branch starts from the same snapshot bytes. A candidate branch differs
//! only in the contract instance's executable: the candidate Wasm is installed
//! and the instance repointed at it, so the contract address and all stored
//! state stay exactly as captured. No network access happens here.
use crate::{fmt, manifest::Manifest};
use sha2::{Digest, Sha256};
use soroban_env_host::{storage::EntryWithLiveUntil, HostError};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{
    AuthorizedFunction, AuthorizedInvocation, Events as _, SnapshotSource, SnapshotSourceInput,
};
use soroban_sdk::xdr::{
    ContractCodeEntry, ContractCodeEntryExt, ContractExecutable, Hash, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerKey, LedgerKeyContractCode, Limited, Limits, ReadXdr, ScAddress,
    ScErrorType, ScSpecEntry, ScVal, WriteXdr,
};
use soroban_sdk::{Address, Env, InvokeError, Symbol, TryFromVal, Val};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::Cursor,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
    str::FromStr,
};

/// Wraps the captured snapshot and records every key the host asked for but
/// the snapshot did not hold. Those are the reads the capture cannot vouch for.
struct RecordingSource {
    inner: Rc<LedgerSnapshot>,
    misses: RefCell<BTreeMap<String, LedgerKey>>,
}

impl SnapshotSource for RecordingSource {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
        let found = SnapshotSource::get(self.inner.as_ref(), key)?;
        if found.is_none() {
            let k: &LedgerKey = key;
            self.misses.borrow_mut().insert(key_id(k), k.clone());
        }
        Ok(found)
    }
}

pub struct StepResult {
    pub label: String,
    pub call: String,
    pub args: Vec<String>,
    pub result: String,
    pub failed: bool,
    /// Who had to authorize what during this call (recorded while auth is mocked), sorted.
    pub auths: Vec<String>,
    /// Contract events this call emitted, in order.
    pub events: Vec<String>,
    /// The exact authorization payloads this call required (mocked runs only).
    pub payloads: Vec<crate::signing::Payload>,
    /// Signed runs only: false when the call fell back to mocking (a contract account had to
    /// authorize), so its signatures were not checked.
    pub signature_checked: Option<bool>,
}

/// How authorization is handled in a run.
pub enum Auth<'a> {
    /// Every require_auth is satisfied; what each call requires is recorded.
    Mocked,
    /// Checking on: each workflow call gets the deployed contract's own requests for that
    /// call, signed with test keys (see `signing`).
    Signed(&'a [Vec<crate::signing::Payload>]),
}

fn val_str(env: &Env, v: &Val) -> String {
    ScVal::try_from_val(env, v).map(|s| fmt::scval(&s)).unwrap_or_else(|_| "<unconvertible>".into())
}

fn invocation_str(env: &Env, inv: &AuthorizedInvocation) -> String {
    let f = match &inv.function {
        AuthorizedFunction::Contract((addr, func, args)) => format!(
            "{}.{}({})",
            val_str(env, &addr.to_val()),
            val_str(env, &func.to_val()),
            args.iter().map(|a| val_str(env, &a)).collect::<Vec<_>>().join(", ")
        ),
        other => format!("{other:?}"),
    };
    if inv.sub_invocations.is_empty() {
        f
    } else {
        let subs: Vec<String> = inv.sub_invocations.iter().map(|i| invocation_str(env, i)).collect();
        format!("{f} > [{}]", subs.join("; "))
    }
}

fn event_str(e: &soroban_sdk::xdr::ContractEvent) -> String {
    let contract = e.contract_id.as_ref().map(|c| ScAddress::Contract(c.clone()).to_string()).unwrap_or_default();
    match &e.body {
        soroban_sdk::xdr::ContractEventBody::V0(b) => format!(
            "{contract}: [{}] {}",
            b.topics.iter().map(fmt::scval).collect::<Vec<_>>().join(", "),
            fmt::scval(&b.data)
        ),
    }
}

pub struct Branch {
    pub label: String,
    pub wasm_sha256: String,
    pub steps: Vec<StepResult>,
    /// Contract storage after the workflow: readable key -> readable value.
    pub final_state: BTreeMap<String, String>,
    /// Keys read during the workflow that the snapshot did not contain.
    pub misses: BTreeMap<String, LedgerKey>,
    /// Upgrade path only: the upgrade call, the migrate calls, and whether the contract ended up
    /// running the candidate's code.
    pub upgrade: Option<StepResult>,
    pub migrate: Vec<StepResult>,
    pub installed: Option<bool>,
}

pub fn key_id(k: &LedgerKey) -> String {
    k.to_xdr_base64(Limits::none()).expect("ledger key encodes")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    fmt::hex(&Sha256::digest(bytes))
}

fn instance_entry_mut<'a>(
    snap: &'a mut LedgerSnapshot,
    contract: &ScAddress,
) -> Option<&'a mut soroban_sdk::xdr::ScContractInstance> {
    snap.ledger_entries.iter_mut().find_map(|(_, (entry, _))| match &mut entry.data {
        LedgerEntryData::ContractData(d)
            if &d.contract == contract && d.key == ScVal::LedgerKeyContractInstance =>
        {
            match &mut d.val {
                ScVal::ContractInstance(i) => Some(i),
                _ => None,
            }
        }
        _ => None,
    })
}

/// The Wasm currently installed for `contract` in the snapshot.
pub fn deployed_wasm(snap: &LedgerSnapshot, contract: &ScAddress) -> Result<Vec<u8>, String> {
    let mut probe = snap.clone();
    let hash = match instance_entry_mut(&mut probe, contract).map(|i| i.executable.clone()) {
        Some(ContractExecutable::Wasm(h)) => h,
        Some(_) => return Err("contract is not Wasm-backed (built-in contracts are not supported)".into()),
        None => return Err(format!("snapshot has no instance entry for {contract}")),
    };
    snap.ledger_entries
        .iter()
        .find_map(|(_, (entry, _))| match &entry.data {
            LedgerEntryData::ContractCode(c) if c.hash == hash => Some(c.code.to_vec()),
            _ => None,
        })
        .ok_or_else(|| format!("snapshot has no code entry for {}", fmt::hex(&hash.0)))
}

/// A copy of `snap` in which `contract` executes `wasm` instead of its deployed code.
/// Storage, address and TTLs are untouched.
pub fn with_candidate(
    snap: &LedgerSnapshot,
    contract: &ScAddress,
    wasm: &[u8],
) -> Result<LedgerSnapshot, String> {
    let mut out = snap.clone();
    let hash = Hash(Sha256::digest(wasm).into());
    let instance_ttl = out
        .ledger_entries
        .iter()
        .find_map(|(_, (entry, ttl))| match &entry.data {
            LedgerEntryData::ContractData(d)
                if &d.contract == contract && d.key == ScVal::LedgerKeyContractInstance =>
            {
                Some(*ttl)
            }
            _ => None,
        })
        .ok_or_else(|| format!("snapshot has no instance entry for {contract}"))?;
    let instance = instance_entry_mut(&mut out, contract).expect("instance located above");
    instance.executable = ContractExecutable::Wasm(hash.clone());
    let code = LedgerEntry {
        last_modified_ledger_seq: out.sequence_number,
        data: LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash: hash.clone(),
            code: wasm.to_vec().try_into().map_err(|_| "candidate Wasm too large".to_string())?,
        }),
        ext: LedgerEntryExt::V0,
    };
    let key = LedgerKey::ContractCode(LedgerKeyContractCode { hash });
    out.ledger_entries.retain(|(k, _)| **k != key);
    out.ledger_entries.push((Box::new(key), (Box::new(code), instance_ttl)));
    Ok(out)
}

/// Contract error codes -> names, read from the Wasm's own contract spec.
pub fn error_names(wasm: &[u8]) -> BTreeMap<u32, String> {
    let mut names = BTreeMap::new();
    let Some(spec) = custom_section(wasm, "contractspecv0") else { return names };
    let len = spec.len() as u64;
    let mut r = Limited::new(Cursor::new(spec), Limits::none());
    while r.inner.position() < len {
        match ScSpecEntry::read_xdr(&mut r) {
            Ok(ScSpecEntry::UdtErrorEnumV0(e)) => {
                for c in e.cases.iter() {
                    names.insert(c.value, c.name.to_utf8_string_lossy());
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    names
}

pub fn custom_section<'a>(wasm: &'a [u8], name: &str) -> Option<&'a [u8]> {
    fn leb(b: &[u8], i: &mut usize) -> Option<usize> {
        let (mut v, mut shift) = (0usize, 0);
        loop {
            let byte = *b.get(*i)?;
            *i += 1;
            v |= ((byte & 0x7f) as usize) << shift;
            shift += 7;
            if byte < 0x80 {
                return Some(v);
            }
        }
    }
    if wasm.get(..4)? != b"\0asm" {
        return None;
    }
    let mut i = 8;
    while i < wasm.len() {
        let id = wasm[i];
        i += 1;
        let size = leb(wasm, &mut i)?;
        let end = i.checked_add(size)?;
        if id == 0 {
            let mut j = i;
            let n = leb(wasm, &mut j)?;
            if wasm.get(j..j + n)? == name.as_bytes() {
                return wasm.get(j + n..end);
            }
        }
        i = end;
    }
    None
}

fn describe_contract_error(code: u32, names: &BTreeMap<u32, String>) -> String {
    match names.get(&code) {
        Some(name) => format!("error: {name} (Error(Contract, #{code}))"),
        None => format!("error: Error(Contract, #{code})"),
    }
}

/// Run every manifest step, in order, in one Env built from `snap`.
pub fn run(snap: LedgerSnapshot, manifest: &Manifest, label: &str) -> Result<Branch, String> {
    run_inner(snap, manifest, label, None, Auth::Mocked)
}

/// Run a candidate. By default its code is swapped in directly; when the manifest has an
/// `upgrade` block, the code is only installed and the contract's own upgrade function
/// (then any migrate calls) puts it in place before the workflow runs.
pub fn candidate(snap: &LedgerSnapshot, manifest: &Manifest, label: &str, wasm: &[u8]) -> Result<Branch, String> {
    candidate_with(snap, manifest, label, wasm, Auth::Mocked)
}

pub fn candidate_with(snap: &LedgerSnapshot, manifest: &Manifest, label: &str, wasm: &[u8], auth: Auth) -> Result<Branch, String> {
    let contract = ScAddress::from_str(&manifest.contract).map_err(|e| format!("bad contract address: {e}"))?;
    match &manifest.upgrade {
        None => run_inner(with_candidate(snap, &contract, wasm)?, manifest, label, None, auth),
        Some(_) => run_inner(install_code(snap, &contract, wasm)?, manifest, label, Some(wasm), auth),
    }
}

/// The deployed code with checking on, against its own signed requests.
pub fn run_signed(snap: LedgerSnapshot, manifest: &Manifest, payloads: &[Vec<crate::signing::Payload>]) -> Result<Branch, String> {
    run_inner(snap, manifest, "baseline (signed)", None, Auth::Signed(payloads))
}

/// A copy of `snap` with `wasm` installed as a code entry (TTL of the contract instance), but
/// the contract still pointing at its deployed code.
pub fn install_code(snap: &LedgerSnapshot, contract: &ScAddress, wasm: &[u8]) -> Result<LedgerSnapshot, String> {
    let mut out = snap.clone();
    let hash = Hash(Sha256::digest(wasm).into());
    let ttl = out
        .ledger_entries
        .iter()
        .find_map(|(_, (entry, ttl))| match &entry.data {
            LedgerEntryData::ContractData(d) if &d.contract == contract && d.key == ScVal::LedgerKeyContractInstance => Some(*ttl),
            _ => None,
        })
        .ok_or_else(|| format!("snapshot has no instance entry for {contract}"))?;
    let code = LedgerEntry {
        last_modified_ledger_seq: out.sequence_number,
        data: LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash: hash.clone(),
            code: wasm.to_vec().try_into().map_err(|_| "candidate Wasm too large".to_string())?,
        }),
        ext: LedgerEntryExt::V0,
    };
    let key = LedgerKey::ContractCode(LedgerKeyContractCode { hash });
    out.ledger_entries.retain(|(k, _)| **k != key);
    out.ledger_entries.push((Box::new(key), (Box::new(code), ttl)));
    Ok(out)
}

fn run_inner(snap: LedgerSnapshot, manifest: &Manifest, label: &str, via_upgrade: Option<&[u8]>, auth: Auth) -> Result<Branch, String> {
    let contract = ScAddress::from_str(&manifest.contract)
        .map_err(|e| format!("bad contract address: {e}"))?;
    let wasm = match via_upgrade {
        Some(w) => w.to_vec(),
        None => deployed_wasm(&snap, &contract)?,
    };
    let names = error_names(&wasm);
    let candidate_hash: [u8; 32] = Sha256::digest(&wasm).into();
    let snap = Rc::new(snap);
    let source = Rc::new(RecordingSource { inner: snap.clone(), misses: RefCell::default() });
    let env = Env::from_ledger_snapshot(SnapshotSourceInput {
        source: source.clone(),
        ledger_info: Some(snap.ledger_info()),
        snapshot: Some(snap.clone()),
    });
    // Mocked: every require_auth is satisfied. Signed runs switch per call below; setup calls
    // (upgrade, migrate) stay mocked because they are not part of the compared workflow.
    env.mock_all_auths();
    let signed_snap = snap.clone();
    let target = Address::from_str(&env, &manifest.contract);

    let invoke = |label: &str, call: &str, args: &[crate::manifest::Arg]| -> Result<StepResult, String> {
        let mut vals = soroban_sdk::Vec::<Val>::new(&env);
        for a in args {
            let sc = a.to_scval_with(Some(&candidate_hash))?;
            vals.push_back(Val::try_from_val(&env, &sc).map_err(|e| format!("arg {sc:?}: {e:?}"))?);
        }
        let events_before: Vec<soroban_sdk::xdr::ContractEvent> = env.events().all().events().to_vec();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            env.try_invoke_contract::<Val, soroban_sdk::Error>(&target, &Symbol::new(&env, call), vals)
        }));
        let (result, failed) = match outcome {
            Ok(Ok(Ok(v))) => (
                ScVal::try_from_val(&env, &v)
                    .map(|s| fmt::scval(&s))
                    .unwrap_or_else(|_| "<unconvertible value>".into()),
                false,
            ),
            Ok(Ok(Err(e))) => (format!("error: return value conversion {e:?}"), true),
            Ok(Err(Ok(err))) if err.is_type(ScErrorType::Contract) => {
                (describe_contract_error(err.get_code(), &names), true)
            }
            Ok(Err(Ok(err))) if err.is_type(ScErrorType::Auth) => (format!("error: authorization rejected ({err:?})"), true),
            Ok(Err(Ok(err))) => (format!("error: {err:?}"), true),
            Ok(Err(Err(InvokeError::Contract(code)))) => (describe_contract_error(code, &names), true),
            Ok(Err(Err(InvokeError::Abort))) => ("error: aborted".into(), true),
            Err(_) => ("error: host panicked".into(), true),
        };
        let mut auths: Vec<String> = env
            .auths()
            .iter()
            .map(|(a, inv)| format!("{} authorizes {}", val_str(&env, &a.to_val()), invocation_str(&env, inv)))
            .collect();
        auths.sort();
        // The host's event buffer may accumulate across calls; keep only what this call added.
        let after: Vec<soroban_sdk::xdr::ContractEvent> = env.events().all().events().to_vec();
        let new_events = if after.starts_with(&events_before) { &after[events_before.len()..] } else { &after[..] };
        // The exact payloads behind `auths`, for signing in a later signed run. Only meaningful
        // while mocking (recording); the host keeps them for the most recent call.
        let payloads: Vec<crate::signing::Payload> = env
            .host()
            .get_recorded_auth_payloads()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.address.map(|a| (a, p.invocation)))
            .collect();
        Ok(StepResult {
            label: label.into(),
            call: call.into(),
            args: args.iter().map(|a| a.display()).collect(),
            result,
            failed,
            auths,
            events: new_events.iter().map(event_str).collect(),
            payloads,
            signature_checked: None,
        })
    };

    // Upgrade path: the contract's own upgrade call, then migrations. Kept apart from the
    // compared workflow steps so step indexes line up across versions.
    let (mut upgrade, mut migrate, mut installed) = (None, Vec::new(), None);
    if let (Some(_), Some(u)) = (via_upgrade, &manifest.upgrade) {
        let up = invoke("upgrade", &u.call, &u.args)?;
        let ok = !up.failed;
        upgrade = Some(up);
        if ok {
            for m in &u.migrate {
                migrate.push(invoke(&m.label, &m.call, &m.args)?);
            }
        }
        let now = env.to_ledger_snapshot();
        installed = Some(deployed_wasm(&now, &contract).map(|w| Sha256::digest(&w)[..] == candidate_hash[..]).unwrap_or(false));
    }

    let mut steps = Vec::new();
    for (i, step) in manifest.steps.iter().enumerate() {
        let checked = match &auth {
            Auth::Mocked => None,
            Auth::Signed(all) => {
                let p = all.get(i).map(Vec::as_slice).unwrap_or(&[]);
                if crate::signing::all_plain_accounts(p) {
                    env.set_auths(&crate::signing::sign(&signed_snap, i, p));
                    Some(true)
                } else {
                    env.mock_all_auths();
                    Some(false)
                }
            }
        };
        let mut r = invoke(&step.label, &step.call, &step.args)?;
        if checked.is_some() {
            r.payloads.clear();
            r.signature_checked = checked;
        }
        steps.push(r);
    }

    let mut final_state = BTreeMap::new();
    for (key, (entry, _)) in env.to_ledger_snapshot().ledger_entries.iter() {
        let LedgerEntryData::ContractData(d) = &entry.data else { continue };
        if d.contract != contract {
            continue;
        }
        if d.key == ScVal::LedgerKeyContractInstance {
            if let ScVal::ContractInstance(i) = &d.val {
                for e in i.storage.iter().flat_map(|m| m.iter()) {
                    final_state.insert(format!("instance {}", fmt::scval(&e.key)), fmt::scval(&e.val));
                }
            }
        } else {
            final_state.insert(fmt::ledger_key(key), fmt::scval(&d.val));
        }
    }
    let misses = source.misses.borrow().clone();
    Ok(Branch { label: label.into(), wasm_sha256: sha256_hex(&wasm), steps, final_state, misses, upgrade, migrate, installed })
}

/// Result of `--check-signatures` for one candidate.
pub struct SignedFinding {
    pub step: usize,
    pub label: String,
    /// "rejected": signatures made for the deployed contract are refused by the candidate.
    /// "new requirement": the candidate requires an authorization the deployed contract didn't.
    pub kind: &'static str,
    pub detail: String,
}

pub struct SignedCandidate {
    pub label: String,
    pub findings: Vec<SignedFinding>,
    /// Workflow calls whose signatures were not checked (a contract account had to authorize).
    pub unchecked: Vec<String>,
    /// Keys the signed run read outside the capture, other than nonces and the test-signer accounts.
    pub outside: Vec<LedgerKey>,
}

pub struct SignedCheck {
    /// None when the check ran; otherwise why it could not.
    pub unavailable: Option<String>,
    pub accounts: usize,
    pub unchecked: Vec<String>,
    pub candidates: Vec<SignedCandidate>,
}

fn synthetic(k: &LedgerKey, touched: &std::collections::BTreeSet<LedgerKey>) -> bool {
    touched.contains(k) || matches!(k, LedgerKey::ContractData(d) if matches!(d.key, ScVal::LedgerKeyNonce(_)))
}

/// Sign the deployed contract's own requests with test keys, check the deployed contract
/// accepts them (otherwise the check is unavailable), then replay each candidate with
/// checking on. Answers "do signatures made for the deployed contract still work on this
/// candidate?". A candidate that drops a requirement still passes here; the
/// required-authorization comparison is what flags that.
pub fn signed_check(
    snap: &LedgerSnapshot,
    manifest: &Manifest,
    baseline: &Branch,
    candidates: &[(&Branch, &[u8])],
    captured: &std::collections::BTreeSet<String>,
) -> Result<SignedCheck, String> {
    let payloads: Vec<Vec<crate::signing::Payload>> = baseline.steps.iter().map(|s| s.payloads.clone()).collect();
    let (signed_snap, touched) = crate::signing::with_test_signers(snap, &payloads);
    let base = run_signed(signed_snap.clone(), manifest, &payloads)?;
    let unchecked: Vec<String> = base.steps.iter().filter(|s| s.signature_checked == Some(false)).map(|s| s.label.clone()).collect();
    if let Some((b, r)) = base.steps.iter().zip(&baseline.steps).find(|(b, r)| b.result != r.result) {
        return Ok(SignedCheck {
            unavailable: Some(format!(
                "the deployed contract did not accept its own signed requests at \"{}\" ({} instead of {})",
                r.label, b.result, r.result
            )),
            accounts: touched.len(),
            unchecked,
            candidates: Vec::new(),
        });
    }
    let mut out = Vec::new();
    for (recorded, wasm) in candidates {
        let signed = candidate_with(&signed_snap, manifest, &recorded.label, wasm, Auth::Signed(&payloads))?;
        let mut findings = Vec::new();
        for (i, (sg, rec)) in signed.steps.iter().zip(&recorded.steps).enumerate() {
            // Only calls that succeed without signature checking: anything else is already a
            // result difference and says nothing about signatures.
            if sg.signature_checked != Some(true) || rec.failed || !sg.failed {
                continue;
            }
            let kind = if baseline.steps[i].payloads.is_empty() && !rec.payloads.is_empty() { "new requirement" } else { "rejected" };
            findings.push(SignedFinding { step: i + 1, label: rec.label.clone(), kind, detail: sg.result.clone() });
        }
        let outside = signed
            .misses
            .iter()
            .filter(|(id, k)| !captured.contains(*id) && !synthetic(k, &touched))
            .map(|(_, k)| k.clone())
            .collect();
        let unchecked = signed.steps.iter().filter(|s| s.signature_checked == Some(false)).map(|s| s.label.clone()).collect();
        out.push(SignedCandidate { label: recorded.label.clone(), findings, unchecked, outside });
    }
    Ok(SignedCheck { unavailable: None, accounts: touched.len(), unchecked, candidates: out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn demo(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../demo").join(path)
    }

    #[test]
    fn contract_errors_are_named_from_the_wasm_spec() {
        let wasm = std::fs::read(demo("wasm/token-v1.wasm")).unwrap();
        assert!(custom_section(&wasm, "contractspecv0").is_some());
        assert!(custom_section(&wasm, "no-such-section").is_none());
        let names = error_names(&wasm);
        assert_eq!(names.get(&1).map(String::as_str), Some("InvalidAmount"));
        assert_eq!(names.get(&2).map(String::as_str), Some("InsufficientBalance"));
        assert!(custom_section(b"not wasm", "contractspecv0").is_none());
    }

    #[test]
    fn demo_capture_replays_to_the_committed_result() {
        let manifest = Manifest::load(&demo("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(demo("capture/snapshot.json")).unwrap();
        let contract = ScAddress::from_str(&manifest.contract).unwrap();

        let base = run(snap.clone(), &manifest, "baseline").unwrap();
        assert_eq!(base.steps[0].result, "12400000000");
        assert_eq!(base.steps[4].result, "()");
        assert_eq!(base.steps[5].result, "11400000000");

        let broken = std::fs::read(demo("wasm/token-v2-broken.wasm")).unwrap();
        let cand = run(with_candidate(&snap, &contract, &broken).unwrap(), &manifest, "v2-broken").unwrap();
        assert_eq!(cand.wasm_sha256, sha256_hex(&broken));
        assert_eq!(cand.steps[0].result, "0");
        assert!(cand.steps[4].failed);
        assert!(cand.steps[4].result.contains("InsufficientBalance"));
        assert!(cand.misses.values().any(|k| fmt::ledger_key(k).contains("BalanceOf(")));
    }

    #[test]
    fn authorizations_are_recorded_per_call_without_carry_over() {
        let manifest = Manifest::load(&demo("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(demo("capture/snapshot.json")).unwrap();
        let base = run(snap, &manifest, "baseline").unwrap();
        let transfer = &base.steps[4];
        assert_eq!(transfer.auths.len(), 1);
        assert!(transfer.auths[0].starts_with("GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3 authorizes "));
        assert!(transfer.auths[0].ends_with(".transfer(GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3, GAA4S5N72PZFRKUUNNA2RZM6P73FLJTTVBBF7NVKVQTUSXYXYVFKPJGI, 1000000000)"));
        // Calls that need no authorization record none, even right after one that did.
        for i in [0, 1, 2, 3, 5, 6, 7] {
            assert!(base.steps[i].auths.is_empty(), "step {} carried auths: {:?}", i + 1, base.steps[i].auths);
        }
        // The demo token emits no events.
        assert!(base.steps.iter().all(|s| s.events.is_empty()));
    }

    #[test]
    fn a_narrower_signature_scope_is_visible_even_when_results_match() {
        let manifest = Manifest::load(&demo("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(demo("capture/snapshot.json")).unwrap();
        let contract = ScAddress::from_str(&manifest.contract).unwrap();
        let base = run(snap.clone(), &manifest, "baseline").unwrap();
        let wasm = std::fs::read(demo("wasm/token-v2-authscope.wasm")).unwrap();
        let cand = run(with_candidate(&snap, &contract, &wasm).unwrap(), &manifest, "v2-authscope").unwrap();
        for (b, c) in base.steps.iter().zip(&cand.steps) {
            assert_eq!(b.result, c.result, "{}", b.label);
        }
        assert_ne!(base.steps[4].auths, cand.steps[4].auths);
        assert!(cand.steps[4].auths[0].ends_with(".transfer(GAA4S5N72PZFRKUUNNA2RZM6P73FLJTTVBBF7NVKVQTUSXYXYVFKPJGI)"));
    }

    #[test]
    fn events_are_recorded_per_call_and_a_swapped_event_shows() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/events");
        let manifest = Manifest::load(&dir.join("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(dir.join("capture/snapshot.json")).unwrap();
        let contract = ScAddress::from_str(&manifest.contract).unwrap();
        let base = run(snap.clone(), &manifest, "baseline").unwrap();
        let only_transfer: Vec<usize> = base.steps.iter().enumerate().filter(|(_, s)| !s.events.is_empty()).map(|(i, _)| i).collect();
        assert_eq!(only_transfer, vec![2], "only the transfer emits");
        assert!(base.steps[2].events[0].contains("[transfer, GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3, GAA4S5N72PZFRKUUNNA2RZM6P73FLJTTVBBF7NVKVQTUSXYXYVFKPJGI]"));
        let wasm = std::fs::read(demo("wasm/token-events-v2-swapped.wasm")).unwrap();
        let cand = run(with_candidate(&snap, &contract, &wasm).unwrap(), &manifest, "v2-swapped").unwrap();
        for (b, c) in base.steps.iter().zip(&cand.steps) {
            assert_eq!(b.result, c.result, "{}", b.label);
        }
        assert!(cand.steps[2].events[0].contains("[transfer, GAA4S5N72PZFRKUUNNA2RZM6P73FLJTTVBBF7NVKVQTUSXYXYVFKPJGI, GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3]"));
    }
    fn upgrade_example() -> (Manifest, LedgerSnapshot) {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/upgrade-path");
        (Manifest::load(&dir.join("manifest.json")).unwrap(), LedgerSnapshot::read_file(dir.join("capture/snapshot.json")).unwrap())
    }

    #[test]
    fn candidates_go_in_through_the_contracts_own_upgrade_function() {
        let (manifest, snap) = upgrade_example();
        let wasm = std::fs::read(demo("wasm/token-v2-broken.wasm")).unwrap();
        let cand = candidate(&snap, &manifest, "v2-broken", &wasm).unwrap();
        let up = cand.upgrade.as_ref().unwrap();
        assert!(!up.failed, "{}", up.result);
        assert_eq!(cand.installed, Some(true));
        assert!(up.auths[0].ends_with(&format!(".upgrade(0x{})", sha256_hex(&wasm))));
        assert_eq!(cand.steps[0].result, "0", "the workflow ran on the candidate");
    }

    #[test]
    fn migrate_steps_run_after_the_upgrade_and_are_recorded() {
        let (mut manifest, snap) = upgrade_example();
        let step: crate::manifest::Step =
            serde_json::from_str(r#"{"label": "check decimals", "call": "decimals", "args": []}"#).unwrap();
        manifest.upgrade.as_mut().unwrap().migrate = vec![step];
        let wasm = std::fs::read(demo("wasm/token-v2-compatible.wasm")).unwrap();
        let cand = candidate(&snap, &manifest, "v2-compatible", &wasm).unwrap();
        assert_eq!(cand.migrate.len(), 1);
        assert!(!cand.migrate[0].failed);
        assert_eq!(cand.migrate[0].result, "7");
        assert_eq!(cand.installed, Some(true));
    }

    #[test]
    fn a_failed_upgrade_is_reported_and_the_old_code_keeps_running() {
        let (mut manifest, snap) = upgrade_example();
        manifest.upgrade.as_mut().unwrap().call = "no_such_function".into();
        let wasm = std::fs::read(demo("wasm/token-v2-broken.wasm")).unwrap();
        let cand = candidate(&snap, &manifest, "v2-broken", &wasm).unwrap();
        assert!(cand.upgrade.as_ref().unwrap().failed);
        assert_eq!(cand.installed, Some(false));
        assert_eq!(cand.steps[0].result, "12400000000", "the deployed code answered");
    }

    fn demo_signed(cands: &[&str]) -> (SignedCheck, Branch) {
        let manifest = Manifest::load(&demo("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(demo("capture/snapshot.json")).unwrap();
        let base = run(snap.clone(), &manifest, "baseline").unwrap();
        let wasms: Vec<Vec<u8>> = cands.iter().map(|c| std::fs::read(demo(&format!("wasm/{c}.wasm"))).unwrap()).collect();
        let recs: Vec<Branch> = cands.iter().zip(&wasms).map(|(c, w)| candidate(&snap, &manifest, c, w).unwrap()).collect();
        let pairs: Vec<(&Branch, &[u8])> = recs.iter().zip(&wasms).map(|(r, w)| (r, w.as_slice())).collect();
        let captured = snap.ledger_entries.iter().map(|(k, _)| key_id(k)).collect();
        (signed_check(&snap, &manifest, &base, &pairs, &captured).unwrap(), base)
    }

    #[test]
    fn payloads_are_recorded_per_call() {
        let (_, base) = demo_signed(&[]);
        let with: Vec<usize> = base.steps.iter().enumerate().filter(|(_, s)| !s.payloads.is_empty()).map(|(i, _)| i).collect();
        assert_eq!(with, vec![4], "only the transfer requires authorization");
    }

    #[test]
    fn signatures_for_the_deployed_contract_work_on_compatible_and_fail_on_authscope() {
        let (check, _) = demo_signed(&["token-v2-compatible", "token-v2-authscope"]);
        assert!(check.unavailable.is_none(), "{:?}", check.unavailable);
        assert_eq!(check.accounts, 1);
        assert!(check.candidates[0].findings.is_empty(), "compatible accepts the signed transfer");
        let f = &check.candidates[1].findings;
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].step, f[0].kind), (5, "rejected"));
        assert!(check.candidates.iter().all(|c| c.outside.is_empty() && c.unchecked.is_empty()));
    }

    #[test]
    fn a_candidate_failing_for_other_reasons_is_not_a_signature_finding() {
        // v2-broken's transfer fails with InsufficientBalance with or without signatures.
        let (check, _) = demo_signed(&["token-v2-broken"]);
        assert!(check.candidates[0].findings.is_empty());
    }

    #[test]
    fn the_signed_check_is_deterministic() {
        let a = demo_signed(&["token-v2-authscope"]).0;
        let b = demo_signed(&["token-v2-authscope"]).0;
        assert_eq!(a.candidates[0].findings[0].detail, b.candidates[0].findings[0].detail);
        let manifest = Manifest::load(&demo("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(demo("capture/snapshot.json")).unwrap();
        let base = run(snap.clone(), &manifest, "baseline").unwrap();
        let p: Vec<Vec<crate::signing::Payload>> = base.steps.iter().map(|s| s.payloads.clone()).collect();
        assert_eq!(crate::signing::sign(&snap, 4, &p[4]), crate::signing::sign(&snap, 4, &p[4]));
    }

    #[test]
    fn the_check_is_unavailable_when_the_deployed_contract_rejects_its_own_requests() {
        let manifest = Manifest::load(&demo("manifest.json")).unwrap();
        let snap = LedgerSnapshot::read_file(demo("capture/snapshot.json")).unwrap();
        let mut base = run(snap.clone(), &manifest, "baseline").unwrap();
        base.steps[4].payloads.clear(); // nothing signed for the transfer
        let check = signed_check(&snap, &manifest, &base, &[], &Default::default()).unwrap();
        assert!(check.unavailable.unwrap().contains("A sends 100 RHD to B"));
    }
}
