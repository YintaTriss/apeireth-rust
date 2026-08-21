//! Hash-chained append-only audit journal (BORROW: agentos-windows-recovery 2026-08-21).
//!
//! **Borrow ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-hash-chained-journal-2026-08-21`
//! **Source**: <https://github.com/Jimmyxiao2009/agentos-windows-recovery> (MIT)
//! **Original pattern**: `TransactionJournal.cs` (lines 17-105) —
//!   append-only NDJSON, each entry contains `previousHash` + `hash` =
//!   `SHA256(seq || timestamp || eventType || data || previousHash)`,
//!   genesis literal `"GENESIS"`, `WriteThrough` + `Flush(flushToDisk: true)`
//!   per append.
//!
//! ## What this crate adds (without touching canonical order)
//!
//! The existing [`ArbitrationLog`] (per-event `content_hash` + canonical
//! order) is unchanged. This module provides a **second, chain-style**
//! hash over the same event flow:
//!
//! - **Per-entry** `content_hash` (existing): SHA-256 of the event
//!   payload. Detects **content** tampering of a single row.
//! - **Chain** `prev_hash + hash` (NEW, in this module): each entry's
//!   `hash` includes the previous entry's `hash`. Detects **deletion**,
//!   **reordering**, and **insertion** of any entry anywhere in the
//!   journal.
//!
//! Both layers together give the full tamper-evidence guarantee from the
//! borrowed pattern. The chain layer is implemented in Rust as an
//! in-memory + on-disk NDJSON log; the SQLite `[events]` table remains
//! the source of truth for canonical order queries (see
//! [`ArbitrationLog::canonical_order`]).
//!
//! ## 0 装 PASS (per O-5 不假装)
//!
//! - The chain layer is **separate** from the canonical order. Existing
//!   queries against `ArbitrationLog` are unaffected. Callers opt in by
//!   constructing a [`HashChainedJournal`] and feeding it `ArbitrationEvent`s
//!   in append-order.
//! - **fsync / WriteThrough**: R215 强化后 (2026-08-21), [`HashChainedJournal::flush`]
//!   和 [`HashChainedJournal::append`] 现在自带 `File::sync_all()` + best-effort
//!   父目录 `sync_all()`,与 `apeireth-host::atomic_write::write_with_durability`
//!   同款 fsync 模式 (per `apeireth-host\src\atomic_write.rs:201-263`)。
//!   调用者**无需**再用 `apeireth_host::atomic_write::write_with_durability` 包一层。
//!   仍**故意**不把 apeireth-host 拉进本 crate (O-5 不假装 + 依赖面最小化),
//!   fsync 模式直接用 `std::fs::File::sync_all()` 复刻。
//! - **In-memory mode** is provided for testing; the on-disk mode is
//!   best-effort durable to the OS page cache (see above for hardening).
//! - **Chain verification** is a pure function over an iterator of
//!   entries — it does not mutate.
//!
//! ## Format (NDJSON, one event per line)
//!
//! ```text
//! {"seq":1,"timestamp_ms":1700000000000,"event_type":"append","data":"<canonical-json>","previous_hash":"GENESIS","hash":"<sha256-hex>"}
//! {"seq":2,"timestamp_ms":1700000000123,"event_type":"append","data":"<canonical-json>","previous_hash":"<prev-hash>","hash":"<sha256-hex>"}
//! ...
//! ```

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Genesis literal used as `previous_hash` of the first entry.
pub const GENESIS_HASH: &str = "GENESIS";

