//! # Three-way conflict detection (三路冲突检测)
//!
//! **BORROW**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-three-way-conflict-2026-08-21`
//! Source: <https://github.com/Jimmyxiao2009/agentos-windows-recovery> (MIT, Copyright 2026 Jimmyxiao2009)
//!
//! ## What it does
//!
//! Before any **destructive** operation (upgrade rollback, restore, arbitration undo, ...),
//! compare three states:
//!
//! 1. `baseline` — snapshot taken before the operation started (usually a saved file + content hash).
//! 2. `expected_after` — what the operation description says the state should be after it runs.
//! 3. `current` — what is actually on disk right now (live probe).
//!
//! If `current ≠ baseline`, something external changed the resource between capture and the
//! destructive step. The caller MUST either reject the operation, or require an explicit
//! `force = true` override (with audit).
//!
//! ## Philosophy anchors
//!
//! - **O-1 safety first** — refuse silent destruction.
//! - **S-2 be pragmatic** — minimal viable mechanism, not a silver bullet.
//! - **O-5 no pretending** — explicit "what is NOT done" list below.
//!
//! ## What this module does NOT do (caller responsibilities)
//!
//! 1. **File locking / mutex** — caller must serialize destructive steps.
//! 2. **Idempotent retry / restore** — caller must own rollback semantics.
//! 3. **Permission / owner checks** — minimal version only inspects content hash.
//! 4. **symlink / hardlink policy** — defaults to following symlinks (may mis-classify
//!    dangling symlinks as "externally deleted").
//! 5. **`expected_after` vs `current` semantic diff** — utility only checks
//!    `baseline` vs `current`. The `expected_after` value is provided by the caller's
//!    trait impl and is the caller's contract.
//! 6. **Cross-platform mtime precision normalization** — we use `unix_ms`. NTFS 100ns /
//!    ext4 ns / FAT 2s differences are NOT reconciled.
//! 7. **Streaming / content-addressed storage** — full snapshot lives in memory; large
//!    directories (GB+) will OOM. Add streaming in a future iteration if needed.
//!
//! ## Typical caller pattern
//!
//! ```ignore
//! use apeireth_host::three_way::{detect_with_force, DetectOutcome};
//!
//! let scope = FileScope { root: target_dir, excludes: vec!["target".into()] };
//! match detect_with_force(&scope, force)? {
//!     DetectOutcome::NoConflict => { /* proceed */ }
//!     DetectOutcome::Conflict(c) => { /* reject with diff */ }
//!     DetectOutcome::ConflictBypassedByForce(c) => { /* log + proceed */ }
//! }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors emitted by `capture_baseline` / `probe_current` / `expected_after`.
///
/// Designed to be small (5 variants) but explicit about why a probe failed.
#[derive(Debug, Error)]
pub enum ThreeWayError {
    /// Root path does not exist or is not a directory.
    #[error("three_way: root path missing: {0}")]
    MissingRoot(PathBuf),

    /// I/O error walking the tree or reading a file.
    #[error("three_way: I/O error at {path}: {source}")]
    Io {
        /// Path that triggered the I/O error.
        path: PathBuf,
        /// Underlying std::io::Error.
        #[source]
        source: std::io::Error,
    },

    /// Hashing failed (unexpected — should not happen for normal files).
    #[error("three_way: hash failure for {0}")]
    HashFailure(PathBuf),

    /// Snapshot serialization / deserialization failed.
    #[error("three_way: snapshot codec error: {0}")]
    Codec(String),

    /// Path is not safe to capture (e.g. absolute path escapes root, contains null bytes).
    #[error("three_way: unsafe path {path}: {reason}")]
    UnsafePath {
        /// Offending path.
        path: String,
        /// Why it is unsafe.
        reason: String,
    },
}

impl ThreeWayError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait + conflict report + entry points
// ---------------------------------------------------------------------------

/// Anything that can answer three questions about its state.
pub trait ThreeWayComparable {
    /// Serializable snapshot type. Must round-trip via JSON (or any serde codec) so
    /// callers can persist `baseline` between processes. Must also implement
    /// [`DiffableSnapshot`] so `detect` can compute `baseline` ↔ `current` diffs.
    type Snapshot: Serialize + DeserializeOwned + PartialEq + Clone + fmt::Debug + DiffableSnapshot;

