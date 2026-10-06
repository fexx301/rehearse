// G1: can soroban-fork 0.9.5 execute the Rehearse fixture (built with soroban-sdk 28)
// against live protocol-29 testnet state? Read-only: forks testnet, never submits.
use soroban_fork::ForkConfig;
use soroban_sdk::{vec, Address, String as SorobanString, Symbol};

const RPC: &str = "https://soroban-testnet.stellar.org:443";
const FIXTURE: &str = "CAZVNRBBQAUZI4KZFYRKQUGI53PPDEHCUGSDWQ5G4HHAQ6CFXSMPGILB";
// Native XLM Stellar Asset Contract on testnet: built into the host, so it runs on any host version.
const XLM_SAC: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

fn call(env: &soroban_sdk::Env, id: &str, func: &str) {
    let addr = Address::from_string(&SorobanString::from_str(env, id));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.try_invoke_contract::<soroban_sdk::Val, soroban_sdk::Error>(
            &addr,
            &Symbol::new(env, func),
            vec![env],
        )
    }));
    match r {
        Ok(Ok(Ok(v))) => println!("RESULT {id}.{func}() -> ok {:?}", v),
        Ok(other) => println!("RESULT {id}.{func}() -> error {:?}", other),
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic".into());
            println!("RESULT {id}.{func}() -> panic {msg}");
        }
    }
}

fn main() {
    let env = ForkConfig::new(RPC).build().expect("fork setup");
    println!(
        "FORK ledger={} reported_protocol={} host_max={}",
        env.ledger_sequence(),
        env.protocol_version(),
        soroban_env_host::meta::INTERFACE_VERSION.protocol
    );
    env.host().set_diagnostic_level(soroban_env_host::DiagnosticLevel::Debug).unwrap();
    call(&env, XLM_SAC, "decimals");
    call(&env, FIXTURE, "balance");
    let addr = Address::from_string(&SorobanString::from_str(&env, FIXTURE));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.invoke_contract::<i128>(&addr, &Symbol::new(&env, "balance"), vec![&env])
    }));
    if let Err(p) = r {
        let msg = p.downcast_ref::<String>().cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "non-string panic".into());
        println!("PANIC_DETAIL {msg}");
    }
}
