# PR #2 Upstream Reconcile — 2026-08-20

> Synchronize latest YintaTriss/master into PR #2 head (Jimmyxiao2009 fork master)
> to resolve the GitHub merge conflict.

## PR identity

- PR #2: https://github.com/YintaTriss/apeireth-rust/pull/2 (cross-repo: Jimmyxiao2009 fork master → YintaTriss upstream master)
- previous PR HEAD: `bfdf533d`
- final PR HEAD: **`23c1b99f`** (pushed to origin/master)
- base (YintaTriss master): moved during the work (`67613d05` → `ddefe197` → `6f434a49` → `956c5682`)

## Integration method

**Normal merge commits, no rebase, no force push.** Two upstream syncs completed:

1. `35d1d709` — merge upstream `ddefe197` (16 upstream commits). 2 semantic conflicts in `companion_serve.rs` main(), resolved by fusing upstream's model/TOML config layer + `base_url()` with Runtime Decoupling's no-key boot (`load_key()` Option + `ProviderRuntimeState`).
2. `23c1b99f` — merge upstream `6f434a49` (1 commit: streaming_chat state machine). **Clean auto-merge, 0 conflicts.**

## Conflict resolution (sync 1)

| File | Conflict | Resolution |
|---|---|---|
| `companion_serve.rs` main() opening | HEAD: `load_key()` Option + provider_state; upstream: `init_model()` + TOML config + `load_key()?` | Fused: kept upstream init_model/init_base_url/TOML config layer + my no-key boot (load_key Option, provider_state). upstream's `load_key()?` → my `load_key()` (Option). |
| `companion_serve.rs` pipeline build | HEAD: `build_pipeline(BASE_URL, key.clone())`; upstream: `build_pipeline(base_url(), Some(key))` | Fused: `build_pipeline(base_url().to_string(), key.clone())` — upstream's `base_url()` + my Option key (no-key). |
| `companion/src/lib.rs` | auto-merged (0 markers) | clean |

## Verification (re-run on `23c1b99f`)

| Command | Result |
|---|---|
| `cargo check --workspace` | PASS |
| `cargo test runtime_capabilities` | 19 PASS (incl. 6 available/reason) |
| `cargo test session_lifecycle_integration` | 10 PASS |
| `cargo test no_key_runtime_smoke` (L3) | 1 PASS |
| `cargo test apeireth-memory --lib` | 349 PASS |
| `agent_trace` (no raw CoT, no deadlock) | 7 PASS |
| `pnpm check` (svelte-check) | 0 errors / 0 warnings |
| `pnpm build` (vite) | PASS |

Runtime Decoupling contracts preserved through both syncs (L3 no-key smoke PASS after each).

## Hard walls (bfdf533d → 23c1b99f delta = upstream content + 2 conflict resolutions)

- 28 files changed, all from upstream 16 commits + my 2 conflict resolutions
- workspace.version: 1.2.0 unchanged
- single capability endpoint: YES (1)
- migration V1–V7 canonical: YES (no upstream duplicate)
- runtime.ts untouched (no secret change)
- `apeireth-core/philosophy.rs` + `apeireth-constraint/lib.rs` changed by **upstream** (PHL-07, 13-key), not by my resolution
- `apeireth-memory` touched (LOCKED) — **pre-existing** from core-capability work (session/governance/migrations/trace), not introduced by this sync

## GitHub mergeability — BLOCKER

After push of `23c1b99f`, GitHub recalculated and reported PR #2 `mergeable: CONFLICTING` again because **upstream master advanced again** (`6f434a49 → 956c5682`, +2 commits: MultiLlmRouter/PipelinePool + LightGBM oracle) while I was working.

I attempted a 3rd sync (merge `956c5682`). It produced a **deep architecture conflict** in `companion_serve.rs` (3 conflict regions): upstream introduced `PipelinePool` + `MultiLlmRouter` (multi-provider router), replacing `AppState.pipeline: Arc<Pipeline>` with `AppState.pool: Arc<PipelinePool>` and rewriting the provider construction in `main()`. This is not a textual/semantic conflict resolvable by picking sides — it requires **architectural reconciliation** of Runtime Decoupling's `ProviderRuntimeState` (built on single `Pipeline`) with upstream's new `PipelinePool` abstraction. Per the task's hard rules (no infinite sync loop) and Phase 12 guidance, I **aborted** the 3rd merge and returned to the stable pushed state `23c1b99f`.

## Why I stopped (not a failure of this round's integration)

1. **The round's goal was achieved twice**: PR #2 was `MERGEABLE` at `35d1d709` (vs base `ddefe197`) and at `23c1b99f` (vs base `6f434a49`). The original conflict (PR head behind upstream) was resolved.
2. **Upstream is in an active refactor period**: it advanced 3 times during this session (`ddefe197`, `6f434a49`, `956c5682`), each push triggering a new conflict. This is upstream activity, not an integration defect.
3. **The 3rd conflict is architectural**, not textual: `PipelinePool`/`MultiLlmRouter` vs `ProviderRuntimeState` requires design work (how does no-key boot + provider availability compose with a multi-provider pool?), which is a new Runtime Decoupling phase, not an upstream sync task. Doing it under sync pressure risks unverified semantics.

## CI check status (informational)

upstream CI on PR head reports several **pre-existing** fails (not introduced by this sync):
- `守门 1: 0 触碰 24 LOCKED crate` — **pre-existing**: core-capability work touches `apeireth-memory` (LOCKED) by design; `apeireth-core`/`apeireth-constraint` touched by upstream's PHL-07. This is an architectural hard-wall conflict requiring upstream maintainer exemption decision, not fixable by sync.
- `cargo fmt --check` — pre-existing fmt drift in upstream's `apeireth-repo-tools/tests` (not my files).
- `companion-desktop CI gate` — aggregate gate fails because hard-walls job fails (not desktop itself).

These fails exist because PR #2's scope (core-capability touching LOCKED memory) is structurally at odds with upstream's 24-LOCKED gate. This predates this sync round.

## Final state

- PR #2 head: `23c1b99f` (origin/master == local HEAD)
- working tree: clean
- reconcile branch: UNCHANGED @ `14cafb43`
- backup branch: `backup/pr2-pre-upstream-reconcile-bfdf533d` @ `bfdf533d` (local)
- force push: NO (all normal fast-forward pushes)
- GitHub mergeable: NO (upstream advanced again to `956c5682`; 3rd sync aborted due to architectural PipelinePool conflict)

## Recommended next step (out of this round's scope)

When upstream's provider-layer refactor (PipelinePool/MultiLlmRouter) stabilizes, do a **Runtime Decoupling Phase 2** that reconciles `ProviderRuntimeState` with `PipelinePool`:
- `ProviderRuntimeState::Ready` when PipelinePool has ≥1 configured provider with a key
- `Unconfigured` when no provider has a key
- capability `available` reflects pool-wide provider availability
- L3 no-key smoke re-validated against the pool

This is a new feature phase, not an upstream sync, and should be done when upstream is not actively rewriting the same region.
