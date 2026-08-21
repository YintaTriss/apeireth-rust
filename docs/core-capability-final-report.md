# Apeireth Core Capability Expansion — Final Report

## 1. Git
- Branch: `feature/core-capability-expansion`
- HEAD: `8f610dd4` (after final doc commit)
- Commits: 10 this session (43044504..8f610dd4)
- git status: clean (working tree clean, all pushed)

## 2. Baseline
- Starting SHA: `4d0ac12e` (integration/pattern-fresh, UI 验收基线)
- Ending SHA: `8f610dd4`
- origin/feature/core-capability-expansion synced with local HEAD.

## 3. Capability Manifest
- Endpoint: `GET /v1/apeireth/capabilities`
- Schema version: 1
- Supported capabilities: chat.completions, health, models.list, sessions.{read,create,rename,archive,restore,close}, memory.{read,append,update,forget,protect,unprotect}, tools.{list,invoke}, permissions.{requests.read,grant,revoke,grants.read,policy.read}, activity.{sse,audit}, trace.{read,subscribe}
- Unsupported (honest): permissions.policy.write (持久化策略, 未实现)
- Legacy profile: conservative, read/chat only.

## 4. Sessions
- Implemented: create/get/list/rename/archive/restore/close (state machine + expected_rev CAS).
- Persistence: V5 migration, restart-preserved (tested).
- Migration: legacy 4-column sessions readable (NULL→active/rev0/global, zero migration).
- Known limitations: no hard delete (archive/close tombstone only); local localStorage sessions not force-migrated to backend (schema/safety undecided, legacy preserved).

## 5. Memory
- Update: content_override (provenance intact, original readable). expected_rev CAS.
- Forget: soft-delete (status=forgotten), excluded from governed retrieval. != purge.
- Protect: blocks normal forget; unprotect required.
- Graph integrity: factg-*/link-* filtered from governed retrieval when forgotten, no dangling pointer.

## 6. Permissions
- Grant: PermissionPack (Permanent/Hours/SingleUse, budget, paths, sandbox). returns grant_id.
- Scope: tool-scoped (tools/paths). Duration via expiry enum.
- Expiry: is_expired boundary tested. 90-day renewal reminder.
- Revoke: immediate effect (next evaluate). master-token gated.
- Policy: evaluate (deterministic Allow/Deny/RequireApproval). policy.write not implemented.

## 7. Agent Trace
- Trace model: TraceSpan (trace_id/span_id/parent_span_id tree).
- Span model: kind (7) + actor + status (5) + redacted attributes.
- Persistence: V7 agent_traces table (append-only), query API.
- SSE: span events via existing broadcast (type=trace, backward compat).
- Redaction: recursive key+value-prefix scrubbing (tested).
- Raw CoT storage: **NO** (summary_is_safe rejects CoT markers).

## 8. Desktop Integration
- 真实接入: capability gating (all buttons gate on manifest), Memory forget/protect/unprotect (ConfirmDialog for forget), Tools grants list/revoke, Activity trace-link→span-tree, RuntimeModal capability info, session/memory/permission/trace fetchers wired.
- 仍 read-only: local localStorage sessions remain primary (backend mutation endpoints wired but not force-switched); permissions.policy.write unsupported (no UI).

## 9. Security Audit
- API Key storage: NOT persisted (in-memory only).
- Master Token storage: NOT persisted (request param, cleared after use).
- Trace secret redaction: YES (recursive, tested with tool-args injection attack).
- Audit secret redaction: YES (GrantView/manifest secret-free; existing PrivacyGuard for tool-call records).
- Credential baseline maintained, no regression.

## 10. Migration
- SQLite migration: V5 (sessions lifecycle) / V6 (episode_governance) / V7 (agent_traces). Append-only, nullable, idempotent, tested (fresh + reopen + legacy).
- LocalStorage migration: loadConfig purges legacy apiKey/masterToken; conversations preserved.
- Legacy runtime compatibility: new frontend + old runtime → legacy profile (no white screen); old frontend + new runtime → existing endpoints preserved.

## 11. Tests
- `cargo test -p apeireth-memory --lib` (agent_trace + memory_governance + session_lifecycle + migrations): 34 passed.
- `cargo test -p apeireth-companion --lib` (runtime_capabilities + agent_trace + packs): 33 passed.
- `cargo test -p apeireth-companion --test session_lifecycle_integration`: 10 passed.
- `node tests/capability-manifest.mjs`: 7 passed.
- `node tests/desktop-capability-gating.mjs`: 5 passed.
- `node tests/security-attack.mjs`: 5 passed.
- `node tests/reality-check.mjs`: 5 passed (baseline maintained).
- `cargo check --workspace`: PASS.
- `pnpm check`: 0 err / 0 warn. `pnpm build`: PASS.
- Total: 77 Rust + 22 frontend tests, all green.

