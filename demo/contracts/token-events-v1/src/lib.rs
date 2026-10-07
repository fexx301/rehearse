#![no_std]
//! Events token v1: the demo token, plus a SEP-41-style `transfer` event.
//!
//! Used to show event comparison: the demo token itself emits no events.
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contractmeta, contracttype, panic_with_error, Address,
    BytesN, Env, String,
};

contractmeta!(key = "version", val = "1.0.0");

// On each write, extend TTLs to about 30 days (5 s ledgers). The threshold sits just below the
// target so every write extends, keeping the seeded testnet state alive.
const TTL_THRESHOLD: u32 = 518_000;
const TTL_EXTEND_TO: u32 = 518_400;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TokenError {
    InvalidAmount = 1,
    InsufficientBalance = 2,
}

#[contracttype]
enum DataKey {
    Admin,
    Decimals,
    Name,
    Symbol,
    Balance(Address),
}

#[contractevent]
pub struct Transfer {
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contract]
pub struct Token;

#[contractimpl]
impl Token {
    pub fn __constructor(e: Env, admin: Address, decimals: u32, name: String, symbol: String) {
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Decimals, &decimals);
        e.storage().instance().set(&DataKey::Name, &name);
        e.storage().instance().set(&DataKey::Symbol, &symbol);
        e.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    pub fn mint(e: Env, to: Address, amount: i128) {
        let admin: Address = e.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if amount <= 0 {
            panic_with_error!(&e, TokenError::InvalidAmount);
        }
        let key = DataKey::Balance(to);
        let balance: i128 = e.storage().persistent().get(&key).unwrap_or(0);
        e.storage().persistent().set(&key, &(balance + amount));
        e.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        e.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        e.storage().persistent().get(&DataKey::Balance(id)).unwrap_or(0)
    }

    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&e, TokenError::InvalidAmount);
        }
        let from_key = DataKey::Balance(from.clone());
        let from_balance: i128 = e.storage().persistent().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic_with_error!(&e, TokenError::InsufficientBalance);
        }
        e.storage().persistent().set(&from_key, &(from_balance - amount));
        e.storage().persistent().extend_ttl(&from_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        let to_key = DataKey::Balance(to.clone());
        let to_balance: i128 = e.storage().persistent().get(&to_key).unwrap_or(0);
        e.storage().persistent().set(&to_key, &(to_balance + amount));
        e.storage().persistent().extend_ttl(&to_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        Transfer { from: from.clone(), to: to.clone(), amount }.publish(&e);
    }

    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    pub fn name(e: Env) -> String {
        e.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(e: Env) -> String {
        e.storage().instance().get(&DataKey::Symbol).unwrap()
    }

    pub fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = e.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        #[allow(deprecated)] // update_current_contract_wasm is deprecated in SDK 28; kept so the deployed bytes stay the same
        e.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
#[path = "../../shared/token_tests.rs"]
mod test;
