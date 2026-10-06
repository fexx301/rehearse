//! Minimal Soroban JSON-RPC client. Used only by `capture`; replay never touches the network.
use serde_json::{json, Value};

pub fn call(rpc: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let resp: Value = ureq::post(rpc)
        .timeout(std::time::Duration::from_secs(30))
        .send_json(body)
        .map_err(|e| format!("{method}: {e}"))?
        .into_json()
        .map_err(|e| format!("{method}: invalid JSON: {e}"))?;
    if let Some(err) = resp.get("error") {
        return Err(format!("{method}: {err}"));
    }
    Ok(resp["result"].clone())
}