## 12. Live Smoke
- Read: LIVE READ VERIFIED (health, capabilities, panel sessions/memory/approvals/audit, grants, evaluate, traces).
- Mutation: LIVE MUTATION VERIFIED (session create→rename→archive→restore + revision conflict 409; grants list/evaluate; trace list). Isolated temp DB.
- Frontend: VISUAL VERIFIED (Playwright+Edge: Desktop loads, all 6 views render, RuntimeModal opens, no white screen, no fatal JS errors). CORS-only failures are test-harness artifacts.
- Visual: VISUAL VERIFIED (via Playwright, not HTTP-200-faked).

## 13. Bugs Found During Reality Check
1. **list_recent_traces deadlock** (severity: high). Root cause: `list_recent_traces` called `self.list_trace_spans()` while holding the `Mutex<Connection>` guard (non-reentrant → deadlock, test hung >60s). Fix: rewrote to do all work in one conn with windowed queries. Regression test: `trace_list_recent_traces` (0.07s).
2. **Manifest test over-strict** (severity: low). Root cause: `capability_ids_are_stable_dotted_strings` required a `.` in all IDs, but `health` is a valid single-word root capability. Fix: relaxed to allow lowercase/digit/dot/underscore.
3. **Arc<SqliteMemoryStore> trait dispatch** (severity: low, build). Root cause: trait methods implemented for `SqliteMemoryStore` not `Arc<SqliteMemoryStore>`; handlers passed `&st.store` (Arc). Fix: `&*st.store` deref. (Recurring across session/memory/trace handlers.)

## 14. Capability Matrix

| Capability | Backend | Desktop | Verification |
|---|---|---|---|
| Capability Manifest | GET /v1/apeireth/capabilities | fetchCapabilities + gating | E2E VERIFIED |
| Sessions.create/rename/archive/restore/close | /v1/apeireth/sessions/* | fetchers wired | LIVE MUTATION VERIFIED |
| Sessions.read | /v1/panel/sessions + /v1/apeireth/sessions | backend ledger tab | LIVE READ VERIFIED |
| Memory.update | PATCH /v1/apeireth/memory/episodes/:id | UI gated | CONTRACT VERIFIED |
| Memory.forget | POST .../forget | UI + ConfirmDialog | UNIT VERIFIED |
| Memory.protect/unprotect | POST .../protect\|unprotect | UI gated | UNIT VERIFIED |
| Memory.read/append | /v1/panel/memory + /v1/memory/append | MemoryView | LIVE READ VERIFIED |
| Permissions.grant | POST /v1/apeireth/grant | grant modal | LIVE READ VERIFIED |
| Permissions.revoke | POST /v1/apeireth/grants/:id/revoke | revoke modal | UNIT VERIFIED |
| Permissions.grants.read | GET /v1/apeireth/grants | grants list | LIVE READ VERIFIED |
| Permissions.evaluate | POST /v1/apeireth/grants/evaluate | (diagnostic) | LIVE READ VERIFIED |
| Trace.read | GET /v1/panel/traces(/:id) | trace-link→span-tree | LIVE READ VERIFIED |
| Trace.subscribe | SSE /v1/apeireth/events (type=trace) | ActivityView | WIRED, NOT LIVE VERIFIED |
| Trace persistence | V7 agent_traces | — | UNIT VERIFIED |
| Trace redaction | redact_attributes | — | E2E VERIFIED (attack test) |
| Trace raw CoT | NOT stored | — | CONTRACT VERIFIED (NO) |
| Legacy runtime compat | legacy_manifest | fallback | E2E VERIFIED |
| Migration V5/V6/V7 | append-only, idempotent | — | UNIT VERIFIED |

## 15. Remaining Issues
- P0 remaining: none
- P1 remaining:
  - Local localStorage sessions not force-migrated to backend (schema/safety undecided; backend endpoints ready).
  - Trace SSE subscribe not live-verified (recorder wired, no live agent run triggered to produce spans in smoke).
  - permissions.policy.write (persistent policy model) not implemented.
  - origin/master divergence (217 commits, 384 files) — main/branch coordination is a separate P1 item (not rebased this round to protect the verified UI baseline).
- P2 remaining:
  - Session hard delete (cascade/retention semantics).
  - Memory purge (physical delete).
  - Graph relation rebuild on forget (currently filtered, not rebuilt).
  - Visual polish (UI unchanged per scope).

## 16. Final Recommendation
READY FOR NEXT PHASE.

All P0 capabilities (Capability Discovery, Session Lifecycle, Memory Mutation, Permission Revoke/Expiry, Structured Trace, Desktop Integration, Migration, Security) are implemented, tested, and verified. 77 Rust + 22 frontend tests green. Live read + mutation smoke verified on isolated DB. Frontend visually verified via Playwright. Credential baseline maintained. No P0 remaining. P1 items are follow-ups (local→backend session migration, live trace, policy.write, main/branch coordination) that do not block the capability expansion's core value.
