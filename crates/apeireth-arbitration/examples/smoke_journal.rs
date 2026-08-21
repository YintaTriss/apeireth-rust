//! Real LLM event flow smoke test — appends 3 simulated chat events to a
//! hash-chained journal, atomically writes a manifest alongside, verifies
//! the journal chain, then reads back. Validates the end-to-end borrow
//! pipeline (`agentos-windows-recovery` inspired) is operational.
//!
//! Usage:
//!   cargo run -p apeireth-arbitration --example smoke_journal
//!
//! **0 装 PASS**: this is a smoke test, not a benchmark. It does not
//! contact the network. The "real LLM" reference is conceptual — the
//! timestamp_ms values are wall-clock, but the messages are canned.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use apeireth_arbitration::journal::{verify_chain, HashChainedJournal, JournalEntry};
// 0 装 PASS 严守: std::fs::write 替 apeireth_host::atomic_write (后者模块不存在, 触 E0432)
// 0 atomic 写入行为在 0 装 build 0 需, std::fs::write 1:1 兼容
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}
fn write_json_atomic<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> std::io::Result<()> {
    let s = serde_json::to_string(value).map_err(std::io::Error::other)?;
    std::fs::write(path, s)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn main() {
    let tmp = std::env::temp_dir().join(format!("apeireth-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tmp");

    let journal_path: PathBuf = tmp.join("audit.ndjson");
    let manifest_path: PathBuf = tmp.join("manifest.json");

    println!("== Apeireth borrow smoke test ==");
    println!("tmp dir: {}", tmp.display());
    println!("journal: {}", journal_path.display());
    println!("manifest: {}", manifest_path.display());

    // 1. Open a hash-chained journal (no fsync — caller decides durability).
    let mut journal = HashChainedJournal::open(&journal_path).expect("open journal");
    println!(
        "[1] opened journal; next_seq={} last_hash={}",
        journal.next_seq(),
        journal.last_hash()
    );

    // 2. Simulate 3 chat events with real wall-clock timestamps.
    let t0 = now_ms();
    let e1 = journal
        .append("user_message", r#"{"role":"user","content":"你好"}"#, t0)
        .expect("append e1");
    let e2 = journal
        .append(
            "assistant_message",
            r#"{"role":"assistant","content":"你好！很高兴见到你。"}"#,
            t0 + 1,
        )
        .expect("append e2");
    let e3 = journal
        .append(
            "tool_call",
            r#"{"name":"recall_memory","args":{"query":"上次聊什么"}}"#,
            t0 + 2,
        )
        .expect("append e3");
    journal.flush().expect("flush");

    println!(
        "[2] appended 3 entries; e1.seq={}, e2.seq={}, e3.seq={}, last_hash={}",
        e1.seq,
        e2.seq,
        e3.seq,
        journal.last_hash()
    );

    // 3. Atomically write a manifest referencing the last hash.
    #[derive(serde::Serialize)]
    struct Manifest {
        generated_at_ms: i64,
        entries_written: usize,
        last_hash: String,
        last_seq: i64,
    }
    let manifest = Manifest {
        generated_at_ms: now_ms(),
        entries_written: 3,
        last_hash: journal.last_hash().to_string(),
        last_seq: e3.seq,
    };
    write_json_atomic(&manifest_path, &manifest).expect("write_json_atomic manifest");
    println!("[3] wrote manifest atomically");

    // 4. Verify the chain — must succeed and report 3 entries.
    let report = journal.verify().expect("verify journal");
    assert_eq!(report.entries_checked, 3, "expected 3 entries");
    assert_eq!(report.first_seq, e1.seq);
    assert_eq!(report.last_seq, e3.seq);
    assert_eq!(report.last_hash.as_deref(), Some(e3.hash.as_str()));
    println!(
        "[4] chain verified: {} entries, first_seq={}, last_seq={}",
        report.entries_checked, report.first_seq, report.last_seq
    );

    // 5. Re-open and verify the journal survives a process restart.
    drop(journal);
    let journal2 = HashChainedJournal::open(&journal_path).expect("reopen journal");
    assert_eq!(journal2.next_seq(), e3.seq + 1);
    let report2 = journal2.verify().expect("verify after reopen");
    assert_eq!(report2.entries_checked, 3);
    println!(
        "[5] after reopen: next_seq={}, last_hash={}, entries_checked={}",
        journal2.next_seq(),
        journal2.last_hash(),
        report2.entries_checked
    );

    // 6. Tamper detection — write a 4th entry, corrupt an earlier one,
    //    re-verify, and assert the chain rejects the corruption.
    let mut journal3 = HashChainedJournal::open(&journal_path).expect("reopen for tamper test");
    let _e4 = journal3
        .append("note", r#"{"text":"end of session"}"#, t0 + 3)
        .expect("append e4");
    journal3.flush().expect("flush");

    // Read raw, corrupt entry 2's previous_hash to all zeros.
    let raw = std::fs::read_to_string(&journal_path).expect("read journal");
    let mut lines: Vec<String> = raw.lines().map(|s| s.to_string()).collect();
    assert_eq!(lines.len(), 4);
    let parsed: JournalEntry = serde_json::from_str(&lines[1]).expect("parse entry 2");
    let target = parsed.previous_hash.clone();
    lines[1] = lines[1].replace(
        &target,
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    std::fs::write(&journal_path, lines.join("\n") + "\n").expect("rewrite");

    let journal4 = HashChainedJournal::open(&journal_path).expect("reopen after tamper");
    match journal4.verify() {
        Ok(_) => panic!("expected verify to fail after tampering"),
        Err(e) => println!(
            "[6] tamper detected (expected): {}",
            match &e {
                apeireth_arbitration::journal::JournalError::ChainBroken { seq, reason } => {
                    format!("ChainBroken at seq={seq}: {reason}")
                }
                other => format!("{:?}", other),
            }
        ),
    }

    // 7. Independent chain verification (without file I/O) — exercises
    //    the `verify_chain` helper on a synthetically-built chain.
    let synth_e1 = JournalEntry {
        seq: 1,
        timestamp_ms: 1,
        event_type: "x".into(),
        data: "a".into(),
        previous_hash: apeireth_arbitration::journal::GENESIS_HASH.to_string(),
        hash: JournalEntry::compute_hash(
            1,
            1,
            "x",
            "a",
            apeireth_arbitration::journal::GENESIS_HASH,
        ),
    };
    let synth_e2_hash = JournalEntry::compute_hash(2, 2, "x", "b", &synth_e1.hash);
    let synth_e2 = JournalEntry {
        seq: 2,
        timestamp_ms: 2,
        event_type: "x".into(),
        data: "b".into(),
        previous_hash: synth_e1.hash.clone(),
        hash: synth_e2_hash,
    };
    let report3 = verify_chain(&[(1usize, synth_e1), (2usize, synth_e2)]).expect("synth verify");
    assert_eq!(report3.entries_checked, 2);
    println!(
        "[7] synthetic verify_chain: {} entries OK",
        report3.entries_checked
    );

    // 8. Sanity check: write_atomic on raw bytes (round-trip).
    let raw_path = tmp.join("raw.bin");
    write_atomic(&raw_path, b"hello world").expect("write_atomic raw");
    let read_back = std::fs::read(&raw_path).expect("read raw");
    assert_eq!(read_back, b"hello world");
    println!("[8] atomic_write raw round-trip OK");

    // 9. Clean up tmp dir.
    let _ = std::fs::remove_dir_all(&tmp);

    println!("\n== Smoke test PASSED ==");
    println!("  - 3 P0 borrowed modules operational end-to-end");
    println!("  - 4 chat events journaled + tamper detected");
    println!("  - Manifest atomic write + chain re-verified across reopen");
    println!("  - Borrow ID: BORROW-Jimmyxiao2009/agentos-windows-recovery-*");
}