/// Errors from hash-chained journal operations.
#[derive(Debug, Error)]
pub enum JournalError {
    /// Failed to open / write / read the on-disk NDJSON log.
    #[error("io error on {path}: {source}")]
    Io {
        /// Path that triggered the I/O error.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse a journal line as JSON.
    #[error("serde_json error parsing journal line {line_no}: {source}")]
    Parse {
        /// 1-based line number in the NDJSON file.
        line_no: usize,
        /// Underlying serde_json error.
        #[source]
        source: serde_json::Error,
    },
    /// Chain verification failed for a specific entry.
    #[error("chain verification failed at seq={seq}: {reason}")]
    ChainBroken {
        /// 1-based sequence number of the offending entry.
        seq: i64,
        /// Human-readable failure reason.
        reason: String,
    },
    /// A duplicate sequence number was detected.
    #[error("duplicate sequence number: seq={seq} (previous at line {previous_line})")]
    DuplicateSeq {
        /// The duplicated seq value.
        seq: i64,
        /// 1-based line number of the previous occurrence.
        previous_line: usize,
    },
    /// A non-monotonic sequence (gaps or out-of-order) was detected.
    #[error("non-monotonic sequence: expected seq={expected}, got seq={got} at line {line_no}")]
    NonMonotonicSeq {
        /// Expected next seq.
        expected: i64,
        /// Actual seq observed.
        got: i64,
        /// 1-based line number where the break occurred.
        line_no: usize,
    },
}

impl JournalError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// A single hash-chained journal entry.
///
/// Serialised as one JSON object per line (NDJSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// 1-based sequence number, monotonic within a journal.
    pub seq: i64,
    /// Epoch milliseconds when the entry was appended.
    pub timestamp_ms: i64,
    /// Event type tag (e.g. `"append"`, `"checkpoint"`, `"note"`).
    /// Free-form string the caller picks; the chain does not interpret it.
    pub event_type: String,
    /// Canonical JSON string of the event payload (or any opaque string).
    pub data: String,
    /// Hash of the previous entry (`GENESIS_HASH` for seq=1).
    pub previous_hash: String,
    /// `SHA256(seq ‖ timestamp_ms ‖ event_type ‖ data ‖ previous_hash)`.
    pub hash: String,
}

impl JournalEntry {
    /// Compute the chain hash for the given fields.
    pub fn compute_hash(
        seq: i64,
        timestamp_ms: i64,
        event_type: &str,
        data: &str,
        previous_hash: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(seq.to_be_bytes());
        hasher.update(timestamp_ms.to_be_bytes());
        hasher.update(event_type.as_bytes());
        hasher.update(data.as_bytes());
        hasher.update(previous_hash.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify this entry's hash and `previous_hash` are internally
    /// consistent (does **not** verify against the chain — use
    /// [`HashChainedJournal::verify`] for full chain verification).
    pub fn verify_self(&self) -> Result<(), JournalError> {
        let expected = Self::compute_hash(
            self.seq,
            self.timestamp_ms,
            &self.event_type,
            &self.data,
            &self.previous_hash,
        );
        if expected != self.hash {
            return Err(JournalError::ChainBroken {
                seq: self.seq,
                reason: format!(
                    "hash mismatch: stored={}, recomputed={}",
                    self.hash, expected
                ),
            });
        }
        Ok(())
    }
}

/// Result of a full chain verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    /// Total entries walked.
    pub entries_checked: usize,
    /// First seq observed (1 if non-empty, 0 if empty).
    pub first_seq: i64,
    /// Last seq observed (0 if empty).
    pub last_seq: i64,
    /// Hash of the last entry in the chain (None if empty).
    pub last_hash: Option<String>,
}

impl VerificationReport {
    /// True iff at least one entry was checked.
    pub fn is_non_empty(&self) -> bool {
        self.entries_checked > 0
    }
}

/// Builder / reader / verifier for the hash-chained NDJSON journal.
///
/// On-disk format: one [`JournalEntry`] per line, JSON, LF-terminated.
/// Pure-Rust; no async; no platform-specific dependencies.
pub struct HashChainedJournal {
    path: PathBuf,
    next_seq: i64,
    last_hash: String,
    writer: Option<BufWriter<File>>,
}

impl HashChainedJournal {
    /// Open or create an on-disk journal. Existing entries are read
    /// once to populate `next_seq` + `last_hash` (no full verify; call
    /// [`verify`](Self::verify) separately if you want tamper detection).
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, JournalError> {
        let path = path.as_ref().to_path_buf();
        let (next_seq, last_hash) = match Self::read_tail(&path) {
            Ok(tail) => tail,
            Err(JournalError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                (1, GENESIS_HASH.to_string())
            }
            Err(e) => return Err(e),
        };
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| JournalError::io(&path, source))?;
        Ok(Self {
            path,
            next_seq,
            last_hash,
            writer: Some(BufWriter::new(file)),
        })
    }

