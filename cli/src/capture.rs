//! Capture: save exactly the ledger state a workflow touches, all read at one ledger.
//!
//! 1. Simulate every manifest step on the live network; the union of the
//!    returned footprints is the baseline key set.
//! 2. Fetch every key in one `getLedgerEntries` call, so all entries (and all
//!    verified absences) come from a single ledger.
//! 3. Replay the baseline and each candidate locally against that snapshot.
//!    Any key a candidate reads that was not captured is added, and step 2
//!    repeats until no new keys appear. After that, every value a candidate
//!    sees is either real captured state or a verified absence at the same ledger.
use crate::{fmt, manifest::Manifest, replay, rpc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::xdr::{
    HostFunction, InvokeContractArgs, InvokeHostFunctionOp, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerHeaderHistoryEntry, LedgerKey, Limits, Memo, MuxedAccount, Operation,
    OperationBody, Preconditions, PublicKey, ReadXdr, ScAddress, ScSymbol, ScVal, SequenceNumber,
    SorobanTransactionData, Transaction, TransactionEnvelope, TransactionExt,
    TransactionV1Envelope, Uint256, WriteXdr,
};
use soroban_sdk::Env;
use std::{collections::BTreeMap, fs, path::Path, str::FromStr};

const MAX_ROUNDS: usize = 5;
const MAX_KEYS: usize = 200; // getLedgerEntries limit; one call keeps a single ledger.

fn invoke_envelope(source: &str, contract: &str, call: &str, args: Vec<ScVal>) -> Result<String, String> {
    let account = match ScAddress::from_str(source).map_err(|e| format!("bad source account: {e}"))? {
        ScAddress::Account(soroban_sdk::xdr::AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(k)))) => k,
        _ => return Err("simulation source must be a G... account".into()),
    };
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: ScAddress::from_str(contract).map_err(|e| format!("bad contract: {e}"))?,
                function_name: ScSymbol(call.try_into().map_err(|e| format!("bad function name: {e:?}"))?),
                args: args.try_into().map_err(|e| format!("too many args: {e:?}"))?,
            }),
            auth: Default::default(),
        }),
    };
    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256(account)),
        fee: 100,
        seq_num: SequenceNumber(0),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: vec![op].try_into().unwrap(),
        ext: TransactionExt::V0,
    };
    TransactionEnvelope::Tx(TransactionV1Envelope { tx, signatures: Default::default() })
        .to_xdr_base64(Limits::none())
        .map_err(|e| format!("encode transaction: {e}"))
}

struct Fetched {
    ledger: u32,
    entries: Vec<(LedgerEntry, Option<u32>)>,
    absent: Vec<LedgerKey>,
    raw: Value,
}

fn fetch(rpc_url: &str, keys: &BTreeMap<String, LedgerKey>) -> Result<Fetched, String> {
    if keys.len() > MAX_KEYS {
        return Err(format!("{} keys exceed the single-call limit of {MAX_KEYS}", keys.len()));
    }
    let ids: Vec<&String> = keys.keys().collect();
    let raw = rpc::call(rpc_url, "getLedgerEntries", json!({ "keys": ids }))?;
    let ledger = raw["latestLedger"].as_u64().ok_or("getLedgerEntries: no latestLedger")? as u32;
    let mut entries = Vec::new();
    let mut present = std::collections::BTreeSet::new();
    for e in raw["entries"].as_array().into_iter().flatten() {
        let key = e["key"].as_str().ok_or("entry without key")?;
        present.insert(key.to_string());
        let data = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().ok_or("entry without xdr")?, Limits::none())
            .map_err(|e| format!("decode entry: {e}"))?;
        let entry = LedgerEntry {
            last_modified_ledger_seq: e["lastModifiedLedgerSeq"].as_u64().unwrap_or(0) as u32,
            data,
            ext: LedgerEntryExt::V0,
        };
        entries.push((entry, e["liveUntilLedgerSeq"].as_u64().map(|v| v as u32)));
    }
    let absent = keys.iter().filter(|(id, _)| !present.contains(*id)).map(|(_, k)| k.clone()).collect();
    Ok(Fetched { ledger, entries, absent, raw })
}

fn snapshot_at(rpc_url: &str, passphrase: &str, fetched: &Fetched) -> Result<(LedgerSnapshot, Value), String> {
    let header_raw = rpc::call(
        rpc_url,
        "getLedgers",
        json!({"startLedger": fetched.ledger, "pagination": {"limit": 1}}),
    )?;
    let header_xdr = header_raw["ledgers"][0]["headerXdr"].as_str().ok_or("getLedgers: no header")?;
    let header = LedgerHeaderHistoryEntry::from_xdr_base64(header_xdr, Limits::none())
        .map_err(|e| format!("decode header: {e}"))?
        .header;
    if header.ledger_seq != fetched.ledger {
        return Err(format!("header ledger {} != entries ledger {}", header.ledger_seq, fetched.ledger));
    }
    let mut snap = Env::default().to_ledger_snapshot();
    snap.protocol_version = header.ledger_version;
    snap.sequence_number = header.ledger_seq;
    snap.timestamp = header.scp_value.close_time.0;
    snap.base_reserve = header.base_reserve;
    snap.network_id = Sha256::digest(passphrase.as_bytes()).into();
    snap.ledger_entries.clear();
    for (entry, ttl) in &fetched.entries {
        snap.ledger_entries.push((Box::new(entry.to_key()), (Box::new(entry.clone()), *ttl)));
    }
    Ok((snap, header_raw))
}

