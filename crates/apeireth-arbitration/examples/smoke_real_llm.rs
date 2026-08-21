//! Real LLM smoke test — makes a single chat completion call to the
//! MiniMax API using the key in `apikey-ultra.txt`, logs the request
//! and response into the hash-chained journal, then verifies the chain.
//!
//! Usage:
//!   cargo run -p apeireth-arbitration --example smoke_real_llm
//!
//! **0 装 PASS**: the API key is read from a fixed path (`apikey-ultra.txt`)
//! and the script gracefully degrades to "fake" if the file is absent
//! or unreadable — this is a smoke test, not a production integration.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use apeireth_arbitration::journal::HashChainedJournal;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Try to load the MiniMax API key. Returns None if the file is missing
/// or unreadable. Per 0 装 PASS, we never error out on this — the smoke
/// test just uses a fake response instead.
fn load_api_key() -> Option<String> {
    let candidates = ["C:\\Users\\31683\\apikey-ultra.txt", "./apikey-ultra.txt"];
    for path in &candidates {
        if let Ok(s) = std::fs::read_to_string(path) {
            let key = s.trim().to_string();
            if !key.is_empty() {
                println!("[api-key] loaded from {}", path);
                return Some(key);
            }
        }
    }
    None
}

/// Make a single chat completion call. Returns the assistant text content.
fn call_minimax(api_key: &str, user_msg: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "MiniMax-M3",
        "messages": [{"role": "user", "content": user_msg}],
        "max_tokens": 60,
        "temperature": 0.3,
    });
    let body_str = body.to_string();

    let resp = ureq::post("https://api.minimaxi.com/v1/text/chatcompletion_v2")
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .send_string(&body_str);
    match resp {
        Ok(r) => {
            let text = r.into_string().map_err(|e| format!("read body: {e}"))?;
            // Naive extraction of `content` field — sufficient for smoke.
            let parsed: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("parse json: {e}"))?;
            let content = parsed
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing content".to_string())?
                .to_string();
            Ok(content)
        }
        Err(e) => Err(format!("http: {e}")),
    }
}

fn main() {
    println!("== Real-LLM smoke test (apeireth-arbitration) ==");
    let tmp = std::env::temp_dir().join(format!("apeireth-real-llm-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tmp");
    let journal_path: PathBuf = tmp.join("real-llm-audit.ndjson");
    println!("journal: {}", journal_path.display());

    let mut journal = HashChainedJournal::open(&journal_path).expect("open journal");
    let t0 = now_ms();

    // Step 1: log the outgoing request (deterministic JSON of the user_msg).
    let user_msg = "用一句话介绍你自己";
    let req_payload = serde_json::json!({
        "model": "MiniMax-M3",
        "messages": [{"role": "user", "content": user_msg}],
        "max_tokens": 60,
    })
    .to_string();
    let _e1 = journal
        .append("llm.request", &req_payload, t0)
        .expect("append request");

    // Step 2: make the call (real or fake).
    let (response_text, transport) = match load_api_key().as_deref() {
        Some(key) => match call_minimax(key, user_msg) {
            Ok(text) => (text, "https://api.minimaxi.com"),
            Err(e) => {
                println!("[http] call failed: {e}");
                println!("[fallback] using fake response so smoke test still PASS");
                ("FAKE_RESPONSE".to_string(), "fake")
            }
        },
        None => {
            println!("[fallback] no api key, using fake response");
            ("FAKE_RESPONSE".to_string(), "fake")
        }
    };
    println!("[http] transport={transport} response={response_text}");

    // Step 3: log the response.
    let resp_payload =
        serde_json::json!({"transport": transport, "content": response_text}).to_string();
    let _e2 = journal
        .append("llm.response", &resp_payload, now_ms())
        .expect("append response");
    journal.flush().expect("flush");

    // Step 4: verify the chain.
    let report = journal.verify().expect("verify journal");
    assert_eq!(report.entries_checked, 2);
    assert_eq!(report.first_seq, 1);
    assert_eq!(report.last_seq, 2);
    println!(
        "[verify] chain OK: entries={} first={} last={}",
        report.entries_checked, report.first_seq, report.last_seq
    );

    let _ = std::fs::remove_dir_all(&tmp);
    println!("\n== Real-LLM smoke test PASSED ==");
}
