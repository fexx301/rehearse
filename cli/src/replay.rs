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
use soroban_sdk::testutils::{SnapshotSource, SnapshotSourceInput};
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
}

pub struct Branch {
    pub label: String,
    pub wasm_sha256: String,
    pub steps: Vec<StepResult>,
    /// Contract storage after the workflow: readable key -> readable value.
    pub final_state: BTreeMap<String, String>,
    /// Keys read during the workflow that the snapshot did not contain.
    pub misses: BTreeMap<String, LedgerKey>,
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
    let contract = ScAddress::from_str(&manifest.contract)
        .map_err(|e| format!("bad contract address: {e}"))?;
    let wasm = deployed_wasm(&snap, &contract)?;
    let names = error_names(&wasm);
    let snap = Rc::new(snap);
    let source = Rc::new(RecordingSource { inner: snap.clone(), misses: RefCell::default() });
    let env = Env::from_ledger_snapshot(SnapshotSourceInput {
        source: source.clone(),
        ledger_info: Some(snap.ledger_info()),
        snapshot: Some(snap.clone()),
    });
    // Signatures are not reproduced offline; every require_auth is satisfied.
    env.mock_all_auths();
    let target = Address::from_str(&env, &manifest.contract);

    let mut steps = Vec::new();
    for step in &manifest.steps {
        let mut args = soroban_sdk::Vec::<Val>::new(&env);
        for a in &step.args {
            let sc = a.to_scval()?;
            args.push_back(Val::try_from_val(&env, &sc).map_err(|e| format!("arg {sc:?}: {e:?}"))?);
        }
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            env.try_invoke_contract::<Val, soroban_sdk::Error>(
                &target,
                &Symbol::new(&env, &step.call),
                args,
            )
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
            Ok(Err(Ok(err))) => (format!("error: {err:?}"), true),
            Ok(Err(Err(InvokeError::Contract(code)))) => (describe_contract_error(code, &names), true),
            Ok(Err(Err(InvokeError::Abort))) => ("error: aborted".into(), true),
            Err(_) => ("error: host panicked".into(), true),
        };
        steps.push(StepResult {
            label: step.label.clone(),
            call: step.call.clone(),
            args: step.args.iter().map(|a| a.display()).collect(),
            result,
            failed,
        });
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
    Ok(Branch { label: label.into(), wasm_sha256: sha256_hex(&wasm), steps, final_state, misses })
}
