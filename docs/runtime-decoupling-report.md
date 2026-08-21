# Apeireth Runtime Decoupling — Final Report

> `feature/companion-no-key-boot` — Runtime Decoupling Phase 1.
> Companion boots without any provider credential; Core Runtime and Provider
> Runtime are decoupled; capability manifest expresses supported/available/reason;
> L3 no-key HTTP smoke is merge-blocking and CI-runnable.

## Baseline

```
14cafb43  (frozen reconcile tip; code freeze 411b23ab)
```

## Branch

```
feature/companion-no-key-boot
```

## Architecture before

`companion_serve` made a provider API key a **startup hard dependency** via a single line:
`let key = load_key()?;` as the first statement of `main()` (companion_serve.rs:1252).
`load_key()` read env `APEIRETH_API_KEY` then fell back to `apikey-ultra.txt`; on
missing → `Err` → `main` returned → **process exited before any resource init**.

Consequences:
- 24/25 routes need no provider at runtime, but all were unreachable (process never booted).
- `/health` (static) and capability manifest (static) were never observable in a no-key state.
- Type system already permitted keyless build (`build_pipeline` takes `Option<String>`,
  `Pipeline::with_config` does not validate token) — the强制 was人为的, in the example bootstrap.
- `AppState.pipeline: Arc<Pipeline>` was a non-Option mandatory field.

## Architecture after

**Core Runtime** (health / capabilities / sessions / memory / permissions / traces /
tools-list) and **Provider Runtime** (chat / inference / provider-backed streaming) are
decoupled. Provider未配置 ≠ Companion Runtime启动失败.

- `load_key()` returns `Option<String>` (best-effort, no early exit).
- `main()` always builds the pipeline (`auth_token: None` when no key — types allow it);
  a `ProviderRuntimeState { Ready, Unconfigured }` tracks the state.
- `AppState` gains `provider_state: ProviderRuntimeState`.
- chat handler: `Unconfigured` → stable 503 `{code: provider_not_configured}` immediately
  (no dispatch attempt / retry wait).
- daemon + CompanionApp LLM calls: already best-effort swallow (audit-verified) — no change
  needed; they degrade gracefully (LLM judicator fails closed = tools conservatively rejected).

## Credential behavior

| State | Boot | Core Runtime | Provider-backed (chat/inference) |
|---|---|---|---|
| missing (no env, no key file) | ✅ boots | ✅ healthy, all core caps available | 503 `provider_not_configured`; manifest `available=false` |
| configured (key present) | ✅ boots | ✅ healthy | ✅ ready; manifest `available=true` |
| invalid/unreachable | ✅ boots (was already) | ✅ healthy | dispatch fails → 503/502 (existing behavior); future `Unavailable` state reserved |

## Health contract

`GET /health` (additive, backward-compatible):
```json
{
  "status": "ok",                       // legacy, preserved
  "service": "apeireth-companion-serve-v4",
  "version": "1.2.0",
  "features": [...],                    // legacy, preserved
  "core":     { "status": "healthy" },  // NEW — core runtime health
  "provider": { "status": "unconfigured" }  // NEW — ready | unconfigured
}
```
Top `status` stays `"ok"` as long as core is healthy (provider missing ≠ runtime dead).

## Capability contract

`Capability` gains two optional fields (backward-compatible via serde defaults):
- `available: Option<bool>` — callable right now (dynamic). `None` (old manifests) → falls back to `supported`.
- `reason: Option<AvailabilityReason>` — only when `available == Some(false)`, machine-readable:
  `provider_not_configured` | `provider_unavailable` | `platform_unsupported` | `disabled_by_policy`.

`current_manifest(provider: &ProviderRuntimeState)`:
- core caps (sessions/memory/trace/permissions/health/models.list) → `available = Some(true)`, no reason.
- provider-backed (`chat.completions`, `tools.invoke`) → `available` reflects provider state;
  `Unconfigured` → `available=false, reason=provider_not_configured` (but `supported` unchanged).

Single canonical endpoint `GET /v1/apeireth/capabilities` preserved (no 404-probing reintroduced).

## Compatibility

- **Old manifest (no `available` field) + new client**: `available=None` → `is_available()` falls back to `supported`. Tested (`backward_compat_old_manifest_without_available_falls_back_to_supported`).
- **New manifest + old client**: `available`/`reason` are additive optional fields; old client reads `supported` only (unchanged). serde does not deny_unknown.
- **Old `/health` clients**: legacy fields (`status`/`service`/`version`/`features`) preserved; new `core`/`provider` additive.
- Desktop `capabilitySupported()` retained; new `capabilityAvailable()` + `capabilityUnavailableReason()` added.