pub fn capture(
    manifest: &Manifest,
    source: &str,
    candidates: &[(String, Vec<u8>)],
    out: &Path,
) -> Result<(), String> {
    let contract = ScAddress::from_str(&manifest.contract).map_err(|e| format!("bad contract: {e}"))?;
    let network = rpc::call(&manifest.rpc, "getNetwork", json!({}))?;
    let passphrase = network["passphrase"].as_str().ok_or("getNetwork: no passphrase")?.to_string();

    // 1. Baseline footprint from live simulation of every step.
    let mut keys: BTreeMap<String, LedgerKey> = BTreeMap::new();
    let mut simulations = Vec::new();
    for step in &manifest.steps {
        let args = step.args.iter().map(|a| a.to_scval()).collect::<Result<Vec<_>, _>>()?;
        let tx = invoke_envelope(source, &manifest.contract, &step.call, args)?;
        let sim = rpc::call(&manifest.rpc, "simulateTransaction", json!({ "transaction": tx }))?;
        if let Some(err) = sim.get("error") {
            return Err(format!("live simulation of step '{}' failed: {err}", step.label));
        }
        let data = SorobanTransactionData::from_xdr_base64(
            sim["transactionData"].as_str().ok_or("simulation returned no transactionData")?,
            Limits::none(),
        )
        .map_err(|e| format!("decode transactionData: {e}"))?;
        let fp = &data.resources.footprint;
        for k in fp.read_only.iter().chain(fp.read_write.iter()) {
            keys.insert(replay::key_id(k), k.clone());
        }
        simulations.push(json!({
            "step": step.label,
            "simulated_at_ledger": sim["latestLedger"],
            "footprint_keys": fp.read_only.len() + fp.read_write.len(),
        }));
    }

    // 2-3. Fetch at one ledger, replay locally, add any key the replays read, repeat.
    let mut rounds = Vec::new();
    for round in 1..=MAX_ROUNDS {
        let fetched = fetch(&manifest.rpc, &keys)?;
        let (snap, header_raw) = snapshot_at(&manifest.rpc, &passphrase, &fetched)?;
        let mut branches = vec![replay::run(snap.clone(), manifest, "baseline")?];
        for (label, wasm) in candidates {
            branches.push(replay::candidate(&snap, manifest, label, wasm)?);
        }
        let mut added = Vec::new();
        for b in &branches {
            for (id, k) in &b.misses {
                if !keys.contains_key(id) {
                    keys.insert(id.clone(), k.clone());
                    added.push(json!({"key": fmt::ledger_key(k), "read_by": b.label}));
                }
            }
        }
        rounds.push(json!({"round": round, "ledger": fetched.ledger, "keys": keys.len() - added.len(), "added": added}));
        if !added.is_empty() {
            continue;
        }

        fs::create_dir_all(out).map_err(|e| format!("create {}: {e}", out.display()))?;
        let snapshot_path = out.join("snapshot.json");
        snap.write_file(&snapshot_path).map_err(|e| format!("write snapshot: {e}"))?;
        let deployed = replay::deployed_wasm(&snap, &contract)?;
        fs::write(out.join("deployed.wasm"), &deployed).map_err(|e| e.to_string())?;
        write_json(&out.join("rpc-entries.json"), &fetched.raw)?;
        write_json(&out.join("rpc-header.json"), &header_raw)?;
        let captured: Vec<String> = fetched.entries.iter().map(|(e, _)| fmt::ledger_key(&e.to_key())).collect();
        let absent: Vec<String> = fetched.absent.iter().map(fmt::ledger_key).collect();
        let absent_ids: Vec<String> = fetched.absent.iter().map(replay::key_id).collect();
        let provenance = json!({
            "tool": concat!("rehearse ", env!("CARGO_PKG_VERSION")),
            "network": manifest.network,
            "network_passphrase": passphrase,
            "rpc": manifest.rpc,
            "contract": manifest.contract,
            "ledger": fetched.ledger,
            "protocol": snap.protocol_version,
            "ledger_close_time": snap.timestamp,
            "deployed_wasm_sha256": replay::sha256_hex(&deployed),
            "snapshot_sha256": replay::sha256_hex(&fs::read(&snapshot_path).map_err(|e| e.to_string())?),
            "simulation_source": source,
            "simulations": simulations,
            "closure_rounds": rounds,
            "closure_candidates": candidates.iter().map(|(l, w)| json!({"label": l, "wasm_sha256": replay::sha256_hex(w)})).collect::<Vec<_>>(),
            "keys_captured": captured,
            "keys_verified_absent": absent,
            "keys_verified_absent_xdr": absent_ids,
            "ttl_configuration": "SDK default state-archival settings, not the network's",
        });
        write_json(&out.join("provenance.json"), &provenance)?;
        println!(
            "Captured {} entries ({} verified absent) at {} ledger {}, protocol {}, in {} round(s)",
            captured.len(),
            absent.len(),
            manifest.network,
            fetched.ledger,
            snap.protocol_version,
            round
        );
        return Ok(());
    }
    Err(format!("footprint did not converge after {MAX_ROUNDS} rounds"))
}

pub fn write_json(path: &Path, v: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}