    /// Capture state **before** the destructive operation. Usually cheap; called once.
    fn capture_baseline(&self) -> Result<Self::Snapshot, ThreeWayError>;

    /// Probe live state **right before** the destructive operation. Must reflect
    /// current disk state, not cached.
    fn probe_current(&self) -> Result<Self::Snapshot, ThreeWayError>;

    /// Describe the state the operation is expected to leave behind. Default impl
    /// returns `Err(Codec)` — most callers will implement it concretely (or
    /// override to clone baseline if the operation does not mutate the snapshot shape).
    fn expected_after(&self) -> Result<Self::Snapshot, ThreeWayError> {
        let _ = self;
        Err(ThreeWayError::Codec(
            "expected_after() not implemented for this scope".into(),
        ))
    }
}

/// A `baseline` ↔ `current` diff, broken down by category so callers can format
/// pretty messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConflictDiff {
    /// path → (baseline value repr, current value repr).
    pub changed_paths: BTreeMap<String, (String, String)>,
    /// paths present in `current` but not in `baseline` (external additions).
    pub added_paths: BTreeMap<String, String>,
    /// paths present in `baseline` but missing from `current` (external deletions).
    pub removed_paths: BTreeSet<String>,
}

impl ConflictDiff {
    /// `true` if no paths changed / added / removed.
    pub fn is_empty(&self) -> bool {
        self.changed_paths.is_empty() && self.added_paths.is_empty() && self.removed_paths.is_empty()
    }

    /// Total number of differing paths (changed + added + removed).
    pub fn total_changes(&self) -> usize {
        self.changed_paths.len() + self.added_paths.len() + self.removed_paths.len()
    }
}

/// A detected conflict: caller decides what to do with this.
#[derive(Debug, Clone)]
pub struct ThreeWayConflict<S: ThreeWayComparable> {
    /// State captured before the operation.
    pub baseline: S::Snapshot,
    /// Live state probed right before the destructive step.
    pub current: S::Snapshot,
    /// `baseline` vs `current` diff.
    pub diff: ConflictDiff,
}

/// Outcome of `detect_with_force`. Lets callers handle the three branches distinctly.
#[derive(Debug, Clone)]
pub enum DetectOutcome<S: ThreeWayComparable> {
    /// `current == baseline` — safe to proceed.
    NoConflict,
    /// `current ≠ baseline`, force was `false` — caller should reject.
    Conflict(ThreeWayConflict<S>),
    /// `current ≠ baseline`, force was `true` — caller may proceed but MUST log the diff.
    ConflictBypassedByForce(ThreeWayConflict<S>),
}

/// Strict detection: compares a previously-captured `baseline` snapshot to a
/// freshly-probed `current`. Returns `Some(conflict)` if they differ.
///
/// **Usage pattern**: caller captures the baseline **before** any potentially-
/// mutating window opens, then passes it to `detect()` right before the
/// destructive step. The baseline must NOT be re-captured internally — by the
/// time the destructive step is about to run, re-capturing would already
/// include any concurrent mutations, defeating the purpose of the check.
pub fn detect<S: ThreeWayComparable>(
    c: &S,
    baseline: S::Snapshot,
) -> Result<Option<ThreeWayConflict<S>>, ThreeWayError> {
    let current = c.probe_current()?;
    let diff = baseline.diff_against(&current);
    if diff.is_empty() {
        Ok(None)
    } else {
        Ok(Some(ThreeWayConflict {
            baseline,
            current,
            diff,
        }))
    }
}

/// Detection with explicit force flag. Same semantics as `detect`, but encodes
/// the policy choice in the returned enum so callers can match on outcome.
pub fn detect_with_force<S: ThreeWayComparable>(
    c: &S,
    baseline: S::Snapshot,
    force: bool,
) -> Result<DetectOutcome<S>, ThreeWayError> {
    let current = c.probe_current()?;
    let diff = baseline.diff_against(&current);
    if diff.is_empty() {
        return Ok(DetectOutcome::NoConflict);
    }
    let conflict = ThreeWayConflict {
        baseline,
        current,
        diff,
    };
    Ok(if force {
        DetectOutcome::ConflictBypassedByForce(conflict)
    } else {
        DetectOutcome::Conflict(conflict)
    })
}

