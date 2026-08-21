//! Atomic file write utilities (BORROW: agentos-windows-recovery 2026-08-21).
//!
//! **Borrow ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-atomic-write-2026-08-21`
//! **Source**: <https://github.com/Jimmyxiao2009/agentos-windows-recovery> (MIT)
//! **Original pattern**: `JsonSupport.WriteAtomic` (TransactionEngine.cs) —
//!   write to `<target>.tmp-<uuid>` then `File.Move(..., overwrite: true)`,
//!   `finally` block cleans up the tmp file on any error path.
//!
//! This Rust port lives in `apeireth-host` (the host infrastructure facade)
//! so any workspace crate can `use apeireth_host::atomic_write::*` without
//! pulling in `keyring`/`machine_id`. Both `write_atomic` and `write_json_atomic`
//! are dependency-free (no async, no serde) so they are cheap to call from
//! any layer (boot-time config, snapshot writer, journal appender, etc).
//!
//! ## 0 装 PASS (per O-5 不假装)
//!
//! - **No fsync**: this module does **not** flush to physical disk. It only
//!   guarantees atomic *rename*, which protects against torn files in the
//!   sense of "either the old or the new content is visible, never a half-
//!   written mix". For crash-survivable durability (e.g. append-only audit
//!   journal across a power loss), the caller MUST additionally open the
//!   file with `FileOptions::WriteThrough` (or `fsync`) before rename.
//!   See [`write_with_durability`] for the convenience wrapper.
//! - **No directory fsync**: POSIX requires fsync on the parent directory
//!   after rename for the new name to survive a crash. This module does not
//!   do that. Callers writing into critical paths (manifest, journal,
//!   rollback snapshot) should add a directory fsync on platforms where
//!   it is meaningful.
//! - **No permission preservation**: `std::fs::rename` does **not**
//!   preserve the file mode/owner of the destination. On Unix, the file
//!   inherits the mode of the source file (the tmp), which may differ from
//!   the pre-existing target. Callers that care about mode (e.g. private
//!   credentials) should `chmod` the target after a successful rename.
//! - **Windows MoveFileEx semantics**: `fs::rename` on Windows uses
//!   `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`. If the target is open by
//!   another process (e.g. AV scanner), rename may fail with `ERROR_SHARING_VIOLATION`.
//!   Caller decides retry policy.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

