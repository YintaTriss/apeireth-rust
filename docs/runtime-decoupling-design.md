# Companion Runtime Decoupling — Design

> Phase 0 reality-check audit + decoupling design for `feature/companion-no-key-boot`.
> Base: `14cafb43` (frozen reconcile tip). All facts verified by code search 2026-08-20.

## 1. Why a provider key is a startup hard dependency today

The coupling is a single line of人为强制, not a type-system requirement.

- `crates/apeireth-companion/examples/companion_serve.rs:1252` — `let key = load_key()?;` is the **first statement** of `main()`.
- `load_key()` (SERVE:385-394): reads env `APEIRETH_API_KEY`; if empty, falls back to `std::fs::read_to_string("apikey-ultra.txt")` (SERVE:391, relative to cwd). On file-missing → `Err("读 apikey 失败: …")`.
- `main()` returns `Result<(), Box<dyn Error>>` (SERVE:1251), so `load_key()?` propagates the error straight out → **process exits before any resource init**.
- Everything after — PORT read (1253), memory store open (1259), pipeline build (1289), router build (1464), `TcpListener::bind` (1505) — never executes when the key is missing.

Net effect: **24/25 routes need no provider at runtime, but all are unreachable because the process never boots.**

## 2. Type system already permits keyless build

- `build_pipeline(base_url, auth_token: Option<String>) -> Result<Pipeline, String>` — `crates/apeireth-api/src/protocol_handlers.rs:102`. **auth_token is `Option<String>`.**
- `Pipeline::with_config` does not validate the token (`apeireth-pipeline/src/lib.rs:186-188`).
- `PipelineConfig.auth_token: Option<String>` with `Default` = `None` (`lib.rs:133,150`).

→ The强制 is purely in the example's `main()`: SERVE:1252 `load_key()?` + SERVE:1289 `Some(key.clone())`. The library types allow a keyless pipeline.

## 3. AppState — pipeline is a mandatory non-Option field

- `AppState` defined in the example file: `companion_serve.rs:347-360` (private struct, not in lib).
- Field `pipeline: Arc<Pipeline>` (SERVE:350) — non-`Option`, required.
- Constructed SERVE:1454-1463 with an already-`Arc<Pipeline>`; no keyless branch.
- This is the structural root of "no key → no AppState → no router".

## 4. Route classification (SERVE:1464-1503)

### Provider-dependent (request path calls provider)
- **`POST /v1/chat/completions`** (SERVE:1468, handler 899) — only route that calls provider:
  - streaming: `stream_forward(&st.pipeline, …)` (SERVE:1077)
  - non-streaming: `dispatch(pipeline, …)` (SERVE:1106/1208-1248)
- Daemon loop (SERVE:1520) also uses pipeline: dream summary (1379), reflect (1393), utterance polish (1424), memory extraction (1349-1360) — all via `dispatch`.

### Provider-independent (zero LLM call at runtime) — 24 routes
- `/` (index), `/health` (1466), `/v1/models` (1467 — **static hardcoded `MODEL` constant**, does NOT call provider)
- grant/grants/approval-requests (1469-1474)
- `/v1/apeireth/events` SSE (1475), `/test-event` (1476)
- `/v1/apeireth/capabilities` (1479 — static manifest)
- session lifecycle (1482-1486)
- memory governance (1489-1492)
- panel + traces (1494-1502)

## 5. Current `/health` semantics

- Handler `async fn health()` SERVE:2074-2081.
- Returns a **static constant JSON**: `status:"ok"`, `service:"apeireth-companion-serve-v4"`, `version: CARGO_PKG_VERSION`, `features:[12 hardcoded strings]`.
- Does NOT check provider, store, or anything at runtime. Semantics = "process alive = ok".
- Because of §1, `/health` was never observable in a provider-absent state.

## 6. Current capability manifest schema — NO `available` field

