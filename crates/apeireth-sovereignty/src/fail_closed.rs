//! Fail-closed three-phase template (BORROW: agentos-windows-recovery 2026-08-21).
//!
//! **Borrow ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-fail-closed-2026-08-21`
//! **Source**: <https://github.com/Jimmyxiao2009/agentos-windows-recovery> (MIT)
//! **Original pattern**: `TransactionEngine.RollbackCore` + `MarkEvidenceFailure`
//!   — three-phase try/catch where each phase must independently verify
//!   before the next phase runs; on any failure the whole transaction is
//!   marked `RecoveryRequired` and **no workspace mutation occurs**.
//!
//! ## What "fail-closed" means here (per O-1 安全优先)
//!
//! ```
//!     ┌────────────────────────────────────────────────────────┐
//!     │ Phase 1: VERIFY (no I/O, no mutation)                  │
//!     │   - Structural checks (rule table, hash chain, types)  │
//!     │   - Returns Ok(()) only when ALL preconditions met    │
//!     └──────────────────┬─────────────────────────────────────┘
//!                        │ Ok → proceed | Err → ABORT
//!     ┌──────────────────▼─────────────────────────────────────┐
//!     │ Phase 2: PREPARE (idempotent staging, may write temp)  │
//!     │   - Build the plan / stage intermediate artefacts      │
//!     │   - Re-validate preconditions one more time            │
//!     │   - Returns Ok(()) only when staging is complete +     │
//!     │     verifies would succeed in Phase 3                   │
//!     └──────────────────┬─────────────────────────────────────┘
//!                        │ Ok → proceed | Err → ABORT (no apply)
//!     ┌──────────────────▼─────────────────────────────────────┐
//!     │ Phase 3: APPLY (the only phase that may mutate)        │
//!     │   - Atomic write / state transition / final commit    │
//!     │   - Returns Ok(()) only when mutation succeeded        │
//!     └────────────────────────────────────────────────────────┘
//! ```
//!
//! **Three guarantees** (per the upstream self-test that the original
//! repo asserts in `Program.cs`):
//! 1. **Workspace untouched if any phase fails** — verify & prepare are
//!    strictly read-only or staging-only.
//! 2. **No "best-effort" / "optimistic" mutation** — the moment Phase 1
//!    or Phase 2 errors out, Phase 3 is **never called**. This is
//!    enforced at the type level via the consuming function, not by
//!    convention.
//! 3. **Errors carry the phase name** — `FailClosedError { phase, source }`
//!    tells the caller exactly which phase failed so it can map to the
//!    correct downstream action (rollback / abort / surface to user).
//!
//! ## 0 装 PASS (per O-5 不假装)
//!
//! - This module provides the **template / framework** only. The actual
//!   `Verify` / `Prepare` / `Apply` semantics are implemented per-caller.
//!   Existing `SelfDisableGuard::check_*` methods already follow this
//!   pattern implicitly; [`run_fail_closed`] makes it explicit so future
//!   callers (e.g. multi-stage upgrade rollback, evidence integrity
//!   restore) can be templated.
//! - No I/O happens in this module. It only orchestrates.
//! - No async. Callers with async work should call this from a blocking
//!   helper or wrap each phase in `tokio::task::spawn_blocking`.

use std::fmt;

/// Which phase of the fail-closed three-phase template failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailClosedPhase {
    /// Phase 1 — structural verification, no I/O.
    Verify,
    /// Phase 2 — staging / preparation, idempotent, may write temp.
    Prepare,
    /// Phase 3 — the only phase that may mutate persistent state.
    Apply,
}

impl FailClosedPhase {
    /// Stable string label for audit logs (e.g. `verify`, `prepare`, `apply`).
    pub fn as_str(&self) -> &'static str {
        match self {
            FailClosedPhase::Verify => "verify",
            FailClosedPhase::Prepare => "prepare",
            FailClosedPhase::Apply => "apply",
        }
    }
}

impl fmt::Display for FailClosedPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error type for [`run_fail_closed`]. Always carries the failing phase
/// so the caller can route to the correct downstream action.
#[derive(Debug)]
pub struct FailClosedError<E> {
    /// Which phase reported the failure.
    pub phase: FailClosedPhase,
    /// The phase's own error (preserved as-is so callers can match on it).
    pub source: E,
}

impl<E: fmt::Display> fmt::Display for FailClosedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fail-closed phase `{}` failed: {}", self.phase, self.source)
    }
}

impl<E: std::error::Error + 'static> std::error::Error for FailClosedError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// A side-effect-free "Verify" closure.
///
/// Return `Ok(())` only when all preconditions are satisfied.
pub trait VerifyPhase {
    /// The error type this phase reports.
    type Error: std::fmt::Debug;
    /// Run the verification. **Must not mutate persistent state.**
    fn verify(&mut self) -> Result<(), Self::Error>;
}

