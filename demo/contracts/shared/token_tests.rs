// Fresh-state test suite shared by every demo token build.
//
// Each build includes this file unchanged. All three pass it, including
// token-v2-broken: a fresh Env writes and reads through the same (renamed) key,
// so ordinary unit tests cannot see a regression that only appears against
// state written by the deployed v1.
extern crate std;

use super::{Token, TokenClient, TokenError};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

fn setup(e: &Env) -> (TokenClient<'_>, Address) {
    let admin = Address::generate(e);
    let id = e.register(
        Token,
        (
            admin.clone(),
            7u32,
            String::from_str(e, "Rehearse Demo Token"),
            String::from_str(e, "RHD"),
        ),
    );
    (TokenClient::new(e, &id), admin)
}

#[test]
fn metadata_is_set_by_constructor() {
    let e = Env::default();
    let (token, _) = setup(&e);
    assert_eq!(token.decimals(), 7);
    assert_eq!(token.name(), String::from_str(&e, "Rehearse Demo Token"));
    assert_eq!(token.symbol(), String::from_str(&e, "RHD"));
}

#[test]
fn mint_credits_holder_and_requires_admin() {
    let e = Env::default();
    e.mock_all_auths();
    let (token, admin) = setup(&e);
    let holder = Address::generate(&e);
    token.mint(&holder, &12_400_000_000);
    assert_eq!(e.auths()[0].0, admin);
    assert_eq!(token.balance(&holder), 12_400_000_000);
}

#[test]
fn transfer_moves_balance_and_requires_sender() {
    let e = Env::default();
    e.mock_all_auths();
    let (token, _) = setup(&e);
    let (a, b) = (Address::generate(&e), Address::generate(&e));
    token.mint(&a, &1_000_0000000);
    token.transfer(&a, &b, &100_0000000);
    assert_eq!(e.auths()[0].0, a);
    assert_eq!(token.balance(&a), 900_0000000);
    assert_eq!(token.balance(&b), 100_0000000);
}

#[test]
fn unknown_holder_has_zero_balance() {
    let e = Env::default();
    let (token, _) = setup(&e);
    assert_eq!(token.balance(&Address::generate(&e)), 0);
}

#[test]
fn transfer_over_balance_fails() {
    let e = Env::default();
    e.mock_all_auths();
    let (token, _) = setup(&e);
    let (a, b) = (Address::generate(&e), Address::generate(&e));
    token.mint(&a, &10);
    assert_eq!(token.try_transfer(&a, &b, &11), Err(Ok(TokenError::InsufficientBalance.into())));
    assert_eq!(token.balance(&a), 10);
}

#[test]
fn non_positive_amounts_are_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (token, _) = setup(&e);
    let (a, b) = (Address::generate(&e), Address::generate(&e));
    assert_eq!(token.try_mint(&a, &0), Err(Ok(TokenError::InvalidAmount.into())));
    assert_eq!(token.try_transfer(&a, &b, &-1), Err(Ok(TokenError::InvalidAmount.into())));
}

#[test]
#[should_panic]
fn mint_without_admin_auth_fails() {
    let e = Env::default();
    let (token, _) = setup(&e);
    token.mint(&Address::generate(&e), &10);
}

#[test]
#[should_panic]
fn upgrade_without_admin_auth_fails() {
    let e = Env::default();
    let (token, _) = setup(&e);
    token.upgrade(&BytesN::from_array(&e, &[0; 32]));
}