## L3 Smoke (no real key, real HTTP socket)

`crates/apeireth-companion/tests/no_key_runtime_smoke.rs` — spawns compiled
`companion_serve` example on a free port (NO `APEIRETH_API_KEY`, temp cwd with no
`apikey-ultra.txt`), validates over raw TcpStream HTTP/1.0:

| Test | Result |
|---|---|
| 1. boot without key | PASS (server up within 40s) |
| 2. GET /health → 200, core=healthy, provider=unconfigured, status=ok | PASS |
| 3. GET /v1/apeireth/capabilities → schema valid; chat supported/available=false/reason=provider_not_configured; core available=true | PASS |
| 4. POST /v1/apeireth/sessions → 201 + list read (core capability truly works) | PASS |
| 5. POST /v1/chat/completions → 503 provider_not_configured; server survives | PASS |

Proves: no real key, real HTTP socket (not router.oneshot / direct call). CI-runnable via
`cargo test --workspace` (nextest discovers it; example compiles under `cargo test --workspace`;
no workflow change needed). Zero external HTTP dep (raw TcpStream).

## Verification

| Command | Result |
|---|---|
| `cargo check --workspace` (85+ crates) | PASS |
| `cargo test -p apeireth-companion --lib` (runtime_capabilities etc.) | 19 PASS (incl. 6 new available/reason) |
| `cargo test -p apeireth-companion --test session_lifecycle_integration` | 10 PASS |
| `cargo test -p apeireth-companion --test no_key_runtime_smoke` (L3) | 1 PASS |
| `cargo test -p apeireth-memory --lib` | 349 PASS (governance/trace/migration) |
| `agent_trace` (trace_no_raw_cot_stored, trace_list_recent_traces) | 7 PASS |
| `pnpm check` (svelte-check, 3419 files) | 0 errors, 0 warnings |
| `pnpm build` (vite) | PASS (15.29s) |
| `cargo fmt -p apeireth-companion -- --check` | rustfmt --check 0 diff on changed files (cargo fmt --all blocked by Windows os error 206 on huge workspace; no regression introduced — frozen baseline also 0 via rustfmt --check) |

## Existing invariants (all PASS — no regression)

- Migration V1–V7 canonical, no duplicate version ✓ (apeireth-memory migration tests)
- Single capability endpoint `/v1/apeireth/capabilities` ✓
- Session lifecycle + optimistic CAS ✓ (10 integration tests)
- Memory forget ≠ purge; protected survives forgetting ✓
- No dangling references ✓
- No raw CoT persisted ✓ (`trace_no_raw_cot_stored`)
- No apiKey/masterToken persisted ✓ (unchanged; secret audit in freeze report)
- `list_recent_traces` no deadlock regression ✓ (`trace_list_recent_traces` pass)

## Deferred (NOT this round)

- **L4** provider live smoke (real key) — env-dependent, not merge-blocking.
- **L5** desktop visual acceptance (Playwright+Edge) — env-dependent.
- `permissions.policy.write` (P2 — policy model not yet stable).
- Legacy local→backend session migration (P1).
- `ProviderRuntimeState::Unavailable { reason }` (provider configured but unreachable) —
  reserved in design, not implemented this round (Ready/Unconfigured sufficient for no-key boot).
- Desktop UI distinct "Unsupported" vs "Provider not configured" visual treatment —
  gating entry point added this round; visual differentiation deferred to avoid redesign.

## Git (this round's commits on feature/companion-no-key-boot, base 14cafb43)

```
4b230e3c  chore: protect local provider credential files (.gitignore)
c9799490  docs: audit companion provider coupling (Phase 0)
8fe308be  feat(companion): decouple provider from core runtime boot (Phase 2-4)
293160d6  feat(companion-desktop): add capability availability gating (minimal) (Phase 7)
f27bc8fb  test: add no-key local runtime HTTP smoke (L3) (Phase 8)
7163b835  docs: document L0-L5 verification ladder + CI integration (Phase 9-10)
<final>    docs: runtime decoupling final report (this file)
```

No rebase, no reset --hard, no force push, no merge to master. Reconcile branch untouched.

## Final verdict

```
READY
```

- companion boots without provider credential (verified via L3 real HTTP smoke)
- core health decoupled from provider (additive /health split, backward-compatible)
- capability supported/available/reason semantics (backward-compatible, tested)
- L3 merge-blocking smoke is CI-runnable, no secrets
- all existing invariants green
- no destructive git, reconcile branch frozen
