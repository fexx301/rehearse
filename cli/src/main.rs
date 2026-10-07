//! rehearse: replay a Soroban contract's real workflow against candidate Wasm,
//! on captured ledger state, before upgrading.
//!
//!   rehearse capture --manifest M --source G... --out DIR [--candidate LABEL=WASM]...
//!   rehearse replay  --manifest M --capture DIR --out REPORT.json --candidate LABEL=WASM... [--fail-on-divergence]
mod capture;
mod fmt;
mod manifest;
mod render;
mod replay;
mod rpc;

use manifest::Manifest;
use serde_json::{json, Value};
use soroban_ledger_snapshot::LedgerSnapshot;
use std::{collections::BTreeSet, fs, path::PathBuf};

const USAGE: &str = "usage:
  rehearse capture --manifest M --source G... --out DIR [--candidate LABEL=WASM]...
  rehearse replay  --manifest M --capture DIR --out REPORT.json --candidate LABEL=WASM... [--fail-on-divergence]
  rehearse render  --report REPORT.json --out REPORT.html";

struct Args {
    manifest: Option<PathBuf>,
    source: Option<String>,
    out: Option<PathBuf>,
    capture: Option<PathBuf>,
    report: Option<PathBuf>,
    candidates: Vec<(String, PathBuf)>,
    fail_on_divergence: bool,
}

fn parse(rest: &[String]) -> Result<Args, String> {
    let mut a = Args { manifest: None, source: None, out: None, capture: None, report: None, candidates: vec![], fail_on_divergence: false };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--manifest" => a.manifest = Some(value()?.into()),
            "--source" => a.source = Some(value()?),
            "--out" => a.out = Some(value()?.into()),
            "--capture" => a.capture = Some(value()?.into()),
            "--report" => a.report = Some(value()?.into()),
            "--fail-on-divergence" => a.fail_on_divergence = true,
            "--candidate" => {
                let v = value()?;
                let (label, path) = v.split_once('=').ok_or("--candidate takes LABEL=WASM")?;
                a.candidates.push((label.into(), path.into()));
            }
            other => return Err(format!("unknown flag {other}\n{USAGE}")),
        }
    }
    Ok(a)
}