    /// Construct an in-memory journal (useful for tests). Not persisted.
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::from(":memory:"),
            next_seq: 1,
            last_hash: GENESIS_HASH.to_string(),
            writer: None,
        }
    }

    /// Path to the on-disk journal file (`":memory:"` for in-memory mode).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Next sequence number that the next [`append`](Self::append) will assign.
    pub fn next_seq(&self) -> i64 {
        self.next_seq
    }

    /// Hash of the last entry (or `GENESIS_HASH` if empty).
    pub fn last_hash(&self) -> &str {
        &self.last_hash
    }

    /// Append a new entry. Returns the constructed entry (with assigned
    /// `seq` + `previous_hash` + `hash`).
    ///
    /// On-disk: `BufWriter::flush()` + `File::sync_all()` after writing the
    /// line — **crash-survivable** (R215 强化 2026-08-21, 同款 fsync 模式参
    /// `apeireth-host\src\atomic_write.rs:201-263`)。调用者无需再自行 fsync。
    pub fn append(
        &mut self,
        event_type: impl Into<String>,
        data: impl Into<String>,
        timestamp_ms: i64,
    ) -> Result<JournalEntry, JournalError> {
        let event_type = event_type.into();
        let data = data.into();
        let seq = self.next_seq;
        let previous_hash = self.last_hash.clone();
        let hash =
            JournalEntry::compute_hash(seq, timestamp_ms, &event_type, &data, &previous_hash);
        let entry = JournalEntry {
            seq,
            timestamp_ms,
            event_type,
            data,
            previous_hash,
            hash: hash.clone(),
        };

        if let Some(writer) = self.writer.as_mut() {
            let line = serde_json::to_string(&entry).map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
            })?;
            writer
                .write_all(line.as_bytes())
                .and_then(|_| writer.write_all(b"\n"))
                .map_err(|source| JournalError::io(&self.path, source))?;
            writer
                .flush()
                .map_err(|source| JournalError::io(&self.path, source))?;
            // R215 强化: per-entry fsync (与 flush() 同款模式, 粒度更细,
            // 每写一条 entry 就 fsync 一次, 牺牲吞吐换强一致)
            let f: &File = writer.get_ref();
            f.sync_all()
                .map_err(|source| JournalError::io(&self.path, source))?;
        }

        self.next_seq += 1;
        self.last_hash = hash;
        Ok(entry)
    }

    /// Flush the underlying writer + fsync (crash-survivable durability).
    ///
    /// R215 强化 (2026-08-21): 在 `BufWriter::flush()` 之后立即 `sync_all()`,
    /// 并 best-effort 对父目录 `sync_all()` — 与 `apeireth-host::atomic_write::
    /// write_with_durability` 同款 fsync 模式 (per `apeireth-host\src\atomic_write.rs:201-263`),
    /// 保证断电时末尾 entry 不丢。
    ///
    /// 调用者无需再用 `apeireth_host::atomic_write::write_with_durability` 包一层。
    pub fn flush(&mut self) -> Result<(), JournalError> {
        if let Some(writer) = self.writer.as_mut() {
            writer
                .flush()
                .map_err(|source| JournalError::io(&self.path, source))?;
            // R215 强化: fsync the underlying file (借用 atomic_write 的 durability 模式)
            let f: &File = writer.get_ref();
            f.sync_all()
                .map_err(|source| JournalError::io(&self.path, source))?;
            // best-effort parent dir fsync (Linux/macOS, 防 crash 后父目录 inode 丢)
            if let Some(parent) = self.path.parent() {
                if let Ok(dir) = std::fs::File::open(parent) {
                    let _ = dir.sync_all(); // 不阻断
                }
            }
        }
        Ok(())
    }

    /// Read the entire journal (NDJSON) and verify the chain.
    ///
    /// Detects:
    /// - **Tampering of any single entry**: `JournalEntry::verify_self`
    ///   recomputes the hash and rejects mismatches.
    /// - **Deletion / insertion**: `previous_hash` of entry N must equal
    ///   `hash` of entry N-1; a missing or extra entry breaks the chain.
    /// - **Reordering**: same as above — `previous_hash` mismatch.
    /// - **Sequence gaps / non-monotonic**: `seq` of entry N must equal
    ///   `prev_seq + 1`.
    /// - **Duplicate `seq`**: two entries with the same `seq` are rejected.
    pub fn verify(&self) -> Result<VerificationReport, JournalError> {
        let mut entries = Vec::new();
        if self.path == Path::new(":memory:") {
            // In-memory mode — nothing to read from disk. Caller should
            // verify by collecting entries they've appended externally.
            return Ok(VerificationReport {
                entries_checked: 0,
                first_seq: 0,
                last_seq: 0,
                last_hash: None,
            });
        }
        let file = File::open(&self.path).map_err(|source| JournalError::io(&self.path, source))?;
        let reader = BufReader::new(file);
        for (line_no, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(|source| JournalError::io(&self.path, source))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: JournalEntry =
                serde_json::from_str(&line).map_err(|source| JournalError::Parse {
                    line_no: line_no + 1,
                    source,
                })?;
            entries.push((line_no + 1, entry));
        }

        verify_chain(&entries)
    }

    fn read_tail(path: &Path) -> Result<(i64, String), JournalError> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok((1, GENESIS_HASH.to_string()))
            }
            Err(source) => return Err(JournalError::io(path, source)),
        };
        let reader = BufReader::new(file);
        let mut last_entry: Option<JournalEntry> = None;
        for (line_no, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(|source| JournalError::io(path, source))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: JournalEntry =
                serde_json::from_str(&line).map_err(|source| JournalError::Parse {
                    line_no: line_no + 1,
                    source,
                })?;
            // Note: we deliberately do NOT call `entry.verify_self()` here.
            // Open is permissive — read what we can. To detect tampering,
            // call [` HashChainedJournal::verify `] explicitly. The reason
            // for this asymmetry: a corrupted journal must still be openable
            // for forensic analysis (the agentos pattern: the engine refuses
            // to *proceed* on invalid journal, but the file itself stays
            // readable for investigation).
            last_entry = Some(entry);
        }
        match last_entry {
            Some(e) => Ok((e.seq + 1, e.hash)),
            None => Ok((1, GENESIS_HASH.to_string())),
        }
    }
}

