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
