# PR #2 Runtime Decoupling Integration — 2026-08-20

> Integrating `feature/companion-no-key-boot` Runtime Decoupling into existing
> PR #2 (cross-repo: Jimmyxiao2009 fork `master` → YintaTriss upstream `master`).

## PR identity (GitHub authoritative)

- PR number: **#2**
- PR URL: https://github.com/YintaTriss/apeireth-rust/pull/2
- PR state: OPEN (isDraft: false)
- PR title: "新的桌面端Shell"
- isCrossRepository: **true**
- head repo: Jimmyxiao2009/apeireth-rust
- head branch: **`master`**
- base repo: YintaTriss/apeireth-rust (upstream)
- base branch: `master`
- base SHA: `67613d05`

## SHAs

- old PR HEAD: **`968b4ceb`** (= origin/master before update = PR #2 head)
- Runtime-Decoupling source: `feature/companion-no-key-boot` @ **`020a2713`**
- merge-base(PR_HEAD, FEATURE_HEAD): `968b4ceb` (= PR_HEAD itself)
- new PR HEAD: **`020a2713`**

## Integration method

**Fast-forward** (Case A — ideal).

- `git merge-base --is-ancestor 968b4ceb 020a2713` → YES (PR head is ancestor of feature)
- `git rev-list --left-right --count 968b4ceb...020a2713` → `0  25` (PR head has 0 unique, feature has 25 ahead)
- Pure fast-forward: `968b4ceb..020a2713`, no merge commit created, no rebase, no force.

The 25 commits = 18 core-capability-expansion + 3 reconcile + 4 runtime-decoupling-freeze-base... precisely:
- 18 core-capability-expansion (capability manifest / session / memory / permission / trace / desktop / migration / tests / docs)
- 3 reconcile (merge `3131c0cd` + fix `bb84fa43` + review `411b23ab` + freeze-report `14cafb43` — actually 4)
- 7 runtime-decoupling (credential hygiene + decouple boot + desktop gating + L3 smoke + docs + report)

## Conflict resolution

**None.** Fast-forward introduces no conflicts.

## Why this updates PR #2

PR #2's head branch is Jimmyxiao2009 fork's `master`. Pushing `020a2713` to
`origin/master` fast-forwards the PR head from `968b4ceb` to `020a2713`. Branch
protection on Jimmyxiao2009/master: **none** (404 — unprotected). Push dry-run
confirmed fast-forward `968b4ceb..020a2713 -> master` with no force.

> Note: this moves origin/master forward 25 commits (the feature work was based on
> a reconcile that already merged origin/master). This is safe (fast-forward, no
> history loss) but it expands PR #2's scope from "desktop shell" to
> "desktop shell + core capability + runtime decoupling". This is the intended
> update per the task.

## Verification (re-run on integration state = 020a2713)

| Command | Result |
|---|---|
| `cargo check --workspace` (85+ crates) | PASS |
| `cargo test -p apeireth-companion --lib runtime_capabilities` | 19 PASS (incl. 6 available/reason) |
| `cargo test -p apeireth-companion --test session_lifecycle_integration` | 10 PASS |
| `cargo test -p apeireth-companion --test no_key_runtime_smoke` (L3) | 1 PASS |
| `cargo test -p apeireth-memory --lib` | 349 PASS |
| `agent_trace` (trace_no_raw_cot_stored, trace_list_recent_traces) | 7 PASS |
| `pnpm check` (svelte-check, 3419 files) | 0 errors, 0 warnings |
| `pnpm build` (vite) | PASS (13.42s) |
| `git diff --check` | 1 pre-existing EOF-blank-line warning in reality-check.mjs (core-capability file, not this round) |
| `cargo fmt --all -- --check` | NOT RUN — Windows os error 206 (command-line too long on huge workspace); rustfmt --check on changed files = 0 diff |

## Hard-wall audit (OLD 968b4ceb → NEW 020a2713)

- 52 files changed, +14058/-1764 (35 Added, 8 Deleted, 9 Modified) — all in core-capability + runtime-decoupling scope
- LOCKED crates (core/asi/cognition/constraint/sovereignty/naming/bus/graph-primitive/state/onion): **untouched**
- workspace.version: **1.2.0 unchanged**
- R11 baseline: unchanged (not touched)
- migration: single canonical `migrations.rs` (V1-V7), no duplicate
- capability endpoint: single `/v1/apeireth/capabilities` (count=1, no duplicate)
- deleted views: 8 master duplicate views deleted (Sidebar/ActivityCenterView/ChatView/MessageContent/CompanionWidget/ConversationsView/MemoryView/SettingsView) — none resurrected
- secret persistence: none (localStorage.setItem only safeConfig baseUrl/model/theme + conversations; apikey-ultra.txt not tracked)
- raw CoT: not persisted (`trace_no_raw_cot_stored` PASS)
- no unexpected large diff expansion: 52 files, all accounted for

## Runtime Decoupling contracts (Phase 5 invariants, verified)

- no-key boot: PASS (L3 Test 1 — server boots without APEIRETH_API_KEY / apikey-ultra.txt)
- /health: top-level `status:"ok"` preserved + additive `core{healthy}`/`provider{ready|unconfigured}`; provider missing ≠ core unhealthy (L3 Test 2)
- capabilities: single canonical endpoint; chat `supported=true/available=false/reason=provider_not_configured` when no key; core caps `available=true` (L3 Test 3)
- backward compat: old manifest without `available` → `available = supported` (tested)
- provider failure: chat → 503 `provider_not_configured`, no panic, no process exit (L3 Test 5)
- L3: real TCP socket HTTP (raw TcpStream), not router.oneshot (L3 PASS)

## Remaining warnings

- `cargo fmt --all -- --check` blocked on Windows (os error 206) — environment limit, not code; CI on Linux unaffected.
- `git diff --check` 1 EOF-blank-line in `reality-check.mjs` — pre-existing from core-capability, not this round.
- L4 (provider live smoke) / L5 (desktop visual): NOT RUN — env-dependent, not merge-blocking by design.

## Push result

- `git push origin 020a2713:master` → fast-forward `968b4ceb..020a2713`, no force.
- PR #2 head updated to `020a2713` (confirmed via `gh pr view 2`).
- local integrated HEAD == GitHub PR #2 head SHA.

## Frozen reconcile branch

`integration/core-capability-reconcile` **UNCHANGED @ `14cafb43`** (not touched this round).

## Final state

- PR #2: updated (head = 020a2713)
- new PR: NO
- PR merged: NO
- feature branch preserved: YES (feature/companion-no-key-boot @ 020a2713)
- reconcile branch untouched: YES (14cafb43)
- working tree: clean
- remote PR head == local integrated head: YES
- force push: NO (fast-forward only)
