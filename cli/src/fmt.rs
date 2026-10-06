//! Human-readable rendering of XDR values and ledger keys for reports.
use soroban_sdk::xdr::{ContractDataDurability, LedgerKey, ScVal};

pub fn scval(v: &ScVal) -> String {
    match v {
        ScVal::Bool(b) => b.to_string(),
        ScVal::Void => "()".into(),
        ScVal::U32(n) => n.to_string(),
        ScVal::I32(n) => n.to_string(),
        ScVal::U64(n) => n.to_string(),
        ScVal::I64(n) => n.to_string(),
        ScVal::U128(p) => (((p.hi as u128) << 64) | p.lo as u128).to_string(),
        ScVal::I128(p) => (((p.hi as i128) << 64) | p.lo as i128).to_string(),
        ScVal::Symbol(s) => s.0.to_utf8_string_lossy(),
        ScVal::String(s) => format!("{:?}", s.0.to_utf8_string_lossy()),
        ScVal::Address(a) => a.to_string(),
        ScVal::Vec(Some(items)) => match items.first() {
            // Enum-style keys such as DataKey::Balance(addr) encode as [Symbol, ..fields].
            Some(ScVal::Symbol(head)) => {
                let rest: Vec<String> = items.iter().skip(1).map(scval).collect();
                if rest.is_empty() {
                    head.0.to_utf8_string_lossy()
                } else {
                    format!("{}({})", head.0.to_utf8_string_lossy(), rest.join(", "))
                }
            }
            _ => format!("[{}]", items.iter().map(scval).collect::<Vec<_>>().join(", ")),
        },
        ScVal::Map(Some(m)) => format!(
            "{{{}}}",
            m.iter()
                .map(|e| format!("{}: {}", scval(&e.key), scval(&e.val)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ScVal::LedgerKeyContractInstance => "instance".into(),
        other => format!("{other:?}"),
    }
}

pub fn ledger_key(k: &LedgerKey) -> String {
    match k {
        LedgerKey::ContractData(d) => {
            let durability = match d.durability {
                ContractDataDurability::Persistent => "persistent",
                ContractDataDurability::Temporary => "temporary",
            };
            format!("{} {} {}", d.contract, durability, scval(&d.key))
        }
        LedgerKey::ContractCode(c) => format!("code {}", hex(&c.hash.0)),
        LedgerKey::Account(a) => format!("account {}", account_strkey(&a.account_id)),
        LedgerKey::Trustline(t) => format!("trustline {}", account_strkey(&t.account_id)),
        other => format!("{other:?}"),
    }
}

fn account_strkey(id: &soroban_sdk::xdr::AccountId) -> String {
    soroban_sdk::xdr::ScAddress::Account(id.clone()).to_string()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
