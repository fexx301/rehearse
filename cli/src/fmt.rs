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

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::xdr::{Int128Parts, ScSymbol, ScVec, StringM};
    use std::str::FromStr;

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(StringM::try_from(s.as_bytes().to_vec()).unwrap()))
    }

    #[test]
    fn i128_extremes_render_exactly() {
        let max = ScVal::I128(Int128Parts { hi: i64::MAX, lo: u64::MAX });
        let min = ScVal::I128(Int128Parts { hi: i64::MIN, lo: 0 });
        assert_eq!(scval(&max), "170141183460469231731687303715884105727");
        assert_eq!(scval(&min), "-170141183460469231731687303715884105728");
    }

    #[test]
    fn enum_style_keys_read_like_rust() {
        let holder = soroban_sdk::xdr::ScAddress::from_str("GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3").unwrap();
        let key = ScVal::Vec(Some(ScVec(vec![sym("Balance"), ScVal::Address(holder)].try_into().unwrap())));
        assert_eq!(scval(&key), "Balance(GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3)");
        let unit = ScVal::Vec(Some(ScVec(vec![sym("Admin")].try_into().unwrap())));
        assert_eq!(scval(&unit), "Admin");
    }
}