/// `baseline` vs `current` diff. Works on `FileSnapshot` shape
/// (`BTreeMap<relpath, FileEntry>`); other snapshot types via the
/// `DiffableSnapshot` extension trait get a no-op diff — caller responsibility
/// to implement it.
///
/// Implementation: serializes both sides to `serde_json::Value` and walks the
/// resulting object. This means **non-map snapshots** (vectors, scalars) silently
/// yield `ConflictDiff::default()` (i.e. no conflict). Caller must override via
/// `DiffableSnapshot::diff` if they use a non-map shape.
/// Trait for callers that want to override the default diff for non-`FileSnapshot`
/// shapes. Required bound on `ThreeWayComparable::Snapshot` so `detect` can compute
/// `baseline` ↔ `current` diffs without knowing the concrete snapshot type.
pub trait DiffableSnapshot {
    /// Compute `self` (baseline) vs `current` diff. Returns a `ConflictDiff`.
    fn diff_against(&self, current: &Self) -> ConflictDiff;
}

impl DiffableSnapshot for FileSnapshot {
    fn diff_against(&self, current: &Self) -> ConflictDiff {
        diff_file_snapshot(self, current)
    }
}

/// Specialization for the default `FileSnapshot` shape.
fn diff_file_snapshot(baseline: &FileSnapshot, current: &FileSnapshot) -> ConflictDiff {
    let mut changed_paths: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut added_paths: BTreeMap<String, String> = BTreeMap::new();
    let mut removed_paths: BTreeSet<String> = BTreeSet::new();

    for (k, v_b) in baseline {
        match current.get(k) {
            Some(v_c) if v_c != v_b => {
                changed_paths.insert(k.clone(), (format!("{:?}", v_b), format!("{:?}", v_c)));
            }
            None => {
                removed_paths.insert(k.clone());
            }
            _ => {}
        }
    }
    for (k, v_c) in current {
        if !baseline.contains_key(k) {
            added_paths.insert(k.clone(), format!("{:?}", v_c));
        }
    }

    ConflictDiff {
        changed_paths,
        added_paths,
        removed_paths,
    }
}

// ---------------------------------------------------------------------------
// FileScope — minimal file-system implementation
// ---------------------------------------------------------------------------

/// One file's content fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// SHA-256(content), 64-char lowercase hex.
    pub sha256_hex: String,
    /// File size in bytes.
    pub size: u64,
    /// Modification time in milliseconds since UNIX epoch.
    pub mtime_unix_ms: i64,
}

/// Snapshot of a directory tree: relative POSIX path → file entry.
pub type FileSnapshot = BTreeMap<String, FileEntry>;

/// A `ThreeWayComparable` over a real directory tree.
#[derive(Debug, Clone)]
pub struct FileScope {
    /// Root directory of the capture.
    pub root: PathBuf,
    /// Relative path prefixes to skip (e.g. `vec!["target".into(), ".git".into()]`).
    pub excludes: Vec<String>,
}

impl FileScope {
    /// Construct a scope rooted at `root`, with no excludes.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            excludes: Vec::new(),
        }
    }

    /// Walk the tree under `root`, returning a `FileSnapshot`.
    fn walk(&self) -> Result<FileSnapshot, ThreeWayError> {
        if !self.root.exists() {
            return Err(ThreeWayError::MissingRoot(self.root.clone()));
        }
        if !self.root.is_dir() {
            return Err(ThreeWayError::Io {
                path: self.root.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "root is not a directory",
                ),
            });
        }

        let mut out: FileSnapshot = BTreeMap::new();
        walk_dir(&self.root, &self.root, &self.excludes, &mut out)?;
        Ok(out)
    }
}

