#![no_std]
//! Demo token v2 (no auth): a deliberately insecure candidate.
//!
//! Identical to v1 except `transfer` no longer calls `from.require_auth()`, so anyone can
//! move anyone's balance. It exists to show why Rehearse compares *required* authorizations
//! as well as checking signatures: signatures made for v1 are still accepted here (nothing
//! asks for them), but the required-authorization comparison flags the missing check.
//! It does not use the shared test suite, which would fail; its own test shows the hole.
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
        if amount <= 0 {
            panic_with_error!(&e, TokenError::InvalidAmount);
        }
        let from_key = DataKey::Balance(from);
        let from_balance: i128 = e.storage().persistent().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic_with_error!(&e, TokenError::InsufficientBalance);
        }
        e.storage().persistent().set(&from_key, &(from_balance - amount));
        e.storage().persistent().extend_ttl(&from_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        let to_key = DataKey::Balance(to);
        let to_balance: i128 = e.storage().persistent().get(&to_key).unwrap_or(0);
        e.storage().persistent().set(&to_key, &(to_balance + amount));
        e.storage().persistent().extend_ttl(&to_key, TTL_THRESHOLD, TTL_EXTEND_TO);
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
mod test {
    extern crate std;
    use super::{Token, TokenClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    #[test]
    fn anyone_can_move_anyones_balance() {
        let e = Env::default();
        let admin = Address::generate(&e);
        let id = e.register(Token, (admin.clone(), 7u32, String::from_str(&e, "T"), String::from_str(&e, "T")));
        let token = TokenClient::new(&e, &id);
        let (a, b) = (Address::generate(&e), Address::generate(&e));
        e.mock_all_auths();
        token.mint(&a, &100);
        e.set_auths(&[]); // no authorizations at all
        token.transfer(&a, &b, &60);
        assert_eq!(token.balance(&b), 60);
    }
}