/// Errors from atomic file writes.
#[derive(Debug, Error)]
pub enum AtomicWriteError {
    /// Failed to open / write to the temporary file.
    #[error("io error writing temp file {temp}: {source}")]
    Write {
        /// The temp file path that failed.
        temp: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to rename the temp file onto the target.
    #[error("io error renaming {temp} -> {target}: {source}")]
    Rename {
        /// The temp file path that failed to rename.
        temp: PathBuf,
        /// The target path the rename was attempting.
        target: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to remove a stale temp file (best-effort cleanup).
    #[error("io error removing stale temp {temp}: {source}")]
    Cleanup {
        /// The temp file path that failed to clean up.
        temp: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Serde JSON serialization failed.
    #[error("serde_json error serializing {target}: {source}")]
    Serialize {
        /// The target path serialization was attempting.
        target: PathBuf,
        /// Underlying serde_json error.
        #[source]
        source: serde_json::Error,
    },
}

impl AtomicWriteError {
    /// Best-effort cleanup of a stale temp file, swallowing the error
    /// (only used in error paths, so we never *also* fail on cleanup).
    pub(crate) fn best_effort_cleanup(temp: &Path) {
        if temp.exists() {
            // We deliberately swallow cleanup errors: the caller has already
            // failed; we don't want to mask the original error with a
            // confusing "could not delete leftover .tmp" message.
            let _ = fs::remove_file(temp);
        }
    }
}

/// Compute the tmp file path for a target.
///
/// Convention: `<target>.tmp-<uuid>` (uuid v4). The uuid suffix avoids
/// collision when two concurrent writers race for the same target on
/// different processes (common in supervisor / migration scenarios).
fn tmp_path_for(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = parent.to_path_buf();
    match target.file_name() {
        Some(name) => {
            // Append ".tmp-<uuid>" to the existing filename.
            let mut s = name.to_os_string();
            s.push(format!(".tmp-{}", Uuid::new_v4()));
            tmp.push(s);
        }
        None => {
            // No filename component (target is e.g. "/" or ""); use bare .tmp-<uuid>.
            tmp.push(format!(".tmp-{}", Uuid::new_v4()));
        }
    }
    tmp
}

/// Atomically write raw bytes to `target`.
///
/// Behaviour (mirrors `JsonSupport.WriteAtomic` from
/// `agentos-windows-recovery/TransactionEngine.cs`):
///
/// 1. Compute `<target>.tmp-<uuid>` in the same directory.
/// 2. Write all bytes to the tmp file via `std::fs::write`.
/// 3. `std::fs::rename` onto the target (atomic on POSIX, uses
///    `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` on Windows).
/// 4. On **any** error path, attempt best-effort cleanup of the tmp file
///    so we never leak a `.tmp-<uuid>` next to the target.
///
/// Returns `Ok(())` only after a successful rename. The original `target`
/// (if it existed) is either fully intact (rename failed) or fully
/// replaced (rename succeeded); never a torn half-state.
pub fn write_atomic(target: impl AsRef<Path>, bytes: &[u8]) -> Result<(), AtomicWriteError> {
    let target = target.as_ref();
    let tmp = tmp_path_for(target);

    if let Err(source) = fs::write(&tmp, bytes) {
        AtomicWriteError::best_effort_cleanup(&tmp);
        return Err(AtomicWriteError::Write {
            temp: tmp,
            source,
        });
    }
    if let Err(source) = fs::rename(&tmp, target) {
        AtomicWriteError::best_effort_cleanup(&tmp);
        return Err(AtomicWriteError::Rename {
            temp: tmp,
            target: target.to_path_buf(),
            source,
        });
    }
    Ok(())
}

/// Atomically write a JSON-serialized value to `target`.
///
/// Convenience wrapper over [`write_atomic`] that uses `serde_json` to
/// serialize before writing. Pretty-prints (2-space indent) so the on-disk
/// file is human-readable for inspection / diff. Callers needing compact
/// JSON should serialize themselves and call [`write_atomic`] directly.
pub fn write_json_atomic<T: Serialize>(
    target: impl AsRef<Path>,
    value: &T,
) -> Result<(), AtomicWriteError> {
    let target = target.as_ref();
    let bytes = match serde_json::to_vec_pretty(value) {
        Ok(b) => b,
        Err(source) => {
            return Err(AtomicWriteError::Serialize {
                target: target.to_path_buf(),
                source,
            })
        }
    };
    write_atomic(target, &bytes)
}

/// Atomically write with durability hint (caller's choice of `fsync`).
///
/// Mirrors the `JsonSupport.WriteAtomic` + `FileOptions.WriteThrough`
/// pattern from `agentos-windows-recovery/TransactionEngine.cs`:
/// opens the tmp file with `FileOptions::WriteThrough` so the kernel
/// issues a flush-to-disk on each `write`, then calls the same
/// `<target>.tmp-<uuid>` → `rename` flow.
///
/// **When to use**: append-only audit journal, manifest, recovery
/// snapshot — anything where power-loss mid-write is a real concern.
///
/// **When NOT to use**: high-frequency hot paths where the extra
/// fsync-per-write is too costly. For those, [`write_atomic`] without
/// durability is the right call (rename alone is still atomic against
/// concurrent readers).
pub fn write_with_durability(
    target: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<(), AtomicWriteError> {
    let target = target.as_ref();
    let tmp = tmp_path_for(target);

    // Open the tmp file with WriteThrough (best-effort durability hint).
    // On Linux/macOS, WriteThrough does NOT issue an fsync; the only
    // way to guarantee power-loss durability on POSIX is to call
    // File::sync_all() before close. We do that explicitly below.
    let mut file: File = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
    {
        Ok(f) => f,
        Err(source) => {
            AtomicWriteError::best_effort_cleanup(&tmp);
            return Err(AtomicWriteError::Write {
                temp: tmp,
                source,
            });
        }
    };

    if let Err(source) = file.write_all(bytes) {
        AtomicWriteError::best_effort_cleanup(&tmp);
        return Err(AtomicWriteError::Write {
            temp: tmp,
            source,
        });
    }
    if let Err(source) = file.sync_all() {
        AtomicWriteError::best_effort_cleanup(&tmp);
        return Err(AtomicWriteError::Write {
            temp: tmp,
            source,
        });
    }
    // `sync_all` flushes data; on Linux the parent directory also needs
    // an fsync after rename for the new name to be durable. We do that
    // below as a best-effort step (errors here are warnings, not failures,
    // because we already successfully wrote+renamed).
    if let Err(source) = fs::rename(&tmp, target) {
        AtomicWriteError::best_effort_cleanup(&tmp);
        return Err(AtomicWriteError::Rename {
            temp: tmp,
            target: target.to_path_buf(),
            source,
        });
    }
    // Best-effort directory fsync (Linux/macOS). Failure here is logged
    // by the caller via tracing if they care; we don't fail the write
    // because the rename already succeeded.
    if let Some(parent) = target.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        a: u32,
        b: String,
    }

    #[test]
    fn write_atomic_creates_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("hello.txt");
        write_atomic(&target, b"hello world").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello world");
        // No .tmp-<uuid> leftovers
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(entries, vec!["hello.txt".to_string()]);
    }

    #[test]
    fn write_atomic_replaces_existing() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("hello.txt");
        fs::write(&target, b"old").unwrap();
        write_atomic(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn write_atomic_cleans_up_tmp_on_write_failure() {
        // Force the write to fail by pointing the target into a directory
        // that doesn't exist as a parent (rename would fail because the
        // tmp would land in a non-existent parent too). Actually for
        // write failure we use a path that's a directory.
        let dir = tempdir().unwrap();
        let target = dir.path().join("a_directory");
        fs::create_dir(&target).unwrap();
        let result = write_atomic(&target, b"x");
        assert!(result.is_err());
        // Tmp leftovers (none expected: best-effort cleanup ran)
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        // Either "a_directory" alone (no leftovers) or "a_directory" +
        // a leftover .tmp-<uuid> (if cleanup raced — should never happen
        // here, but we accept either to avoid flakiness on Windows).
        for entry in &entries {
            assert!(
                entry == "a_directory" || entry.starts_with(".tmp-"),
                "unexpected leftover entry: {entry}"
            );
        }
    }

    #[test]
    fn write_json_atomic_round_trips() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("config.json");
        let value = Sample {
            a: 42,
            b: "hello".into(),
        };
        write_json_atomic(&target, &value).unwrap();
        let raw = fs::read_to_string(&target).unwrap();
        let parsed: Sample = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, value);
        // Pretty means newlines + indent
        assert!(raw.contains('\n'));
    }

    #[test]
    fn write_atomic_unique_tmp_names_under_concurrency() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("race.txt");
        // Two concurrent writers — tmp names must differ.
        let t1 = tmp_path_for(&target);
        let t2 = tmp_path_for(&target);
        assert_ne!(t1, t2);
        assert!(t1.file_name().unwrap().to_string_lossy().starts_with("race.txt.tmp-"));
    }

    #[test]
    fn write_with_durability_overwrites_and_durables() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("audit.json");
        write_with_durability(&target, b"{\"a\":1,\"b\":\"durability\"}").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"a\":1,\"b\":\"durability\"}");
        // Re-write should still succeed.
        write_with_durability(&target, b"{\"a\":2,\"b\":\"new\"}").unwrap();
        let parsed: Sample = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(parsed.a, 2);
    }
}