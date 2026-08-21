# Apeireth Verification Ladder (L0–L5)

> Runtime Decoupling: defines the merge-blocking vs environment-dependent
> verification rungs. L4 must NOT block ordinary PRs on a real provider secret.

## Rungs

| Rung | What | Credential | Merge-blocking? |
|---|---|---|---|
| **L0** | compile / static (`cargo check --workspace`, `cargo fmt --check`, `svelte-check`) | none | ✅ yes |
| **L1** | unit tests (`cargo test --lib`, per-crate) | none | ✅ yes |
| **L2** | integration tests (`cargo test --test *`, in-process, no socket) | none | ✅ yes |
| **L3** | local runtime HTTP smoke — real TCP socket, no provider credential | **none** | ✅ yes |
| **L4** | provider live smoke — real inference/SSE against a real model | real API key | ❌ no (env-dependent) |
| **L5** | desktop E2E / visual acceptance (Playwright+Edge, 1280×720 + 1920×1080) | runtime + desktop | ❌ no (env-dependent) |

## L0–L3 = merge-blocking (CI-runnable, no secrets)

- **L0**: `cargo check --workspace`, `cargo fmt --all -- --check`, `pnpm check` (svelte-check), `pnpm build` (vite).
- **L1**: `cargo test --lib` per crate (e.g. `apeireth-companion --lib` 694+ tests, `apeireth-memory` governance/trace tests).
- **L2**: `cargo test --test session_lifecycle_integration` (10 in-memory tests, no socket), `apeireth-memory` migration tests.
- **L3**: `cargo test -p apeireth-companion --test no_key_runtime_smoke` — spawns the compiled `companion_serve` example on a free port with NO `APEIRETH_API_KEY` and a temp cwd (no `apikey-ultra.txt`), then validates over a real TCP socket + HTTP/1.0:
  1. server boots without key
  2. `GET /health` → 200, `core=healthy`, `provider=unconfigured`, top `status=ok` (compat)
  3. `GET /v1/apeireth/capabilities` → schema valid; `chat.completions` `supported=true/available=false/reason=provider_not_configured`; core caps `available=true`
  4. `POST /v1/apeireth/sessions` → 201 + list read (core capability truly works over HTTP)
  5. `POST /v1/chat/completions` → 503 `provider_not_configured`; server survives

### CI integration

L0–L3 run automatically in `.github/workflows/rust.yml` via
`cargo nextest run --workspace --profile ci --locked` (L3 is a normal `#[test]`
that nextest discovers and runs — no workflow change needed). The L3 test:
- requires **no secrets** (no `APEIRETH_API_KEY`, no `apikey-ultra.txt`)
- binds a free port per test (parallel-safe under nextest)
- uses `env_remove("APEIRETH_API_KEY")` (not `env_clear`) to preserve platform env

`companion-desktop-ci.yml` covers the frontend (Tauri `cargo check` + `pnpm svelte-check`).

## L4–L5 = environment-dependent (release validation, NOT PR-blocking)

- **L4**: requires a real provider API key (`apikey-ultra.txt` or `APEIRETH_API_KEY`).
  Validates real inference, SSE streaming, model discovery against a live model.
  Run manually before release; never required to pass an ordinary PR.
- **L5**: requires desktop runtime + a display. Playwright+Edge visual smoke
  (Chat / Sessions / Activity / Tools / Memory / Settings / RuntimeModal) at
  1280×720 and 1920×1080. Run manually before release.

## Why L4 is not a PR blocker

Provider availability is orthogonal to core runtime compliance. Requiring a
real model key on every PR would couple provider reachability into the merge
gate — a provider hiccup would block unrelated PRs. The Runtime Decoupling
design (Core Runtime vs Provider Runtime) makes this separation clean:
`/health` reports core health independently of provider, and capability
`available` reflects provider state without making the runtime dead.