`crates/apeireth-companion/src/runtime_capabilities.rs`:
- `Capability` (CAPS:27-45): `id`, `supported`, `read`(default), `write`(default), `version`(default 1), `operations`(default, skip if empty). **No `available`.**
- `CapabilityManifest` (CAPS:70-81): `schema_version`, `runtime`, `capabilities`, `legacy`(default false).
- `MANIFEST_SCHEMA_VERSION = 1` (CAPS:24).
- `current_manifest()` (CAPS:152-227): pure static, takes no params, reflects no runtime state (e.g. key presence).
- 25 capability IDs declared; only `permissions.policy.write` is `supported:false`.
- `legacy_manifest()` (CAPS:247-296): server-side mirror of client fallback.
- Query helpers: `is_supported` (CAPS:87), `find` (92), `supported_ids` (100).

## 7. Desktop gating — reads only `supported`

- `fetchCapabilities` (`frontend/.../runtime.ts:597-623`): GET `/v1/apeireth/capabilities`; non-200 / schema-invalid / network error → `legacyCapabilityManifest()` (no throw).
- `capabilitySupported(manifest, id)` (TS:629-637): gates on `cap.supported === true` (TS:633). **Reads no `available` field.**
- `Capability` interface (`types.ts:139-146`): `id/supported/read?/write?/version?/operations?` — **no `available`**.
- Note: `types.ts:213 available:boolean` belongs to `ToolItem` (tools list), NOT Capability — unrelated.

## 8. Decoupling design

### 8.1 Boot: key optional
Change `main()` so a missing key does NOT exit. `load_key()` returns `Option<String>` (or the key is loaded best-effort); the pipeline is built with `auth_token: None` when no key. Core runtime boots regardless.

### 8.2 ProviderRuntimeState (single abstraction)
Introduce one typed enum to avoid scattered `Option<Client>`/`bool`:
```rust
enum ProviderRuntimeState {
    Ready { token_source: TokenSource },   // key present
    Unconfigured,                          // no key found (normal state)
    // future: Unavailable { reason } for provider-reachable-but-erroring
}
```
Carry this in `AppState` (or a sub-struct). Provider-dependent handlers branch on it.

### 8.3 Provider-dependent routes — stable contract when unconfigured
`/v1/chat/completions` (and daemon LLM calls) when `Unconfigured`: return a stable, machine-readable error (existing envelope, code `provider_not_configured`), HTTP 503. No panic, no hang, no 500-unknown. Daemon LLM tasks skip/no-op when unconfigured (not crash the loop).

### 8.4 `/health` — additive core/provider split
Keep existing fields (`status:"ok"` etc.) for backward compat. Add `core{}` and `provider{}`:
- `core.status`: healthy (runtime booted)
- `provider.status`: `ready` | `unconfigured`
- Top `status` stays `"ok"` as long as core is healthy (provider missing ≠ runtime dead).

### 8.5 Capability — `supported` + `available` + `reason`
Extend `Capability` with `available: bool` (default = supported, for backward compat) and `reason: Option<AvailabilityReason>` (only when `available:false`). `reason` is a machine-readable enum: `provider_not_configured`, `provider_unavailable`, `platform_unsupported`, `disabled_by_policy`.
- Provider-backed caps (e.g. `chat.completions`, `models.list` if it were live) → `available` reflects provider state.
- Core caps (sessions/memory/trace/permissions) → `available = supported` always (no provider dep).
- `current_manifest()` becomes runtime-aware: takes `ProviderRuntimeState` to set `available`/`reason` for provider-backed caps.

### 8.6 Backward compatibility
- `available` defaults to `supported` via serde `#[serde(default)]` so old manifests without it still parse.
- Desktop fallback: if manifest lacks `available`, treat `available = supported` (Phase 6).
- Old `/health` fields preserved; new fields are additive.

### 8.7 L3 smoke (no key)
Real HTTP socket test: bind localhost, start server with NO key, assert `/health` (core healthy, provider unconfigured), `/v1/apeireth/capabilities` (schema valid, core caps available, chat supported-but-unavailable with `reason=provider_not_configured`), one core capability (session create/read), and a provider route returning stable 503. No `apikey-ultra.txt`, no env key.

## 9. Scope guard (NOT this round)
permissions.policy.write, local→backend session migration, desktop redesign, new provider/model/tool/memory/trace architecture, PR #2 cleanup. L4/L5 are env-dependent, not merge-blocking.
