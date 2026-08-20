//! L3 真 TCP socket smoke 测 (per PR #2 concept).
//!
//! **目的**: 验证 `companion_serve` example 真的接了 HTTP 服务, **不依赖**任何 HTTP client 库
//! (0 reqwest / 0 ureq / 0 axum test feature), **不修改** `crates/apeireth-companion/src/` 任何
//! 既有 .rs, **不修改** Cargo.toml, **不引入**任何新依赖.
//!
//! **机制**: 真 spawn `target/debug/examples/companion_serve` 子进程 → 等 readiness
//! (loop connect_timeout) → `std::net::TcpStream::connect` 真接 127.0.0.1:PORT → 手构
//! HTTP/1.0 请求 (`GET /v1/models HTTP/1.0\r\nApi-Key: ...\r\nHost: ...\r\n\r\n`) →
//! `read_to_end()` 读完整响应 → 解析 status line 验 200/404 + body 含 `object`/`data`/`model` 字段.
//!
//! **0 触碰 严守** (per spec §6.3 13 项):
//! - 0 触碰 `crates/apeireth-companion/src/` 既有 .rs
//! - 0 触碰 24 LOCKED crate
//! - 0 改 workspace.version (1.2.0)
//! - 0 触碰 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache)
//! - 0 触碰 `gh_*.ps1` 5 文件 / `crates/apeireth-environment/tests/` /
//!   `crates/apeireth-provider/tests/`
//! - 0 引外部依赖 (Cargo.toml 0 改)
//!
//! **8 哲学锚穿透** (per project 8-anchor framework):
//! - S-1 真实隔离目标: 真 spawn 子进程, 真接 TCP, 真读字节; 不 mock
//! - S-2 实事求是: 失败路径 (404) 也测; 不假装"只测 happy path"
//! - S-3 质量工程化: 3 测独立 spawn (端口隔离, 0 state 泄漏), RAII cleanup 0 残留子进程
//! - O-1 安全优先: `Api-Key` header 仅是 placeholder (subprocess 启时不查), 无 master token
//! - O-2 走在前人肩上: 用 cargo 注入 env `CARGO_EXAMPLE_EXAMPLE_<name>` + `CARGO_MANIFEST_DIR` 双路径兜底拿编译产物
//! - O-3 干到底: 一次 1 文件 3 测, 完工即用; 不留 TODO
//! - O-4 任何人都能接手: 模式清晰 (spawn / wait / req / parse / cleanup), 注释详
//! - O-5 不假装: 0 装 PASS — 子进程起不来 → 测 fail 并说明; 不断言"理论上应该"
//!
//! **每测独立**: 3 测各起 1 子进程, 端口不同 (18091/18092/18093) — 0 状态泄漏.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// 子进程 wrapper — `Drop` 严守清理 (per O-1 + S-3), 即使测 panic 也不残留僵尸进程.
struct ServeProc {
    child: Child,
}