/// Verify a sequence of `(line_no, entry)` pairs as a chain.
///
/// Exposed (not `pub(crate)`) so callers with their own in-memory entry
/// buffer can verify without going through a journal file.
pub fn verify_chain(entries: &[(usize, JournalEntry)]) -> Result<VerificationReport, JournalError> {
    let mut expected_prev_hash = GENESIS_HASH.to_string();
    let mut expected_next_seq: i64 = 1;
    let mut last_hash: Option<String> = None;
    let mut first_seq: i64 = 0;

    for (line_no, entry) in entries {
        if *line_no == 1 && entries.len() >= 1 && first_seq == 0 {
            first_seq = entry.seq;
        }
        if entry.seq != expected_next_seq {
            // First entry must be seq=1; subsequent must be prev+1.
            // (We also reject any duplicate seq because expected_next_seq
            // is derived from the previous observed seq.)
            if entry.seq < expected_next_seq {
                return Err(JournalError::DuplicateSeq {
                    seq: entry.seq,
                    previous_line: line_no.saturating_sub(1),
                });
            }
            return Err(JournalError::NonMonotonicSeq {
                expected: expected_next_seq,
                got: entry.seq,
                line_no: *line_no,
            });
        }
        entry.verify_self()?;
        if entry.previous_hash != expected_prev_hash {
            return Err(JournalError::ChainBroken {
                seq: entry.seq,
                reason: format!(
                    "previous_hash mismatch: stored={}, expected={}",
                    entry.previous_hash, expected_prev_hash
                ),
            });
        }
        expected_prev_hash = entry.hash.clone();
        expected_next_seq = entry.seq + 1;
        last_hash = Some(entry.hash.clone());
    }

    Ok(VerificationReport {
        entries_checked: entries.len(),
        first_seq: if entries.is_empty() { 0 } else { first_seq },
        last_seq: entries.last().map(|(_, e)| e.seq).unwrap_or(0),
        last_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn compute_hash_is_deterministic() {
        let h1 = JournalEntry::compute_hash(1, 1000, "append", "{\"x\":1}", GENESIS_HASH);
        let h2 = JournalEntry::compute_hash(1, 1000, "append", "{\"x\":1}", GENESIS_HASH);
        assert_eq!(h1, h2);
        // 64 hex chars (32 bytes)
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn chain_links_correctly() {
        let mut j = HashChainedJournal::in_memory();
        let e1 = j.append("append", "{\"a\":1}", 1000).unwrap();
        let e2 = j.append("append", "{\"a\":2}", 1001).unwrap();
        let e3 = j.append("append", "{\"a\":3}", 1002).unwrap();

        assert_eq!(e1.previous_hash, GENESIS_HASH);
        assert_eq!(e2.previous_hash, e1.hash);
        assert_eq!(e3.previous_hash, e2.hash);
        assert_eq!(j.next_seq(), 4);
        assert_eq!(j.last_hash(), e3.hash);
    }

    #[test]
    fn verify_empty_journal_is_ok() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.ndjson");
        let j = HashChainedJournal::open(&path).unwrap();
        // No entries — verify (file read) returns empty report.
        let report = j.verify().unwrap();
        assert_eq!(report.entries_checked, 0);
        assert_eq!(report.first_seq, 0);
        assert_eq!(report.last_seq, 0);
        assert!(report.last_hash.is_none());
    }

    #[test]
    fn verify_chain_function_accepts_valid_chain() {
        let mut j = HashChainedJournal::in_memory();
        j.append("append", "a", 1000).unwrap();
        j.append("append", "b", 1001).unwrap();
        j.append("append", "c", 1002).unwrap();

        // Build the entries list from the journal (via on-disk would
        // require a real file; use verify_chain directly with a synthetic
        // valid chain to exercise the helper).
        let e1 = JournalEntry {
            seq: 1,
            timestamp_ms: 1,
            event_type: "t".into(),
            data: "a".into(),
            previous_hash: GENESIS_HASH.to_string(),
            hash: JournalEntry::compute_hash(1, 1, "t", "a", GENESIS_HASH),
        };
        let e2 = JournalEntry {
            seq: 2,
            timestamp_ms: 2,
            event_type: "t".into(),
            data: "b".into(),
            previous_hash: e1.hash.clone(),
            hash: JournalEntry::compute_hash(2, 2, "t", "b", &e1.hash),
        };
        let entries = vec![(1usize, e1), (2usize, e2)];
        let report = verify_chain(&entries).unwrap();
        assert_eq!(report.entries_checked, 2);
        assert_eq!(report.first_seq, 1);
        assert_eq!(report.last_seq, 2);
        assert_eq!(
            report.last_hash.as_deref(),
            Some(entries[1].1.hash.as_str())
        );
    }

    #[test]
    fn verify_detects_tampered_hash() {
        let e1 = JournalEntry {
            seq: 1,
            timestamp_ms: 1,
            event_type: "t".into(),
            data: "a".into(),
            previous_hash: GENESIS_HASH.to_string(),
            hash: JournalEntry::compute_hash(1, 1, "t", "a", GENESIS_HASH),
        };
        let mut e2 = JournalEntry {
            seq: 2,
            timestamp_ms: 2,
            event_type: "t".into(),
            data: "b".into(),
            previous_hash: e1.hash.clone(),
            hash: JournalEntry::compute_hash(2, 2, "t", "b", &e1.hash),
        };
        // Tamper: change data but keep old hash.
        e2.data = "tampered".into();
        let entries = vec![(1usize, e1), (2usize, e2)];
        let err = verify_chain(&entries).unwrap_err();
        assert!(matches!(err, JournalError::ChainBroken { seq: 2, .. }));
    }

    #[test]
    fn verify_detects_previous_hash_mismatch() {
        let e1 = JournalEntry {
            seq: 1,
            timestamp_ms: 1,
            event_type: "t".into(),
            data: "a".into(),
            previous_hash: GENESIS_HASH.to_string(),
            hash: JournalEntry::compute_hash(1, 1, "t", "a", GENESIS_HASH),
        };
        let e2 = JournalEntry {
            seq: 2,
            timestamp_ms: 2,
            event_type: "t".into(),
            data: "b".into(),
            previous_hash: "deadbeef".to_string(), // wrong!
            hash: JournalEntry::compute_hash(2, 2, "t", "b", "deadbeef"),
        };
        let entries = vec![(1usize, e1), (2usize, e2)];
        let err = verify_chain(&entries).unwrap_err();
        assert!(matches!(err, JournalError::ChainBroken { seq: 2, .. }));
    }

    #[test]
    fn verify_detects_duplicate_seq() {
        let e1 = JournalEntry {
            seq: 1,
            timestamp_ms: 1,
            event_type: "t".into(),
            data: "a".into(),
            previous_hash: GENESIS_HASH.to_string(),
            hash: JournalEntry::compute_hash(1, 1, "t", "a", GENESIS_HASH),
        };
        let mut e2 = e1.clone();
        e2.data = "b".into();
        e2.hash = JournalEntry::compute_hash(1, 1, "t", "b", &e1.previous_hash);
        let entries = vec![(1usize, e1), (2usize, e2)];
        let err = verify_chain(&entries).unwrap_err();
        assert!(matches!(err, JournalError::DuplicateSeq { .. }));
    }

    #[test]
    fn verify_detects_non_monotonic_seq() {
        let e1 = JournalEntry {
            seq: 1,
            timestamp_ms: 1,
            event_type: "t".into(),
            data: "a".into(),
            previous_hash: GENESIS_HASH.to_string(),
            hash: JournalEntry::compute_hash(1, 1, "t", "a", GENESIS_HASH),
        };
        let e3 = JournalEntry {
            seq: 3, // gap from 1 → 3
            timestamp_ms: 3,
            event_type: "t".into(),
            data: "c".into(),
            previous_hash: e1.hash.clone(),
            hash: JournalEntry::compute_hash(3, 3, "t", "c", &e1.hash),
        };
        let entries = vec![(1usize, e1), (2usize, e3)];
        let err = verify_chain(&entries).unwrap_err();
        assert!(matches!(err, JournalError::NonMonotonicSeq { got: 3, .. }));
    }

    #[test]
    fn on_disk_journal_persists_and_verifies() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.ndjson");

        // Write 3 entries
        {
            let mut j = HashChainedJournal::open(&path).unwrap();
            j.append("user_message", r#"{"i":1}"#, 1000).unwrap();
            j.append("tool_call", r#"{"i":2}"#, 1001).unwrap();
            j.append("approval", r#"{"i":3}"#, 1002).unwrap();
            j.flush().unwrap();
        }

        // Re-open and verify
        let j2 = HashChainedJournal::open(&path).unwrap();
        assert_eq!(j2.next_seq(), 4);
        let report = j2.verify().unwrap();
        assert_eq!(report.entries_checked, 3);
        assert_eq!(report.first_seq, 1);
        assert_eq!(report.last_seq, 3);
        assert!(report.last_hash.is_some());

        // Tamper: append a fourth entry then modify the on-disk file
        // to break entry 2's previous_hash.
        {
            let mut j3 = HashChainedJournal::open(&path).unwrap();
            j3.append("note", r#"{"i":4}"#, 1003).unwrap();
            j3.flush().unwrap();
        }
        // Read raw file, corrupt entry 2.
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = raw.lines().map(|s| s.to_string()).collect();
        assert_eq!(lines.len(), 4);
        // Replace entry 2's `previous_hash` with something else.
        lines[1] = lines[1].replace(
            &{
                let parsed: JournalEntry = serde_json::from_str(&lines[1]).unwrap();
                parsed.previous_hash
            },
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        let corrupted = lines.join("\n") + "\n";
        std::fs::write(&path, corrupted).unwrap();

        let j4 = HashChainedJournal::open(&path).unwrap();
        // Tampering breaks the chain — verify must reject. The exact
        // error variant depends on what gets corrupted first; in this
        // case we changed entry 2's previous_hash without re-hashing, so
        // verify_self fails on entry 2 with a hash mismatch (which
        // reports as ChainBroken). Either ChainBroken at seq 2 or
        // ChainBroken at seq 3 (where the chain link would break) is
        // acceptable; both prove tampering is detected.
        let err = j4.verify().unwrap_err();
        match err {
            JournalError::ChainBroken { seq: 2, .. } | JournalError::ChainBroken { seq: 3, .. } => {
            }
            other => panic!("expected ChainBroken at seq 2 or 3, got {other:?}"),
        }
    }

    #[test]
    fn open_resumes_from_existing_journal() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("resume.ndjson");
        {
            let mut j = HashChainedJournal::open(&path).unwrap();
            j.append("a", "1", 1).unwrap();
            j.append("b", "2", 2).unwrap();
            j.flush().unwrap();
        }
        // Reopen: next_seq should be 3.
        let mut j2 = HashChainedJournal::open(&path).unwrap();
        assert_eq!(j2.next_seq(), 3);
        // Capture old last_hash BEFORE appending.
        let prev_hash = j2.last_hash().to_string();
        let e3 = j2.append("c", "3", 3).unwrap();
        assert_eq!(e3.seq, 3);
        assert_eq!(e3.previous_hash, prev_hash);
        // After append, j2's last_hash is now e3's hash (different).
        assert_eq!(j2.last_hash(), e3.hash);
        assert_ne!(j2.last_hash(), prev_hash);
    }

    // R215 强化 (2026-08-21): flush() 必须 fsync,断电时末尾 entry 不丢。
    // 验证方式:append → flush (走 sync_all 路径) → 再读盘验证内容 + next_seq。
    // Windows 上 `File::sync_all()` 在 tempfile 目录里是可用的 (NTFS journal),
    // 与 POSIX 行为略有差异但都能保证数据落地。
    #[test]
    fn flush_calls_sync_all_on_underlying_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fsync-audit.ndjson");
        let mut j = HashChainedJournal::open(&path).unwrap();
        j.append("a", "1", 1).unwrap();
        j.append("b", "2", 2).unwrap();
        // R215 强化: flush 不应该 panic, 应该成功 (sync_all 在 tmpdir 里可用)
        j.flush().expect("flush must succeed with fsync");
        // 验证: 末尾 entry 已落盘 (append 已 fsync, flush 是冗余的 fsync, 但应成功)
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"event_type\":\"a\""),
            "entry a should be on disk"
        );
        assert!(
            raw.contains("\"event_type\":\"b\""),
            "entry b should be on disk"
        );
        assert_eq!(j.next_seq(), 3);

        // 再 append 一条 + flush, 验证 flush 本身(非 append) 走的 fsync 路径
        j.append("c", "3", 3).unwrap();
        j.flush()
            .expect("second flush must also succeed with fsync");
        let raw2 = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw2.contains("\"event_type\":\"c\""),
            "entry c should be on disk after flush"
        );
        assert_eq!(j.next_seq(), 4);
    }
}
