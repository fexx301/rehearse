#![no_std]
//! Demo token v2 (compatible): a candidate that refactors without changing behavior.
//!
//! Same storage keys and semantics as v1; balance reads and writes move into
//! helpers. Rehearse should report no divergence for this build.
use soroban_sdk::{
    contract, contracterror, contractimpl, contractmeta, contracttype, panic_with_error, Address,
    BytesN, Env, String,
};

contractmeta!(key = "version", val = "2.0.0");

// On each write, extend TTLs to about 30 days (5 s ledgers) once they fall below the threshold.
const TTL_THRESHOLD: u32 = 100_000;
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

fn read_balance(e: &Env, holder: Address) -> i128 {
    e.storage().persistent().get(&DataKey::Balance(holder)).unwrap_or(0)
}

fn write_balance(e: &Env, holder: Address, amount: i128) {
    let key = DataKey::Balance(holder);
    e.storage().persistent().set(&key, &amount);
    e.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
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
        let balance = read_balance(&e, to.clone());
        write_balance(&e, to, balance + amount);
        e.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        read_balance(&e, id)
    }

    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&e, TokenError::InvalidAmount);
        }
        let from_balance = read_balance(&e, from.clone());
        if from_balance < amount {
            panic_with_error!(&e, TokenError::InsufficientBalance);
        }
        write_balance(&e, from, from_balance - amount);
        let to_balance = read_balance(&e, to.clone());
        write_balance(&e, to, to_balance + amount);
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