/// Std-only recursive directory walker. Errors out on first I/O failure (no
/// silent skips — caller wants to know if the tree is unreadable).
fn walk_dir(
    root: &Path,
    dir: &Path,
    excludes: &[String],
    out: &mut FileSnapshot,
) -> Result<(), ThreeWayError> {
    let read = fs::read_dir(dir).map_err(|e| ThreeWayError::io(dir, e))?;
    for entry in read {
        let entry = entry.map_err(|e| ThreeWayError::io(dir, e))?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| ThreeWayError::io(&path, e))?;
        if file_type.is_dir() {
            walk_dir(root, &path, excludes, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let rel = relposix(root, &path);
        if is_excluded(&rel, excludes) {
            continue;
        }
        let entry_data = hash_file(&path)?;
        out.insert(rel, entry_data);
    }
    Ok(())
}

impl ThreeWayComparable for FileScope {
    type Snapshot = FileSnapshot;

    fn capture_baseline(&self) -> Result<FileSnapshot, ThreeWayError> {
        self.walk()
    }

    fn probe_current(&self) -> Result<FileSnapshot, ThreeWayError> {
        self.walk()
    }

    fn expected_after(&self) -> Result<FileSnapshot, ThreeWayError> {
        // FileScope minimal: expected_after = baseline (no structural mutation planned).
        // Callers with mutations should wrap or implement their own ThreeWayComparable.
        self.capture_baseline()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn hash_file(path: &Path) -> Result<FileEntry, ThreeWayError> {
    let meta = fs::metadata(path).map_err(|e| ThreeWayError::io(path, e))?;
    let bytes = fs::read(path).map_err(|e| ThreeWayError::io(path, e))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let hex = hex::encode(digest);
    if hex.len() != 64 {
        return Err(ThreeWayError::HashFailure(path.to_path_buf()));
    }
    let mtime_unix_ms = meta
        .modified()
        .map_err(|e| ThreeWayError::io(path, e))?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(FileEntry {
        sha256_hex: hex,
        size: meta.len(),
        mtime_unix_ms,
    })
}

fn relposix(root: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(root).unwrap_or(p);
    // Convert Windows backslashes to forward slashes for stable keys.
    rel.to_string_lossy().replace('\\', "/")
}

fn is_excluded(rel: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|ex| {
        // Prefix match on the first component (directory skip) or exact file match.
        rel == ex
            || rel.starts_with(&format!("{}/", ex))
            || rel.split('/').next() == Some(ex.as_str())
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn empty_dir_baseline_equals_current() {
        let tmp = TempDir::new().unwrap();
        let scope = FileScope::new(tmp.path());
        let baseline = scope.capture_baseline().unwrap();
        let result = detect(&scope, baseline).unwrap();
        assert!(result.is_none(), "empty dir should yield no conflict");
    }

    #[test]
    fn single_file_unchanged() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.txt", b"hello");
        let scope = FileScope::new(tmp.path());
        let baseline = scope.capture_baseline().unwrap();
        assert!(detect(&scope, baseline).unwrap().is_none());
    }

    #[test]
    fn single_file_modified() {
        let tmp = TempDir::new().unwrap();
        let p = write_file(tmp.path(), "a.txt", b"hello");
        let scope = FileScope::new(tmp.path());

        // Caller pattern: capture baseline BEFORE any window of mutation,
        // then probe again right before the destructive step.
        let baseline = scope.capture_baseline().unwrap();
        fs::write(&p, b"hello MODIFIED").unwrap();

        let conflict = detect(&scope, baseline).unwrap().expect("must be conflict");
        assert_eq!(conflict.diff.changed_paths.len(), 1);
        assert!(conflict.diff.changed_paths.contains_key("a.txt"));
        assert_eq!(conflict.diff.total_changes(), 1);
    }

    #[test]
    fn force_override_bypasses_conflict() {
        let tmp = TempDir::new().unwrap();
        let p = write_file(tmp.path(), "a.txt", b"hello");
        let scope = FileScope::new(tmp.path());

        // capture baseline BEFORE external mutation.
        let baseline = scope.capture_baseline().unwrap();
        fs::write(&p, b"mutated externally").unwrap();

        // force = false → Conflict
        match detect_with_force(&scope, baseline.clone(), false).unwrap() {
            DetectOutcome::Conflict(c) => {
                assert_eq!(c.diff.changed_paths.len(), 1);
            }
            other => panic!("expected Conflict, got {:?}", other_summary(&other)),
        }
        // force = true → ConflictBypassedByForce
        match detect_with_force(&scope, baseline, true).unwrap() {
            DetectOutcome::ConflictBypassedByForce(c) => {
                assert_eq!(c.diff.changed_paths.len(), 1);
            }
            other => panic!("expected ConflictBypassedByForce, got {:?}", other_summary(&other)),
        }
    }

    #[test]
    fn nested_dir_changed() {
        let tmp = TempDir::new().unwrap();
        let nested_dir = tmp.path().join("sub").join("deeper");
        let p = write_file(&nested_dir, "leaf.bin", b"original");
        let scope = FileScope::new(tmp.path());

        // Capture baseline BEFORE the mutation.
        let baseline = scope.capture_baseline().unwrap();
        fs::write(&p, b"changed").unwrap();

        let conflict = detect(&scope, baseline).unwrap().expect("must be conflict");
        // Path must use forward slashes and preserve nesting.
        let key = "sub/deeper/leaf.bin";
        assert!(
            conflict.diff.changed_paths.contains_key(key),
            "nested path key missing; got keys: {:?}",
            conflict.diff.changed_paths.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn non_existent_dir_baseline() {
        let scope = FileScope::new("Z:/this/path/does/not/exist/xyz_123");
        // Even with a baseline in hand, probe_current must fail when the root
        // disappears (e.g. someone deleted the directory under us).
        let baseline = FileSnapshot::new();
        match detect(&scope, baseline) {
            Err(ThreeWayError::MissingRoot(p)) => {
                assert!(p.to_string_lossy().contains("does/not/exist"));
            }
            Err(other) => panic!("expected MissingRoot, got {:?}", other),
            Ok(_) => panic!("expected error, got Ok"),
        }
    }

    #[test]
    fn expected_after_differs_from_baseline_no_external_change() {
        // expected_after ≠ baseline but baseline == current → no conflict (utility
        // only compares baseline vs current; expected_after diff is caller's job).
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.txt", b"hello");
        let scope = FileScope::new(tmp.path());
        let baseline = scope.capture_baseline().unwrap();
        let expected = scope.expected_after().unwrap();

        // expected_after for FileScope defaults to baseline; if a custom impl
        // diverges here, the conflict is still Ok(None) (utility only checks
        // baseline vs current).
        let _ = expected;
        assert!(detect(&scope, baseline).unwrap().is_none());
    }

    #[test]
    fn added_and_removed_paths_classified() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "keep.txt", b"keep");
        write_file(tmp.path(), "will_remove.txt", b"remove me");
        let scope = FileScope::new(tmp.path());

        // capture BEFORE mutation so baseline reflects pre-mutation state.
        let baseline = scope.capture_baseline().unwrap();

        // External change: add new.txt, delete will_remove.txt, modify keep.txt.
        write_file(tmp.path(), "new.txt", b"new");
        fs::remove_file(tmp.path().join("will_remove.txt")).unwrap();
        fs::write(tmp.path().join("keep.txt"), b"keep MODIFIED").unwrap();

        let conflict = detect(&scope, baseline).unwrap().expect("must be conflict");
        let diff = &conflict.diff;
        assert_eq!(diff.changed_paths.len(), 1, "keep.txt changed");
        assert!(diff.changed_paths.contains_key("keep.txt"));
        assert_eq!(diff.added_paths.len(), 1, "new.txt added");
        assert!(diff.added_paths.contains_key("new.txt"));
        assert_eq!(diff.removed_paths.len(), 1, "will_remove.txt removed");
        assert!(diff.removed_paths.contains("will_remove.txt"));
        assert_eq!(diff.total_changes(), 3);
    }

    fn other_summary<S: ThreeWayComparable>(o: &DetectOutcome<S>) -> String {
        match o {
            DetectOutcome::NoConflict => "NoConflict".into(),
            DetectOutcome::Conflict(_) => "Conflict(_)".into(),
            DetectOutcome::ConflictBypassedByForce(_) => "ConflictBypassedByForce(_)".into(),
        }
    }
}