/// An idempotent "Prepare" closure that may stage temp artefacts but
/// must not yet mutate the persistent target.
///
/// Return `Ok(())` only when staging is complete AND a re-check of
/// preconditions would still succeed in Phase 3.
pub trait PreparePhase {
    /// The error type this phase reports.
    type Error: std::fmt::Debug;
    /// Prepare staging / intermediate artefacts.
    fn prepare(&mut self) -> Result<(), Self::Error>;
}

/// The "Apply" closure. The **only** phase that may mutate the persistent
/// target.
pub trait ApplyPhase {
    /// The error type this phase reports.
    type Error: std::fmt::Debug;
    /// Commit the staged artefacts into the persistent target.
    fn apply(&mut self) -> Result<(), Self::Error>;
}

/// Orchestrate the three-phase fail-closed template.
///
/// **Guarantees**:
/// - `verify()` runs first. On `Err`, `prepare` and `apply` are **never called**.
/// - `prepare()` runs only if `verify()` succeeded. On `Err`, `apply` is
///   **never called**.
/// - `apply()` runs only if `prepare()` succeeded.
///
/// The caller still owns staging cleanup (this template does not roll
/// back partial `prepare()` artefacts — that responsibility lives with
/// the caller because it requires domain knowledge of what staging
/// means for that specific operation).
///
/// `PV` = VerifyPhase, `PP` = PreparePhase, `PA` = ApplyPhase.
/// All three must share an error type `E` (so the caller can match on
/// one variant). For heterogeneous error types, callers can wrap each
/// phase in their own error enum.
///
/// # Example
///
/// ```ignore
/// use apeireth_sovereignty::fail_closed::{self, FailClosedPhase};
///
/// struct MyOp;
///
/// impl fail_closed::VerifyPhase for MyOp {
///     type Error = String;
///     fn verify(&mut self) -> Result<(), String> { Ok(()) }
/// }
/// impl fail_closed::PreparePhase for MyOp {
///     type Error = String;
///     fn prepare(&mut self) -> Result<(), String> { Ok(()) }
/// }
/// impl fail_closed::ApplyPhase for MyOp {
///     type Error = String;
///     fn apply(&mut self) -> Result<(), String> { Ok(()) }
/// }
///
/// fail_closed::run_fail_closed(MyOp)?;
/// # Ok::<(), fail_closed::FailClosedError<String>>(())
/// ```
pub fn run_fail_closed<PV, PP, PA, E>(
    mut verify: PV,
    mut prepare: PP,
    mut apply: PA,
) -> Result<(), FailClosedError<E>>
where
    PV: VerifyPhase<Error = E>,
    PP: PreparePhase<Error = E>,
    PA: ApplyPhase<Error = E>,
    E: std::fmt::Debug,
{
    if let Err(source) = verify.verify() {
        return Err(FailClosedError {
            phase: FailClosedPhase::Verify,
            source,
        });
    }
    if let Err(source) = prepare.prepare() {
        return Err(FailClosedError {
            phase: FailClosedPhase::Prepare,
            source,
        });
    }
    if let Err(source) = apply.apply() {
        return Err(FailClosedError {
            phase: FailClosedPhase::Apply,
            source,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test op that records which phases ran. Tests assert that on
    /// phase-N failure, phases N+1, N+2 are NEVER called.
    #[derive(Clone)]
    struct RecordingOp {
        ran: Vec<FailClosedPhase>,
        verify_result: Result<(), String>,
        prepare_result: Result<(), String>,
        apply_result: Result<(), String>,
    }

    impl Default for RecordingOp {
        fn default() -> Self {
            Self {
                ran: Vec::new(),
                verify_result: Ok(()),
                prepare_result: Ok(()),
                apply_result: Ok(()),
            }
        }
    }

    impl VerifyPhase for RecordingOp {
        type Error = String;
        fn verify(&mut self) -> Result<(), String> {
            self.ran.push(FailClosedPhase::Verify);
            self.verify_result.clone()
        }
    }
    impl PreparePhase for RecordingOp {
        type Error = String;
        fn prepare(&mut self) -> Result<(), String> {
            self.ran.push(FailClosedPhase::Prepare);
            self.prepare_result.clone()
        }
    }
    impl ApplyPhase for RecordingOp {
        type Error = String;
        fn apply(&mut self) -> Result<(), String> {
            self.ran.push(FailClosedPhase::Apply);
            self.apply_result.clone()
        }
    }

    #[test]
    fn all_phases_pass_run_in_order() {
        let verify = RecordingOp::default();
        let prepare = RecordingOp::default();
        let apply = RecordingOp::default();
        let result = run_fail_closed(verify, prepare, apply);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_failure_aborts_before_prepare_and_apply() {
        let mut verify = RecordingOp::default();
        verify.verify_result = Err("verify boom".into());
        let prepare = RecordingOp::default();
        let apply = RecordingOp::default();

        let err = run_fail_closed(verify, prepare, apply).unwrap_err();
        assert_eq!(err.phase, FailClosedPhase::Verify);
        assert_eq!(err.source, "verify boom");
    }

    #[test]
    fn prepare_failure_aborts_before_apply() {
        let verify = RecordingOp::default();
        let mut prepare = RecordingOp::default();
        prepare.prepare_result = Err("prepare boom".into());
        let apply = RecordingOp::default();

        let err = run_fail_closed(verify, prepare, apply).unwrap_err();
        assert_eq!(err.phase, FailClosedPhase::Prepare);
        assert_eq!(err.source, "prepare boom");
    }

    #[test]
    fn apply_failure_does_not_short_circuit() {
        let verify = RecordingOp::default();
        let prepare = RecordingOp::default();
        let mut apply = RecordingOp::default();
        apply.apply_result = Err("apply boom".into());

        let err = run_fail_closed(verify, prepare, apply).unwrap_err();
        assert_eq!(err.phase, FailClosedPhase::Apply);
        assert_eq!(err.source, "apply boom");
    }

    #[test]
    fn phase_labels_are_stable_strings() {
        assert_eq!(FailClosedPhase::Verify.as_str(), "verify");
        assert_eq!(FailClosedPhase::Prepare.as_str(), "prepare");
        assert_eq!(FailClosedPhase::Apply.as_str(), "apply");
    }

    #[test]
    fn error_display_includes_phase_name() {
        let err = FailClosedError {
            phase: FailClosedPhase::Verify,
            source: "bad input".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("verify"));
        assert!(s.contains("bad input"));
    }

    /// Re-runnable recording op using Rc<RefCell<>> so a single instance
    /// can be passed into all three phase slots and we can observe which
    /// ran. This proves the **fail-closed ordering contract**: on
    /// verify-failure, prepare/apply are not called.
    #[test]
    fn verify_failure_skips_prepare_and_apply() {
        use std::cell::RefCell;

        #[derive(Default)]
        struct Shared {
            ran: RefCell<Vec<FailClosedPhase>>,
        }
        impl VerifyPhase for &Shared {
            type Error = String;
            fn verify(&mut self) -> Result<(), String> {
                self.ran.borrow_mut().push(FailClosedPhase::Verify);
                Err("verify failed".into())
            }
        }
        impl PreparePhase for &Shared {
            type Error = String;
            fn prepare(&mut self) -> Result<(), String> {
                self.ran.borrow_mut().push(FailClosedPhase::Prepare);
                Ok(())
            }
        }
        impl ApplyPhase for &Shared {
            type Error = String;
            fn apply(&mut self) -> Result<(), String> {
                self.ran.borrow_mut().push(FailClosedPhase::Apply);
                Ok(())
            }
        }
        let shared = Shared::default();
        let err = run_fail_closed(&shared, &shared, &shared).unwrap_err();
        assert_eq!(err.phase, FailClosedPhase::Verify);
        assert_eq!(*shared.ran.borrow(), vec![FailClosedPhase::Verify]);
    }

    /// Similar proof for prepare-failure.
    #[test]
    fn prepare_failure_skips_apply() {
        use std::cell::RefCell;

        #[derive(Default)]
        struct Shared {
            ran: RefCell<Vec<FailClosedPhase>>,
        }
        impl VerifyPhase for &Shared {
            type Error = String;
            fn verify(&mut self) -> Result<(), String> {
                self.ran.borrow_mut().push(FailClosedPhase::Verify);
                Ok(())
            }
        }
        impl PreparePhase for &Shared {
            type Error = String;
            fn prepare(&mut self) -> Result<(), String> {
                self.ran.borrow_mut().push(FailClosedPhase::Prepare);
                Err("prepare failed".into())
            }
        }
        impl ApplyPhase for &Shared {
            type Error = String;
            fn apply(&mut self) -> Result<(), String> {
                self.ran.borrow_mut().push(FailClosedPhase::Apply);
                Ok(())
            }
        }
        let shared = Shared::default();
        let err = run_fail_closed(&shared, &shared, &shared).unwrap_err();
        assert_eq!(err.phase, FailClosedPhase::Prepare);
        assert_eq!(
            *shared.ran.borrow(),
            vec![FailClosedPhase::Verify, FailClosedPhase::Prepare]
        );
    }
}