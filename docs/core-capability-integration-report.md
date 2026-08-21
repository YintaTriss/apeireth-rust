# Apeireth Fresh Integration & Reconciliation — Final Report

> Autonomous integration session, 2026-08-20.
> Working repo: `apeireth-fresh` (origin = Jimmyxiao2009/apeireth-rust).
> Strategy doc: `docs/core-capability-integration-strategy.md`.

## 1. Git

| | |
|---|---|
| Starting feature | `feature/core-capability-expansion` |
| Feature HEAD | `b99ff8c2` (14 feature-only commits) |
| Starting master | `origin/master` |
| Master HEAD | `968b4ceb` (217 master-only commits) |
| Merge base | `91b2d2e0` |
| Integration branch | `integration/core-capability-reconcile` |
| Integration HEAD | `3131c0cd` (merge commit) + follow-up fix pending |
| git status | clean working tree |
| Pushed | **NO** (local-only) |

> Correction to original prompt assumption: `4d0ac12e` is **feature-only** (not on origin/master). The true common ancestor is `91b2d2e0`. `4d0ac12e` is the 3rd commit after the fork point on the feature side.

## 2. Divergence

- Feature-only commits: **14** (capability manifest / session / memory / permission / trace / desktop / migration hardening / tests / docs)
- Master-only commits: **217** = 129 docs + 24 test + 24 fix + 10 feat + Phase 0-9 desktop refactor + cron/CI/sandbox/companion_serve streaming
- Files changed both sides (C-bucket): **7** (2 Rust auto-merged clean, 5 frontend conflicted)

## 3. Chosen Strategy

**Hybrid — Option B (merge `origin/master` into feature) + frontend semantic reconciliation.**

Reasons:
- **Migration safety**: master added zero migrations → feature append-only V1-V7 canonical, no renumbering.
- **Commit separability**: backend zero conflict; merge auto-preserves all 14 feature commits.
- **Conflict count**: only 5 frontend files (not architectural-mutex — directory-relocation vs old-path edits).
- **Architecture overlap**: master did NOT reimplement feature capabilities (master "capability" hits are sandbox docs + llm-iface stub tests, unrelated to `/v1/apeireth/capabilities`).
- **Verification baseline**: feature's verified baseline is the floor; master fixes layered on top.
- **Rollback simplicity**: integration branch is local-only; no force push, no reset --hard.

Option A rejected (cherry-pick would conflict per-commit against Phase refactor, no rename detection). Option C forbidden (backend zero conflict does not meet the trigger condition).

## 4. Conflict Summary

