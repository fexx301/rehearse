//! Runs the built binary: help and version, and the exit code of a failed upgrade.
use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rehearse"))
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

#[test]
fn help_and_version_exit_zero() {
    for args in [vec!["--help"], vec!["-h"], vec!["replay", "--help"]] {
        let out = bin().args(&args).output().unwrap();
        assert!(out.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("usage:"), "{args:?}");
    }
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), concat!("rehearse ", env!("CARGO_PKG_VERSION")));
    assert_eq!(bin().output().unwrap().status.code(), Some(1), "no arguments is an error");
}

fn replay_with_upgrade(name: &str, edit: impl Fn(&mut serde_json::Value)) -> (Option<i32>, serde_json::Value) {
    let dir = std::env::temp_dir().join(format!("rehearse-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(repo("examples/upgrade-path/manifest.json")).unwrap()).unwrap();
    edit(&mut m);
    let manifest = dir.join("manifest.json");
    std::fs::write(&manifest, serde_json::to_vec(&m).unwrap()).unwrap();
    let report = dir.join("report.json");
    let out = bin()
        .args(["replay", "--manifest"]).arg(&manifest)
        .arg("--capture").arg(repo("examples/upgrade-path/capture"))
        .arg("--candidate").arg(format!("v2-compatible={}", repo("demo/wasm/token-v2-compatible.wasm").display()))
        .arg("--out").arg(&report)
        .arg("--fail-on-divergence")
        .output()
        .unwrap();
    let r: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    (out.status.code(), r)
}

#[test]
fn a_failed_upgrade_fails_the_check() {
    let (code, r) = replay_with_upgrade("upgrade", |m| m["upgrade"]["call"] = "no_such_function".into());
    assert_eq!(code, Some(2));
    assert_eq!(r["candidates"][0]["status"], "upgrade failed");
    assert_eq!(r["candidates"][0]["installed_via_upgrade"]["candidate_code_installed"], false);
}

#[test]
fn a_failed_migration_fails_the_check() {
    let (code, r) = replay_with_upgrade("migrate", |m| {
        m["upgrade"]["migrate"] = serde_json::json!([{"label": "migrate", "call": "no_such_migration", "args": []}]);
    });
    assert_eq!(code, Some(2));
    assert_eq!(r["candidates"][0]["status"], "upgrade failed");
    assert_eq!(r["candidates"][0]["installed_via_upgrade"]["migrate"][0]["failed"], true);
}

#[test]
fn a_clean_upgrade_passes_the_check() {
    let (code, r) = replay_with_upgrade("clean", |_| {});
    assert_eq!(code, Some(0));
    assert_eq!(r["candidates"][0]["status"], "no observed difference");
}

fn replay_signed(candidate: &str) -> (Option<i32>, serde_json::Value) {
    let dir = std::env::temp_dir().join(format!("rehearse-cli-{}-signed-{candidate}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let report = dir.join("report.json");
    let out = bin()
        .args(["replay", "--manifest"]).arg(repo("examples/auth-scope/manifest.json"))
        .arg("--capture").arg(repo("examples/auth-scope/capture"))
        .arg("--candidate").arg(format!("{candidate}={}", repo(&format!("demo/wasm/token-{candidate}.wasm")).display()))
        .arg("--out").arg(&report)
        .args(["--check-signatures", "--fail-on-divergence"])
        .output()
        .unwrap();
    let r: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    (out.status.code(), r)
}

#[test]
fn signatures_for_the_deployed_contract_are_rejected_by_a_narrower_candidate() {
    let (code, r) = replay_signed("v2-authscope");
    assert_eq!(code, Some(2));
    assert_eq!(r["signature_check"]["deployed_contract"], "accepted its own signed requests");
    assert_eq!(r["candidates"][0]["signature_differences"][0]["finding"], "rejected");
}

#[test]
fn a_candidate_that_drops_auth_passes_signing_but_fails_the_requirement_comparison() {
    let (code, r) = replay_signed("v2-noauth");
    assert_eq!(code, Some(2));
    assert!(r["candidates"][0].get("signature_differences").is_none(), "nothing asks for the signatures");
    assert_eq!(r["candidates"][0]["auth_differences"][0]["candidate"], serde_json::json!([]));
}

#[test]
fn a_compatible_candidate_passes_with_signatures_checked() {
    let (code, r) = replay_signed("v2-compatible");
    assert_eq!(code, Some(0));
    assert_eq!(r["candidates"][0]["status"], "no observed difference");
}

fn replay_error(manifest: &std::path::Path, capture: &std::path::Path) -> (Option<i32>, String) {
    let out = bin()
        .args(["replay", "--manifest"]).arg(manifest)
        .arg("--capture").arg(capture)
        .arg("--candidate").arg(format!("v2={}", repo("demo/wasm/token-v2-compatible.wasm").display()))
        .arg("--out").arg(std::env::temp_dir().join(format!("rehearse-cli-{}-err.json", std::process::id())))
        .output()
        .unwrap();
    (out.status.code(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_missing_capture_says_how_to_make_one() {
    let (code, err) = replay_error(&repo("demo/manifest.json"), &repo("no-such-capture"));
    assert_eq!(code, Some(1));
    assert!(err.contains("run `rehearse capture --out"), "{err}");
    let (code, err) = replay_error(&repo("demo/manifest.json"), &repo("demo"));
    assert_eq!(code, Some(1));
    assert!(err.contains("is not a capture"), "{err}");
}

#[test]
fn a_misspelled_call_is_an_error_not_a_clean_report() {
    let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(repo("demo/manifest.json")).unwrap()).unwrap();
    m["steps"][0]["call"] = "balanse".into();
    let path = std::env::temp_dir().join(format!("rehearse-cli-{}-typo.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
    let (code, err) = replay_error(&path, &repo("demo/capture"));
    assert_eq!(code, Some(1));
    assert!(err.contains("calls `balanse`, which the deployed contract doesn't export"), "{err}");
    assert!(err.contains("balance"), "{err}");
}

#[test]
fn a_manifest_for_another_contract_names_both() {
    let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(repo("demo/manifest.json")).unwrap()).unwrap();
    let other: serde_json::Value = serde_json::from_slice(&std::fs::read(repo("examples/events/manifest.json")).unwrap()).unwrap();
    m["contract"] = other["contract"].clone();
    let path = std::env::temp_dir().join(format!("rehearse-cli-{}-other.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
    let (code, err) = replay_error(&path, &repo("demo/capture"));
    assert_eq!(code, Some(1));
    assert!(err.contains(other["contract"].as_str().unwrap()) && err.contains("capture again"), "{err}");
}

#[test]
fn signatures_are_checked_through_the_upgrade_path() {
    for (candidate, code, finding) in [("v2-compatible", 0, None), ("v2-authscope", 2, Some("rejected"))] {
        let report = std::env::temp_dir().join(format!("rehearse-cli-{}-upsig-{candidate}.json", std::process::id()));
        let out = bin()
            .args(["replay", "--manifest"]).arg(repo("examples/upgrade-path/manifest.json"))
            .arg("--capture").arg(repo("examples/upgrade-path/capture"))
            .arg("--candidate").arg(format!("{candidate}={}", repo(&format!("demo/wasm/token-{candidate}.wasm")).display()))
            .arg("--out").arg(&report)
            .args(["--check-signatures", "--fail-on-divergence"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(code), "{candidate}");
        let r: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        assert_eq!(r["signature_check"]["deployed_contract"], "accepted its own signed requests");
        assert_eq!(r["candidates"][0]["installed_via_upgrade"]["candidate_code_installed"], true);
        assert_eq!(r["candidates"][0]["signature_differences"][0]["finding"].as_str(), finding, "{candidate}");
    }
}