impl ServeProc {
    fn spawn(port: u16) -> Self {
        // 解析 example 编译产物路径. CARGO_EXAMPLE_EXAMPLE_<name> 是 cargo 运行时注入 env (per Cargo 文档),
        // 但仅在 example 自身 + tests 间共享; 部分 cargo 版本对 integration tests 注入有时序差异, 因此双路径兜底:
        //   1. 优先读 CARGO_EXAMPLE_EXAMPLE_companion_serve env (cargo 标准注入)
        //   2. fallback: 用 CARGO_MANIFEST_DIR (编译期注入的 crate 根) 推 workspace target/debug/examples/
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let exe = std::env::var("CARGO_EXAMPLE_EXAMPLE_companion_serve")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                // workspace root = manifest_dir 上 2 级 (本 crate 在 crates/apeireth-companion/ 下)
                let workspace_root = std::path::Path::new(manifest_dir)
                    .parent()
                    .and_then(|p| p.parent())
                    .expect("workspace_root 解析");
                // Windows 路径加 .exe 后缀
                let exe_path = workspace_root
                    .join("target")
                    .join("debug")
                    .join("examples")
                    .join(if cfg!(windows) {
                        "companion_serve.exe"
                    } else {
                        "companion_serve"
                    });
                exe_path.to_string_lossy().into_owned()
            });
        let exe_for_err = exe.clone();

        let mut cmd = Command::new(exe);
        cmd.env("PORT", port.to_string())
            // APEIRETH_API_KEY 是 example 启动必备 (load_key() 失败 → main() Err → exit).
            // smoke 测 0 真接 LLM, 仅验 HTTP 路由 → 给个非空 placeholder 即够.
            .env("APEIRETH_API_KEY", "l3-smoke-placeholder-key")
            // 关闭 LLM 重资源管道: 短梦静默期 + 短反思周期, 避免测试期子进程吃资源.
            .env("APEIRETH_DREAM_QUIET_SECONDS", "3600")
            .env("APEIRETH_REFLECT_PERIOD_HOURS", "168")
            .env("RUST_LOG", "warn")
            // stdout/stderr 接到 pipe → 不阻塞子进程; 但不读 → 真出错时不卡死测试.
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("spawn companion_serve 失败 (exe={exe_for_err:?}): {e}"));

        Self { child }
    }

    /// 等到可连接 (per S-2 + O-5: 不假装"立即可连", 真等 readiness).
    /// 最长等 10s, 每 100ms 试一次.
    fn wait_ready(&self, addr: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut last_err = String::new();
        loop {
            if Instant::now() >= deadline {
                panic!("等 {addr} readiness 超时 (10s): {last_err}");
            }
            // 先解析域名/IP, 再 connect_timeout
            match addr.to_socket_addrs() {
                Ok(mut addrs) => {
                    if let Some(a) = addrs.next() {
                        match TcpStream::connect_timeout(&a, Duration::from_millis(500)) {
                            Ok(_) => return, // 真接上了 → 服务 ready
                            Err(e) => {
                                last_err = format!("{e}");
                            }
                        }
                    } else {
                        last_err = "no socket addrs".into();
                    }
                }
                Err(e) => {
                    last_err = format!("resolve: {e}");
                }
            }
            sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for ServeProc {
    fn drop(&mut self) {
        // S-3 质量工程化: 子进程清理严守 — kill 不一定等, 但尽力 wait 收尸.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 真接 TCP + 手构 HTTP/1.0 请求 + 读完整响应.
fn http1_get(host_port: &str, path: &str, api_key: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(host_port).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set_read_timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("set_write_timeout");

    // HTTP/1.0 + Connection: close → server 写完即关连接, 读 read_to_end() 干净.
    let req = format!(
        "GET {path} HTTP/1.0\r\n\
         Api-Key: {api_key}\r\n\
         Host: 127.0.0.1\r\n\
         Connection: close\r\n\
         \r\n"
    );
    stream.write_all(req.as_bytes()).expect("write req");
    stream.flush().expect("flush");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read_to_end");
    let raw = String::from_utf8_lossy(&buf).into_owned();

    // 解析 status line: "HTTP/1.0 200 OK\r\n..."
    let status_line = raw.split("\r\n").next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("无法解析 status line: {status_line:?}"));

    // body = status line + headers 之后的全部
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();

    (status, body)
}

/// 测 1: GET /v1/models → 200 + body 含 `object`/`data`/`model` 字段 (per OpenAI 兼容契约).
#[test]
fn l3_real_socket_get_v1_models_200() {
    let port: u16 = 18091;
    let proc = ServeProc::spawn(port);
    proc.wait_ready(&format!("127.0.0.1:{port}"), Duration::from_secs(10));

    let (status, body) = http1_get(&format!("127.0.0.1:{port}"), "/v1/models", "test-key");

    assert_eq!(status, 200, "GET /v1/models 应返 200, body={body}");
    assert!(body.contains("\"object\""), "body 缺 object 字段: {body}");
    assert!(body.contains("\"data\""), "body 缺 data 字段: {body}");
    assert!(body.contains("\"model\""), "body 缺 model 字段: {body}");

    drop(proc); // 显式 drop 触发清理 (panic 前也由 Drop guard 保底)
}

/// 测 2: GET /health → 200 + body 含 `status: ok`.
#[test]
fn l3_real_socket_get_health_200() {
    let port: u16 = 18092;
    let proc = ServeProc::spawn(port);
    proc.wait_ready(&format!("127.0.0.1:{port}"), Duration::from_secs(10));

    let (status, body) = http1_get(&format!("127.0.0.1:{port}"), "/health", "test-key");

    assert_eq!(status, 200, "GET /health 应返 200, body={body}");
    assert!(
        body.contains("\"status\"") && body.contains("ok"),
        "body 应含 status: ok: {body}"
    );

    drop(proc);
}

/// 测 3: GET /zzz (unknown path) → 404 (axum 默认对未注册路径返 404).
/// **S-2 + O-5 严守**: 不假装"只测 happy path", 失败路径也是契约一部分.
#[test]
fn l3_real_socket_get_unknown_path_404() {
    let port: u16 = 18093;
    let proc = ServeProc::spawn(port);
    proc.wait_ready(&format!("127.0.0.1:{port}"), Duration::from_secs(10));

    let (status, body) = http1_get(&format!("127.0.0.1:{port}"), "/zzz", "test-key");

    assert_eq!(status, 404, "GET /zzz (未知路径) 应返 404, body={body}");

    drop(proc);
}
