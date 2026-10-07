//! The workflow manifest: which contract, which calls, in which order.
use serde::{Deserialize, Serialize};
use soroban_sdk::xdr::{BytesM, Int128Parts, ScAddress, ScBytes, ScString, ScSymbol, ScVal, StringM};
use std::{fs, path::Path, str::FromStr};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub description: String,
    pub network: String,
    pub rpc: String,
    pub contract: String,
    /// Optional formatting hints for reports: token amounts in whole units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Display>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Display {
    pub decimals: u32,
    pub symbol: String,
    /// Calls whose integer results are token amounts.
    pub amount_calls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub label: String,
    pub call: String,
    #[serde(default)]
    pub args: Vec<Arg>,
    /// Optional expected baseline result, compared as the formatted value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arg {
    Address(String),
    I128(String),
    /// 64-bit integers are written as strings so JSON never rounds them.
    U64(String),
    I64(String),
    U32(u32),
    String(String),
    Symbol(String),
    /// Hex, with or without a 0x prefix.
    Bytes(String),
    Bool(bool),
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let m: Manifest =
            serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
        if m.steps.is_empty() {
            return Err("manifest has no steps".into());
        }
        for step in &m.steps {
            for arg in &step.args {
                arg.to_scval()?;
            }
        }
        Ok(m)
    }
}

impl Arg {
    pub fn to_scval(&self) -> Result<ScVal, String> {
        Ok(match self {
            Arg::Address(s) => ScVal::Address(
                ScAddress::from_str(s).map_err(|e| format!("bad address {s}: {e}"))?,
            ),
            Arg::I128(s) => {
                let v: i128 = s.parse().map_err(|e| format!("bad i128 {s}: {e}"))?;
                ScVal::I128(Int128Parts { hi: (v >> 64) as i64, lo: v as u64 })
            }
            Arg::U64(s) => ScVal::U64(s.parse().map_err(|e| format!("bad u64 {s}: {e}"))?),
            Arg::I64(s) => ScVal::I64(s.parse().map_err(|e| format!("bad i64 {s}: {e}"))?),
            Arg::U32(v) => ScVal::U32(*v),
            Arg::String(s) => ScVal::String(ScString(
                StringM::try_from(s.as_bytes().to_vec()).map_err(|e| format!("bad string: {e}"))?,
            )),
            Arg::Symbol(s) => ScVal::Symbol(ScSymbol(
                StringM::try_from(s.as_bytes().to_vec()).map_err(|e| format!("bad symbol: {e}"))?,
            )),
            Arg::Bytes(s) => ScVal::Bytes(ScBytes(
                BytesM::try_from(decode_hex(s)?).map_err(|e| format!("bad bytes: {e}"))?,
            )),
            Arg::Bool(b) => ScVal::Bool(*b),
        })
    }

    pub fn display(&self) -> String {
        match self {
            Arg::Address(s) | Arg::I128(s) | Arg::U64(s) | Arg::I64(s) | Arg::Symbol(s) => s.clone(),
            Arg::Bytes(s) => format!("0x{}", s.trim_start_matches("0x")),
            Arg::U32(v) => v.to_string(),
            Arg::String(s) => format!("{s:?}"),
            Arg::Bool(b) => b.to_string(),
        }
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let h = s.trim_start_matches("0x");
    if h.len() % 2 != 0 {
        return Err(format!("bad bytes {s}: odd number of hex digits"));
    }
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).map_err(|e| format!("bad bytes {s}: {e}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt;

    fn arg(json: &str) -> Arg {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn every_argument_type_parses_and_converts() {
        let cases = [
            (r#"{"address": "GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3"}"#, "GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3"),
            (r#"{"i128": "-170141183460469231731687303715884105728"}"#, "-170141183460469231731687303715884105728"),
            (r#"{"u64": "18446744073709551615"}"#, "18446744073709551615"),
            (r#"{"i64": "-9223372036854775808"}"#, "-9223372036854775808"),
            (r#"{"u32": 7}"#, "7"),
            (r#"{"string": "hi"}"#, "\"hi\""),
            (r#"{"symbol": "Balance"}"#, "Balance"),
            (r#"{"bool": true}"#, "true"),
        ];
        for (json, shown) in cases {
            let v = arg(json).to_scval().unwrap_or_else(|e| panic!("{json}: {e}"));
            assert_eq!(fmt::scval(&v), shown, "{json}");
        }
    }

    #[test]
    fn bytes_accept_hex_with_or_without_prefix() {
        for json in [r#"{"bytes": "0x00ff10"}"#, r#"{"bytes": "00FF10"}"#] {
            match arg(json).to_scval().unwrap() {
                ScVal::Bytes(b) => assert_eq!(b.0.to_vec(), vec![0x00, 0xff, 0x10]),
                other => panic!("{other:?}"),
            }
        }
        assert!(arg(r#"{"bytes": "abc"}"#).to_scval().is_err());
        assert!(arg(r#"{"bytes": "zz"}"#).to_scval().is_err());
    }

    #[test]
    fn bad_values_are_rejected_with_the_value_named() {
        let e = arg(r#"{"u64": "-1"}"#).to_scval().unwrap_err();
        assert!(e.contains("-1"), "{e}");
        assert!(arg(r#"{"address": "not-an-address"}"#).to_scval().is_err());
        assert!(arg(r#"{"i128": "1.5"}"#).to_scval().is_err());
    }

    #[test]
    fn a_manifest_without_steps_is_refused() {
        let dir = std::env::temp_dir().join(format!("rehearse-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("m.json");
        std::fs::write(&p, r#"{"description":"x","network":"testnet","rpc":"http://x","contract":"C","steps":[]}"#).unwrap();
        assert_eq!(Manifest::load(&p).unwrap_err(), "manifest has no steps");
    }
}
