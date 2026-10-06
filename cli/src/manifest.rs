//! The workflow manifest: which contract, which calls, in which order.
use serde::{Deserialize, Serialize};
use soroban_sdk::xdr::{Int128Parts, ScAddress, ScString, ScVal, StringM};
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
    U32(u32),
    String(String),
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
            Arg::U32(v) => ScVal::U32(*v),
            Arg::String(s) => ScVal::String(ScString(
                StringM::try_from(s.as_bytes().to_vec()).map_err(|e| format!("bad string: {e}"))?,
            )),
            Arg::Bool(b) => ScVal::Bool(*b),
        })
    }

    pub fn display(&self) -> String {
        match self {
            Arg::Address(s) | Arg::I128(s) => s.clone(),
            Arg::U32(v) => v.to_string(),
            Arg::String(s) => format!("{s:?}"),
            Arg::Bool(b) => b.to_string(),
        }
    }
}
