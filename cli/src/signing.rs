//! `replay --check-signatures`: replay with real signature checking, using test signers.
//!
//! Replay holds nobody's keys, so in a copy of the snapshot each ordinary account that
//! has to authorize something gets a signer key derived here, and the deployed contract's
//! own authorization requests are signed with it, the way a wallet signs what simulation
//! returns. Candidates then run with checking on, against those same signed entries.
use ed25519_dalek::{Signer as _, SigningKey};
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::xdr::{
    AccountEntry, AccountEntryExt, AccountId, BytesM, Hash, HashIdPreimage, HashIdPreimageSorobanAuthorization,
    LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, LedgerKeyAccount, Limits, ScAddress,
    ScBytes, ScMap, ScMapEntry, ScSymbol, ScVal, ScVec, SequenceNumber, Signer, SignerKey,
    SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanAuthorizedInvocation, SorobanCredentials,
    String32, Thresholds, Uint256, WriteXdr,
};
use std::collections::BTreeSet;

/// One authorization a call required: who, and the exact invocation tree they approve.
pub type Payload = (ScAddress, SorobanAuthorizedInvocation);

/// How far ahead the signed entries expire, in ledgers.
const EXPIRES_AFTER: u32 = 1000;

/// The deterministic test key standing in for `account`'s real key.
fn test_key(account: &ScAddress) -> SigningKey {
    let seed: [u8; 32] = Sha256::digest(format!("rehearse test signer for {account}").as_bytes()).into();
    SigningKey::from_bytes(&seed)
}

fn sym(s: &str) -> ScVal {
    ScVal::Symbol(ScSymbol(s.as_bytes().to_vec().try_into().unwrap()))
}

/// Ordinary (G…) accounts only. Contract accounts need their own signature scheme.
pub fn all_plain_accounts(payloads: &[Payload]) -> bool {
    payloads.iter().all(|(a, _)| matches!(a, ScAddress::Account(_)))
}

/// A nonce no two signed entries share: derived from the call index and the signer.
fn nonce(step: usize, idx: usize, account: &ScAddress) -> i64 {
    let h = Sha256::digest(format!("{step}/{idx}/{account}").as_bytes());
    i64::from_be_bytes(h[..8].try_into().unwrap()) & i64::MAX
}

/// Signed entries for one call's payloads, as a wallet would produce them.
pub fn sign(snap: &LedgerSnapshot, step: usize, payloads: &[Payload]) -> Vec<SorobanAuthorizationEntry> {
    let expiration = snap.sequence_number + EXPIRES_AFTER;
    payloads
        .iter()
        .enumerate()
        .map(|(idx, (address, invocation))| {
            let nonce = nonce(step, idx, address);
            let preimage = HashIdPreimage::SorobanAuthorization(HashIdPreimageSorobanAuthorization {
                network_id: Hash(snap.network_id),
                nonce,
                signature_expiration_ledger: expiration,
                invocation: invocation.clone(),
            });
            let payload = Sha256::digest(preimage.to_xdr(Limits::none()).expect("preimage encodes"));
            let key = test_key(address);
            let signature = key.sign(&payload);
            let bytes = |b: &[u8]| ScVal::Bytes(ScBytes(BytesM::try_from(b.to_vec()).unwrap()));
            let sig = ScVal::Map(Some(ScMap(
                vec![
                    ScMapEntry { key: sym("public_key"), val: bytes(key.verifying_key().as_bytes()) },
                    ScMapEntry { key: sym("signature"), val: bytes(&signature.to_bytes()) },
                ]
                .try_into()
                .unwrap(),
            )));
            SorobanAuthorizationEntry {
                credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                    address: address.clone(),
                    nonce,
                    signature_expiration_ledger: expiration,
                    signature: ScVal::Vec(Some(ScVec(vec![sig].try_into().unwrap()))),
                }),
                root_invocation: invocation.clone(),
            }
        })
        .collect()
}

/// A copy of `snap` in which every ordinary account in `payloads` accepts its test key at
/// the medium threshold. Accounts missing on-chain are created; existing ones keep their
/// balance and settings but their signers are replaced. Returns the account keys touched.
pub fn with_test_signers(snap: &LedgerSnapshot, payloads: &[Vec<Payload>]) -> (LedgerSnapshot, BTreeSet<LedgerKey>) {
    let mut out = snap.clone();
    let mut touched = BTreeSet::new();
    let accounts: BTreeSet<AccountId> = payloads
        .iter()
        .flatten()
        .filter_map(|(a, _)| match a {
            ScAddress::Account(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    for id in accounts {
        let key = LedgerKey::Account(LedgerKeyAccount { account_id: id.clone() });
        let addr = ScAddress::Account(id.clone());
        let signer = Signer {
            key: SignerKey::Ed25519(Uint256(test_key(&addr).verifying_key().to_bytes())),
            weight: 1,
        };
        let existing = out.ledger_entries.iter().find_map(|(k, (e, _))| match &e.data {
            LedgerEntryData::Account(a) if **k == key => Some(a.clone()),
            _ => None,
        });
        let mut account = existing.unwrap_or(AccountEntry {
            account_id: id.clone(),
            balance: 0,
            seq_num: SequenceNumber(0),
            num_sub_entries: 0,
            inflation_dest: None,
            flags: 0,
            home_domain: String32::default(),
            thresholds: Thresholds([1, 0, 0, 0]),
            signers: Default::default(),
            ext: AccountEntryExt::V0,
        });
        // Master key weight 0 (we don't hold it); the test signer meets low/medium/high.
        account.thresholds = Thresholds([0, 1, 1, 1]);
        account.signers = vec![signer].try_into().unwrap();
        account.num_sub_entries = 1;
        let entry = LedgerEntry { last_modified_ledger_seq: out.sequence_number, data: LedgerEntryData::Account(account), ext: LedgerEntryExt::V0 };
        out.ledger_entries.retain(|(k, _)| **k != key);
        out.ledger_entries.push((Box::new(key.clone()), (Box::new(entry), None)));
        touched.insert(key);
    }
    (out, touched)
}