| File / module | Conflict type | Resolution |
|---|---|---|
| `crates/apeireth-companion/src/lib.rs` | auto-merge | Clean (master sandbox module + fmt; feature module mounts — different regions) |
| `crates/apeireth-companion/examples/companion_serve.rs` | auto-merge | Clean (master TP34 streaming + fmt; feature integration) |
| `frontend/.../lib/runtime.ts` | content | Rewritten: feature canonical contract (capability manifest + V2 mutations + structured ToolCall) retained; master `subscribeCompanionEvents`/`CompanionPresentationState`/`chatOnce` fused in. Duplicate fetcher signatures resolved (feature's richer versions canonical; master names aliased). |
| `frontend/.../App.svelte` | content | Rewritten: feature inline shell (capability gating + RuntimeModal + structured tool calls) retained; master companion SSE + reasoning-delta + retry + pending-approvals polling fused in. Fixed 1 type error (`onclick={() => send()}`). |
| `frontend/.../lib/MemoryView.svelte` | modify/delete | Feature version retained (capability-gated forget/protect/update); PageHeader import path fixed. |
| `frontend/.../lib/MessageContent.svelte` | modify/delete | Feature version retained (ToolCallCard structured rendering); TaskCard/ExecutionTimeline import paths fixed. |
| `frontend/.../lib/ConversationsView.svelte` | content (master rename) | Feature version recreated at `src/lib/` (App.svelte import path); PageHeader import fixed. Master's `src/features/conversations/` duplicate removed. |

Component-layout decision: master relocated shared components `src/lib/{PageHeader,StatusDot,TaskCard,ExecutionTimeline}.svelte` → `src/components/`. Reconciliation adopted master's relocation as canonical and fixed all feature-view import paths. Removed 8 unused master duplicate views (`features/{chat,activity,tools,memory,settings,companion}/*`, `app/Sidebar`) to avoid dual source-of-truth; kept `features/quick/QuickWindowView` (used by `main.ts` quick-window mode).

## 5. Migration Reconciliation

- Old master migrations: none added (master untouched `crates/apeireth-memory/src/migrations.rs`).
- Feature migrations: V1-V4 (baseline) + V5 (sessions_lifecycle) + V6 (episode_governance) + V7 (agent_traces), append-only.
- Final migration order: **V1 → V2 → V3 → V4 → V5 → V6 → V7** (canonical, unchanged).
- Duplicate migration version: **NONE**.

Verified by `apeireth-memory` tests: `store_runs_at_least_one_migration`, `store_migration_ids_sorted_ascending`, `store_migration_ids_are_positive` all pass.

## 6. Capability Manifest

- Final endpoint: `GET /v1/apeireth/capabilities` (single canonical, no `/v1/capabilities` or `/v1/runtime/capabilities` duplicates).
- Schema version: 1 (CapabilityManifest with schema_version, runtime, capabilities[]).
- Compatibility: legacy fallback when runtime has no native endpoint (conservative read-only profile, no mutation speculation); null manifest gates everything off.
- Desktop gating: `capabilitySupported()` gates UI buttons (no 404-probing). Verified by `desktop-capability-gating.mjs` + `capability-manifest.mjs`.

## 7. Sessions

- Lifecycle: create / get / list / rename / archive / restore / close — full state machine. Verified by `session_lifecycle_integration.rs` (10/10 pass).
- Revision: optimistic CAS (`expected_rev`) — `session_revision_conflict_rename_race` pass.
- Persistence: reopen survives — `session_restart_persistence_reopen` pass.
- Legacy local sessions: `src/lib/ConversationsView.svelte` keeps local workspace tab + read-only backend ledger tab. Legacy client compat verified by `session_legacy_client_compat_old_upsert_readable`.
- Migration plan for full local→backend migration: **DEFERRED** (P1, per prompt §27 — not forced this round; local sessions preserved).

## 8. Memory

- Update: `updateMemoryEpisode` (governed, CAS revision) — verified.
- Forget: soft-delete, excluded from retrieval — `memory_forget_excludes_from_retrieval` pass. Forget ≠ purge.
- Protect: protected episodes resist forget (UI disables forget on protected) — verified.
- Graph integrity: no dangling refs (append-only episodes + governance sidecar).
- `memory_concurrent_revision_conflict` + `memory_legacy_episode_no_governance_default_active` pass.

## 9. Permissions

- Grant: `grantToolPermission` (master token, not persisted) — verified.
- Revoke: `revokeGrant` (immediate effect) — `packs.rs` Mutex, clean no-reentrancy.
- Expiry: GrantView carries expiry/expired/active.
- Policy write (`permissions.policy.write`): **DEFERRED WITH REASON** (P2, per prompt §28 — policy model not yet stable; manifest declares it unsupported).

## 10. Trace

- Persistence: `agent_traces` table (V7), root+children spans — `trace_root_and_children_persisted`, `trace_restart_persistence` pass.
- SSE: `subscribeCompanionEvents` (reconciled from master, exponential backoff reconnect) wired in App.svelte.
- Live verification: **NOT VERIFIED** (live runtime requires external `apikey-ultra.txt`; see §14).
- Raw CoT persisted: **NO** — `trace_no_raw_cot_stored` pass (critical invariant).

## 11. Desktop

- Capability gating: ✓ (views receive `capabilities` prop, gate mutation buttons).
- Secret storage: apiKey/masterToken NOT persisted (security audit §13).
- Tools: `ToolsView` (capability-gated grants + revoke + approval).
- Sessions: `ConversationsView` (local + backend ledger).
- Memory: `MemoryView` (forget/protect/update gated).
- Trace: `ActivityView` (trace tree, capability-gated trace link).
- Build: `svelte-check` 0 errors / `vite build` success (both app + quick-window chunks).

## 12. Upstream Fixes Preserved

Master's 217 commits preserved via merge (not overwritten by cherry-pick): 129 doc/README stale fixes, 24 per-crate integration test suites (cron/tool-*/host/acp/config/credentials/environment/llm-iface/telemetry/wiki/stock/etc.), CI workflows (companion-desktop-ci, pii-leak-detection, release-prep, 8-hard-wall gate), Makefile targets, Docker multi-arch, cron @-shorthand, rate-limiter retry, companion_serve TP34 streaming + extract_minimax_cot, sandbox Stage 1+2 microVM, Phase 0-9 desktop native shell. None overwritten — merge took feature's side only on the 5 conflicted frontend files, all others auto-merged from master.

## 13. Tests

| Command | Result |
|---|---|
| `cargo check --workspace` (85+ crates) | ✅ PASS (exit 0, 58s) |
| `cargo check -p apeireth-companion --example companion_serve` | ✅ PASS |
| `cargo test -p apeireth-memory` | ✅ PASS (migration + governance + trace + persistence) |
| `cargo test -p apeireth-companion --lib` | ✅ PASS (694 passed, 0 failed) |
| `cargo test -p apeireth-companion --test session_lifecycle_integration` | ✅ PASS (10/10) |
| `pnpm check` (svelte-check, 3419 files) | ✅ PASS (0 errors, 0 warnings) |
| `pnpm build` (vite) | ✅ PASS (8.17s, app + quick-window chunks) |
| `tests/capability-manifest.mjs` | ✅ PASS |
| `tests/desktop-capability-gating.mjs` | ✅ PASS |
| `tests/reality-check.mjs` | ✅ PASS |
| `tests/security-attack.mjs` | ✅ PASS |
| `tests/frontend-smoke.cjs` | ✅ PASS |

## 14. Live Smoke

| Dimension | Status |
|---|---|
| Capability | CONTRACT VERIFIED (tests) / live NOT VERIFIED |
| Session | CONTRACT VERIFIED (10 integration tests) / live NOT VERIFIED |
| Memory | CONTRACT VERIFIED (governance tests) / live NOT VERIFIED |
| Permission | CONTRACT VERIFIED (packs tests) / live NOT VERIFIED |
| Trace | CONTRACT VERIFIED (trace tests, no-raw-CoT) / live NOT VERIFIED |
| SSE | CONTRACT VERIFIED (subscribeCompanionEvents compiled + wired) / live NOT VERIFIED |
| Desktop | BUILD VERIFIED (svelte-check + vite build) / visual NOT VERIFIED |
| Visual (Playwright+Edge) | NOT VERIFIED |

**Live smoke blocked by**: `companion_serve` requires an external file `apikey-ultra.txt` (read via `std::fs::read_to_string("apikey-ultra.txt")`, line 391 of the example) which is not present in the repo (user-managed secret file). Per autonomous rules, no real secret access and no fabricated key file. Runtime binary builds cleanly; all contract/unit/integration tests pass; live HTTP probes deferred to user with the key file present.

## 15. Bugs Found During Integration

| Severity | Root cause | Fix | Regression test |
|---|---|---|---|
| Low | `App.svelte` `onclick={send}` passed MouseEvent to `send(customText?: string)` — type error surfaced by svelte-check after merge | Changed to `onclick={() => send()}` | svelte-check (0 errors) |

No backend bugs found — auto-merge was semantically clean (confirmed by 694 + 10 + memory tests).

### Human review of the 8 deleted master views (pre-merge gate)

Verified all 8 are genuine dead code — **no live entry lost**:
- 0 live imports reference any of the 8 deleted views in the reconciled tree.
- `main.ts` entry mounts only `App` + `features/quick/QuickWindowView` (kept).
- Cross-checked each deleted view against its feature-side counterpart: feature views are the canonical, working versions (capability-gated, richer).
- **Notable edge case**: master's `ChatView.svelte` imported `../tools/ApprovalDrawer.svelte`, but `src/features/tools/` **never existed on master** — the import was already a broken/dangling reference, so `ChatView` could not have compiled on master alone. Deleting it removed dead code, not a live "in-chat approval" entry. The working approval path is feature's `ToolsView` approval banner + `pendingApprovals` polling. No UX regression.

## 16. Remaining Issues

**P0**: none outstanding (all P0 invariants verified: migration, capability manifest, session/memory/permission schema, security, workspace build/tests).

**P1**:
- Trace SSE live validation (NOT VERIFIED — needs running backend with `apikey-ultra.txt`).
- Desktop visual smoke (Playwright+Edge, 1280x720 + 1920x1080) — NOT VERIFIED.
- Legacy local→backend session migration plan — DEFERRED (design only, not forced).

**P2**:
- `permissions.policy.write` — DEFERRED WITH REASON (policy model not yet stable).
- Future-incompat warnings on transitive deps `nom v1.2.4`, `proc-macro-error2` — pre-existing, unrelated to this merge.

## 17. Final Recommendation

**READY FOR USER REVIEW.**

- ✅ master upstream fixes preserved (217 commits, auto-merged)
- ✅ core capability preserved (14 commits, backend zero-conflict)
- ✅ migration not conflicting (V1-V7 canonical)
- ✅ Desktop builds (svelte-check 0 errors, vite build success)
- ✅ security not regressed (no secret persistence, no raw CoT)
- ✅ tests green (cargo workspace + 694 companion unit + 10 session integration + memory governance/trace + 5 frontend reality)
- ✅ history reviewable (merge commit + reconciled files, no squash, no destructive git)
- ✅ no destructive git, no force push, no rebase, no merge into master

**NOT auto-merged into master** — integration branch `integration/core-capability-reconcile` is local-only, awaiting user review. User should run live smoke with `apikey-ultra.txt` present, then decide on PR/merge into master.

A follow-up commit amends the `App.svelte` `onclick` fix + this report (the merge commit itself is already made at `3131c0cd`).