fn read_candidates(list: &[(String, PathBuf)]) -> Result<Vec<(String, Vec<u8>)>, String> {
    list.iter()
        .map(|(l, p)| fs::read(p).map(|w| (l.clone(), w)).map_err(|e| format!("read {}: {e}", p.display())))
        .collect()
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let result = match argv.first().map(String::as_str) {
        Some("capture") => parse(&argv[1..]).and_then(|a| {
            let manifest = Manifest::load(&a.manifest.ok_or("--manifest is required")?)?;
            let candidates = read_candidates(&a.candidates)?;
            capture::capture(
                &manifest,
                &a.source.ok_or("--source is required (any funded G... account; nothing is signed)")?,
                &candidates,
                &a.out.ok_or("--out is required")?,
            )
        }),
        Some("replay") => parse(&argv[1..]).and_then(replay_cmd),
        Some("render") => parse(&argv[1..]).and_then(|a| {
            let path = a.report.ok_or("--report is required")?;
            let report: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?)
                .map_err(|e| format!("parse {}: {e}", path.display()))?;
            let out = a.out.ok_or("--out is required")?;
            fs::write(&out, render::render(&report)).map_err(|e| format!("write {}: {e}", out.display()))?;
            println!("HTML report written to {}", out.display());
            Ok(())
        }),
        _ => Err(USAGE.into()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn replay_cmd(a: Args) -> Result<(), String> {
    let fail_on_divergence = a.fail_on_divergence;
    let manifest = Manifest::load(a.manifest.as_ref().ok_or("--manifest is required")?)?;
    let dir = a.capture.ok_or("--capture is required")?;
    let out = a.out.ok_or("--out is required")?;
    if a.candidates.is_empty() {
        return Err("at least one --candidate is required".into());
    }
    let provenance: Value = serde_json::from_slice(
        &fs::read(dir.join("provenance.json")).map_err(|e| format!("read provenance: {e}"))?,
    )
    .map_err(|e| format!("parse provenance: {e}"))?;
    let snapshot_bytes = fs::read(dir.join("snapshot.json")).map_err(|e| format!("read snapshot: {e}"))?;
    if replay::sha256_hex(&snapshot_bytes) != provenance["snapshot_sha256"].as_str().unwrap_or("") {
        return Err("snapshot.json does not match the SHA-256 recorded in provenance.json".into());
    }
    if provenance["contract"].as_str() != Some(manifest.contract.as_str()) {
        return Err("manifest contract differs from the captured contract".into());
    }
    let snap = LedgerSnapshot::read_file(dir.join("snapshot.json")).map_err(|e| format!("load snapshot: {e}"))?;
    let verified_absent: BTreeSet<String> = provenance["keys_verified_absent_xdr"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let contract = soroban_sdk::xdr::ScAddress::from_str_checked(&manifest.contract)?;

    let baseline = replay::run(snap.clone(), &manifest, "baseline (deployed)")?;
    let mut candidates = Vec::new();
    for (label, wasm) in read_candidates(&a.candidates)? {
        candidates.push(replay::run(replay::with_candidate(&snap, &contract, &wasm)?, &manifest, &label)?);
    }

    let uncaptured = |b: &replay::Branch| -> Vec<String> {
        b.misses.iter().filter(|(id, _)| !verified_absent.contains(*id)).map(|(_, k)| fmt::ledger_key(k)).collect()
    };
    // Keys a branch read that are verified absent on-chain, minus those the baseline also read
    // (auth-mock nonces and the like). For a broken candidate this is usually the "why".
    let baseline_absent: BTreeSet<&String> = baseline.misses.keys().filter(|id| verified_absent.contains(*id)).collect();
    let absent_only = |b: &replay::Branch| -> Vec<String> {
        b.misses
            .iter()
            .filter(|(id, _)| verified_absent.contains(*id) && !baseline_absent.contains(id))
            .map(|(_, k)| fmt::ledger_key(k))
            .collect()
    };
    let steps_json = |b: &replay::Branch| -> Vec<Value> {
        b.steps
            .iter()
            .map(|s| json!({"label": s.label, "call": s.call, "args": s.args, "result": s.result, "failed": s.failed}))
            .collect()
    };

    let expectation_failures: Vec<Value> = manifest
        .steps
        .iter()
        .zip(&baseline.steps)
        .filter_map(|(m, r)| {
            m.expect.as_ref().filter(|e| **e != r.result).map(|e| json!({"step": m.label, "expected": e, "baseline": r.result}))
        })
        .collect();

    let mut candidate_reports = Vec::new();
    let mut summary = Vec::new();
    for c in &candidates {
        let step_diffs: Vec<Value> = baseline
            .steps
            .iter()
            .zip(&c.steps)
            .enumerate()
            .filter(|(_, (b, x))| b.result != x.result)
            .map(|(i, (b, x))| json!({"step": i + 1, "label": b.label, "baseline": b.result, "candidate": x.result}))
            .collect();
        let keys: BTreeSet<&String> = baseline.final_state.keys().chain(c.final_state.keys()).collect();
        let state_diffs: Vec<Value> = keys
            .into_iter()
            .filter(|k| baseline.final_state.get(*k) != c.final_state.get(*k))
            .map(|k| json!({"key": k, "baseline": baseline.final_state.get(k), "candidate": c.final_state.get(k)}))
            .collect();
        let outside = uncaptured(c);
        let status = if !step_diffs.is_empty() || !state_diffs.is_empty() {
            "diverged"
        } else if !outside.is_empty() {
            "unverified: read state outside the capture"
        } else {
            "no observed difference"
        };
        summary.push(json!({"candidate": c.label, "status": status, "step_differences": step_diffs.len(), "state_differences": state_diffs.len()}));
        candidate_reports.push(json!({
            "label": c.label,
            "wasm_sha256": c.wasm_sha256,
            "status": status,
            "steps": steps_json(c),
            "step_differences": step_diffs,
            "state_differences": state_diffs,
            "reads_outside_capture": outside,
            "reads_absent_on_chain": absent_only(c),
        }));
    }

    let report = json!({
        "tool": concat!("rehearse ", env!("CARGO_PKG_VERSION")),
        "claim": "Observed differences for the listed calls on the captured state. Not a safety certification.",
        "manifest": manifest,
        "capture": {
            "network": provenance["network"],
            "contract": provenance["contract"],
            "ledger": provenance["ledger"],
            "protocol": provenance["protocol"],
            "ledger_close_time": provenance["ledger_close_time"],
            "deployed_wasm_sha256": provenance["deployed_wasm_sha256"],
            "snapshot_sha256": provenance["snapshot_sha256"],
            "keys_captured": provenance["keys_captured"],
            "keys_verified_absent": provenance["keys_verified_absent"],
        },
        "runtime": {
            "soroban_sdk": "28.0.0",
            "soroban_env_host": "28.0.2 with experimental `next` feature",
            "auth_mocked": true,
            "network_calls_during_replay": false,
        },
        "baseline": {
            "label": baseline.label,
            "wasm_sha256": baseline.wasm_sha256,
            "status": if expectation_failures.is_empty() { "ok" } else { "baseline expectation failed" },
            "expectation_failures": expectation_failures,
            "steps": steps_json(&baseline),
            "reads_outside_capture": uncaptured(&baseline),
        },
        "candidates": candidate_reports,
        "summary": summary,
    });
    capture::write_json(&out, &report)?;
    print_table(&baseline, &candidates);
    for s in &summary {
        println!("{}: {}", s["candidate"].as_str().unwrap_or(""), s["status"].as_str().unwrap_or(""));
    }
    println!("Report written to {}", out.display());
    // For CI: exit 2 when any candidate is not "no observed difference" (diverged, or read
    // state outside the capture). Exit 1 stays reserved for errors.
    if fail_on_divergence {
        let failing: Vec<&str> = summary
            .iter()
            .filter(|s| s["status"] != "no observed difference")
            .filter_map(|s| s["candidate"].as_str())
            .collect();
        if !failing.is_empty() {
            eprintln!("rehearse: --fail-on-divergence: {} did not match the deployed contract", failing.join(", "));
            std::process::exit(2);
        }
    }
    Ok(())
}

fn print_table(baseline: &replay::Branch, candidates: &[replay::Branch]) {
    let width = 28;
    let mut header = format!("{:<34}{:<width$}", "step", baseline.label);
    for c in candidates {
        header.push_str(&format!("{:<width$}", c.label));
    }
    println!("{header}");
    for (i, b) in baseline.steps.iter().enumerate() {
        let mut line = format!("{:<34}{:<width$}", truncate(&b.label, 32), truncate(&b.result, width - 2));
        for c in candidates {
            let r = &c.steps[i].result;
            let mark = if *r == b.result { "" } else { "≠ " };
            line.push_str(&format!("{:<width$}", truncate(&format!("{mark}{r}"), width - 2)));
        }
        println!("{line}");
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

trait FromStrChecked: Sized {
    fn from_str_checked(s: &str) -> Result<Self, String>;
}

impl FromStrChecked for soroban_sdk::xdr::ScAddress {
    fn from_str_checked(s: &str) -> Result<Self, String> {
        use std::str::FromStr;
        Self::from_str(s).map_err(|e| format!("bad contract address {s}: {e}"))
    }
}
