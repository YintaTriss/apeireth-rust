//! L3 Local Runtime HTTP Smoke — no provider credential required.
//!
//! Runtime Decoupling gate: proves the Core Runtime boots and serves over a
//! real HTTP socket with NO API key / NO apikey-ultra.txt. This is the merge-
//! blocking L3 rung of the L0–L5 verification ladder (see
//! docs/runtime-decoupling-report.md).
//!
//! What this verifies (real TCP socket, real HTTP client — NOT router.oneshot):
//!  1. companion_serve boots without any provider credential
//!  2. GET /health → success, core healthy, provider unconfigured
//!  3. GET /v1/apeireth/capabilities → schema valid, core caps available,
//!     chat supported-but-unavailable with reason=provider_not_configured
//!  4. POST /v1/apeireth/sessions → core capability truly works over HTTP
//!  5. POST /v1/chat/completions → stable 503 provider_not_configured, no panic
//!
//! No real key, no external provider, no apikey-ultra.txt. CI-runnable.
//! The example binary is built by `cargo test --workspace` (examples compile
//! alongside tests); we locate it under the target dir.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Locate the compiled companion_serve example binary.
fn example_binary() -> Option<PathBuf> {
    // target dir: CARGO_MANIFEST_DIR/../../target (companion crate -> workspace root/target)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir
        .ancestors()
        .nth(2)
        .map(|a| a.join("target"))
        .unwrap_or_else(|| manifest_dir.join("../../../target"));
    let candidates = [
        target_dir.join("debug").join("examples").join("companion_serve"),
        target_dir.join("debug").join("examples").join("companion_serve.exe"),
        // if CARGO_TARGET_DIR override
        PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap_or_default())
            .join("debug")
            .join("examples")
            .join("companion_serve"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Pick a free port (bind then immediately drop so the child can rebind).
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind free port");
    listener.local_addr().unwrap().port()
}

/// Wait until the server responds on /health, up to ~40s (cold start incl. store open).
fn wait_for_boot(port: u16) -> bool {
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if let Ok((status, _)) = http_get_raw(port, "GET", "/health", None) {
            if status == 200 {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// Minimal HTTP/1.0 client over a raw TcpStream (no external dep).
/// Returns (status_code, body_string).
fn http_get_raw(port: u16, method: &str, path: &str, body: Option<&str>) -> std::io::Result<(u16, String)> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(2),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let req = match body {
        Some(b) => format!(
            "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        ),
        None => format!(
            "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
        ),
    };
    stream.write_all(req.as_bytes())?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp)?;
    // Parse status line + body (split on first \r\n\r\n)
    let (status, body) = parse_http_response(&resp);
    Ok((status, body))
}

/// Parse a raw HTTP/1.0 response into (status_code, body_string).
fn parse_http_response(resp: &str) -> (u16, String) {
    let mut parts = resp.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("").to_string();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    (status, body)
}

struct ServerGuard {
    child: Option<Child>,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Boot companion_serve with NO provider credential on a free port.
/// Returns (guard, port). Returns None (skip) if the example binary is not
/// present (e.g. running a single crate test without building examples first).
fn boot_no_key_server() -> Option<(ServerGuard, u16)> {
    let bin = example_binary()?;
    let port = free_port();
    // CRITICAL: no APEIRETH_API_KEY, no apikey-ultra.txt in cwd.
    // Use a temp dir as cwd so no stray apikey-ultra.txt is found.
    let temp_cwd = std::env::temp_dir().join(format!("apeireth-l3-smoke-{port}"));
    std::fs::create_dir_all(&temp_cwd).ok();
    // Do NOT env_clear — the example needs platform env (SYSTEMROOT/PATH/etc.)
    // to boot its TCP runtime. Instead, explicitly REMOVE the provider key env
    // and rely on the temp cwd (no apikey-ultra.txt there) for the no-key state.
    let mut child = Command::new(&bin)
        .env("PORT", port.to_string())
        .env_remove("APEIRETH_API_KEY")
        .env_remove("APEIRETH_SEED_MEMORY")
        .current_dir(&temp_cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    // Capture stderr for diagnostics if boot fails.
    let stderr = child.stderr.take();
    let guard = ServerGuard { child: Some(child) };
    if wait_for_boot(port) {
        Some((guard, port))
    } else {
        if let Some(mut s) = stderr {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            eprintln!("[L3] server did not boot; stderr:\n{buf}");
        }
        Some((guard, port)) // return so caller asserts boot failure clearly
    }
}

fn http_get(port: u16, path: &str) -> (u16, Value) {
    let (status, body) = http_get_raw(port, "GET", path, None).expect("GET failed");
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    (status, v)
}

fn http_post(port: u16, path: &str, body: Value) -> (u16, Value) {
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    let (status, resp_body) =
        http_get_raw(port, "POST", path, Some(&body_str)).expect("POST failed");
    let v: Value = serde_json::from_str(&resp_body).unwrap_or(Value::Null);
    (status, v)
}

#[test]
fn l3_no_key_runtime_smoke_full() {
    let Some((mut guard, port)) = boot_no_key_server() else {
        eprintln!("[L3] SKIP: companion_serve example binary not found (build examples first)");
        return;
    };

    // Wait for boot (Test 1: process/server boot without provider credential).
    let booted = wait_for_boot(port);
    assert!(booted, "[L3 Test 1] server must boot without key (no /health within 40s)");

    // ── Test 1+2: /health — success, core healthy, provider unconfigured ──
    let (h_status, h_body) = http_get(port, "/health");
    assert_eq!(
        h_status, 200,
        "[L3 Test 1] server must boot without key; got /health status {h_status}"
    );
    let core_status = h_body["core"]["status"].as_str().unwrap_or("");
    let provider_status = h_body["provider"]["status"].as_str().unwrap_or("");
    assert_eq!(
        core_status, "healthy",
        "[L3 Test 2] core must be healthy without key; got core.status={core_status}"
    );
    assert_eq!(
        provider_status, "unconfigured",
        "[L3 Test 2] provider must be unconfigured without key; got provider.status={provider_status}"
    );
    // backward compat: top-level status still present
    assert_eq!(h_body["status"].as_str(), Some("ok"));

    // ── Test 3: capability manifest — schema valid, chat unavailable, core available ──
    let (c_status, c_body) = http_get(port, "/v1/apeireth/capabilities");
    assert_eq!(c_status, 200, "[L3 Test 3] capabilities must return 200");
    assert_eq!(c_body["schema_version"].as_u64(), Some(1));
    assert_eq!(c_body["legacy"].as_bool(), Some(false));

    // find chat.completions: supported=true, available=false, reason=provider_not_configured
    let chat = find_cap(&c_body, "chat.completions");
    let chat = chat.expect("[L3 Test 3] chat.completions must be declared");
    assert_eq!(chat["supported"].as_bool(), Some(true), "chat supported");
    assert_eq!(chat["available"].as_bool(), Some(false), "chat unavailable without key");
    assert_eq!(
        chat["reason"].as_str(),
        Some("provider_not_configured"),
        "chat reason must be machine-readable provider_not_configured"
    );

    // core caps: available=true (sessions.create)
    let sess = find_cap(&c_body, "sessions.create").expect("sessions.create declared");
    assert_eq!(sess["supported"].as_bool(), Some(true));
    assert_eq!(sess["available"].as_bool(), Some(true), "core cap available without key");
    assert!(
        sess.get("reason").map_or(true, |r| r.is_null()),
        "available core cap must have no reason"
    );

    // ── Test 4: core capability truly works over HTTP (session create + read) ──
    let (s_status, s_body) = http_post(
        port,
        "/v1/apeireth/sessions",
        serde_json::json!({"title":"l3-smoke","scope":"global"}),
    );
    assert_eq!(s_status, 201, "[L3 Test 4] session create must succeed (core capability)");
    assert_eq!(s_body["state"].as_str(), Some("active"));
    assert_eq!(s_body["revision"].as_u64(), Some(0));
    let sid = s_body["id"].as_str().expect("session id").to_string();

    // read it back via list (core read capability)
    let (list_status, list_body) = http_get(port, "/v1/apeireth/sessions");
    assert_eq!(list_status, 200, "[L3 Test 4] session list (read) must succeed");
    let sessions = list_body["sessions"].as_array().expect("sessions array");
    assert!(
        sessions.iter().any(|s| s["id"].as_str() == Some(&sid)),
        "created session must appear in list — proves core RT works, not just health"
    );

    // ── Test 5: provider-dependent route graceful failure (stable 503, no panic) ──
    let (chat_status, chat_body) = http_post(
        port,
        "/v1/chat/completions",
        serde_json::json!({"model":"x","messages":[{"role":"user","content":"hi"}]}),
    );
    assert_eq!(
        chat_status, 503,
        "[L3 Test 5] chat must return 503 when unconfigured (got {chat_status})"
    );
    assert_eq!(
        chat_body["error"]["code"].as_str(),
        Some("provider_not_configured"),
        "chat error code must be stable provider_not_configured"
    );

    // server must still be alive after the provider-route hit (no panic/crash)
    let (h2_status, _) = http_get(port, "/health");
    assert_eq!(h2_status, 200, "[L3 Test 5] server must survive provider-route hit");

    // guard dropped here → child killed
    guard.child.take().map(|mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });
}

/// Find a capability by id in a manifest JSON body.
fn find_cap<'a>(manifest: &'a Value, id: &str) -> Option<&'a Value> {
    manifest["capabilities"]
        .as_array()?
        .iter()
        .flat_map(|g| g["capabilities"].as_array().into_iter().flatten())
        .find(|c| c["id"].as_str() == Some(id))
}
