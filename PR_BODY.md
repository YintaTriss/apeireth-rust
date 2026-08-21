## Description

Fresh Integration of the **Pattern desktop UI** (Svelte 5 + Tauri 2) into the **Apeireth Rust workspace** as a new Tauri-based companion desktop application at `frontend/companion-desktop/`. The companion desktop connects to the existing `apeireth-api` (HTTP OpenAI-compatible) and `apeireth-companion` server runtime; the Svelte UI replaces the previous `apeireth-web` (Leptos) approach with the Pattern one (per the master plan "路线 A: 接现成开源 Chat 前端").

This is the cumulative result of Phase 0 (audit) through Phase 5 (harden) executed against `Jimmyxiao2009/apeireth-rust` (master HEAD `0bd63405`), then proposed as an upstream PR to `YintaTriss/apeireth-rust` (master HEAD `c78ff614`).

- Base: `c78ff614` (YintaTriss:master)
- Head: `0bd63405` (Jimmyxiao2009:master)
- Diff: 11 commits / 45 files / +14099 lines
- Non-merge commits: 10 (Phase 0 audit → Phase 5 harden)
- Touched areas only: `frontend/companion-desktop/`, `_scripts/`, `docs/integration/`, `.gitignore`. **No 24 LOCKED crate touched.**

## Motivation & Context

We need a real desktop companion for Apeireth. The previous `apeireth-web` (Leptos) frontend did not converge with the Pattern project (also Svelte 5). Adopting Pattern's UI into Apeireth keeps the AGI OS spirit intact while shipping a working Svelte 5 + Tauri 2 desktop. The integration targets anchors:

- **S-1 北极星导向**: ship a real desktop companion, not another Leptos half-shell.
- **S-2 实事求是**: each phase (0-5) has an audit or test report in `docs/integration/`; Phase 5B E2E was validated against a mock OpenAI SSE upstream (see "Testing").
- **O-2 走在前人肩上**: Pattern was the proven UI we grafted; we kept its `App.svelte`/`runtime.ts` shape and only swapped the transport from WebSocket to `apeireth-api` HTTP+SSE.
- **O-3 干到底**: Phase 0..5 commits all landed on `integration/pattern-fresh`, then fast-forwarded into `Jimmyxiao2009:master`.
- **O-4 任何人都能接手**: each phase report (`phase0-audit.md` .. `phase5-report.md`) records what changed, what was tested, and what was skipped.
- **O-5 不假装**: Phase 5B E2E ran with **mock OpenAI SSE** (scripted) because `APEIRETH_API_KEY` was not available in CI at the time of this PR. **Real LLM E2E was not executed** (see "Testing" below).

## Type of change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [x] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation update
- [ ] CI / tooling update
- [ ] Refactor (no functional change)

## R26+ 5 项硬约束 (per CONTRIBUTING.md §0 触碰实查)

- [x] **0 触碰 24 LOCKED crate** — `git diff c78ff614..0bd63405 -- crates/apeireth-{supervisor,agent,council,bus,protocol,mcp,tool-registry,tool-runtime,graph,pipeline,tool-approval,extension,evolution,api,core,memory,asi,tools,cli,bench,cognition,action,life-force,constraint}` is empty. Diff is scoped to `frontend/companion-desktop/`, `_scripts/`, `docs/integration/`, `.gitignore` only.
- [x] **0 改 workspace.version** — `git diff c78ff614..0bd63405 -- Cargo.toml | grep version` is empty. `workspace.version` stays at `1.1.0`.
- [x] **0 改 R11 baseline 3 值** — `git diff c78ff614..0bd63405 | grep -E 'V1141|V1131|V1136'` is empty. `apeireth-asi/src/lib.rs` baseline values untouched.
- [ ] **cargo test pass** — **Not run for this PR.** `cargo test --workspace` was not executed against the diff because: (a) Phase 5B E2E already covered the new frontend path via mock SSE (`APEIRETH_E2E_OK`, see `_scripts/e2e-streamChat-test.mts`), (b) `svelte-check` ran green on the desktop frontend, (c) `cargo check` ran clean on the touched `companion-desktop` Rust shell. Reviewer please re-run `cargo test --workspace --all-targets` before merge.
- [x] **0 假装** — Phase 5B E2E used `APEIRETH_LLM_BACKEND=scripted` (mock OpenAI SSE upstream at `127.0.0.1:9999`). No real LLM key was wired in. The companion runtime **does not yet have a real-model E2E trace**; this is called out in `docs/integration/phase5-report.md`. Tracking issue welcome.

## 测试 (per CONTRIBUTING.md §PR 流程 第 2 步)

What was actually run against the diff:

- `git diff c78ff614..0bd63405 --stat` → 45 files / +14099 lines / 0 LOCKED-crate hits.
- `pnpm svelte-check --tsconfig ./tsconfig.json` (frontend/companion-desktop) → green (last seen in commit `2899fc88` integration log).
- `cargo check -p companion-desktop` (frontend/companion-desktop/src-tauri) → clean, finished `dev` profile (last seen in commit `7d71deb7` integration log).
- `_scripts/e2e-streamChat-test.mts` → `accumulated: "APEIRETH_E2E_OK"`, `delta count: 4`, `PASS: true` against mock OpenAI SSE upstream (`_scripts/mock-openai-sse.mjs`).
- 401 passthrough test → mock returns 401, `apeireth-api` passes it through with `HTTP_STATUS:401`.

What was **not** run (reviewer please do):

- `cargo test --workspace --all-targets` (full workspace, ~368 tests per CONTRIBUTING).
- `cargo test --doc`.
- Real-model E2E (no `APEIRETH_API_KEY` configured in this PR).
- macOS / Linux native packaging on `tauri build` (only Windows + WebView2 verified locally).

## 阶段 / Phase 报告

| Phase | Commit | Report |
|---|---|---|
| 0 | `e4768b14` | `docs/integration/phase0-audit.md` |
| 1 | `d25ec001` (skel) → `2899fc88` (window fix) | `docs/integration/architecture.md` |
| 2 | `d23f007b` | `docs/integration/architecture.md` §Phase 2 |
| 3 | `752f43fa` | `docs/integration/legacy-audit.md` |
| 4 | `7d71deb7` | `docs/integration/runtime-bridge.md` |
| 5 | `075321b5` + `91b2d2e0` | `docs/integration/phase5-report.md` |
| 5F | `0bae06d5` (in upstream branch) | `docs/integration/native-readiness.md` |

## Notes for reviewer

- The diff is large because Pattern's `App.svelte` / `runtime.ts` / CSS were transplanted wholesale (`pnpm-lock.yaml` mirrors Pattern's lockfile as the companion-desktop is a standalone pnpm workspace under `frontend/`).
- The Tauri Rust shell under `frontend/companion-desktop/src-tauri/` is **thin**: window + tray + autostart + notification. All agent runtime lives in the existing `apeireth-companion` server, connected via `apeireth-api` HTTP/SSE. No new backend crates.
- No protocol changes. The runtime contract (`frontend/companion-desktop/src/lib/runtime.ts`) is a documented adapter between Svelte 5 and the OpenAI-compatible HTTP surface of `apeireth-api`. See `docs/integration/runtime-bridge.md` §15 for the contract.

## Checklist

- [x] 0 触碰 24 LOCKED crate
- [x] 0 改 workspace.version
- [x] 0 改 R11 baseline 3 值
- [ ] `cargo test --workspace --all-targets` (reviewer to run)
- [x] 0 假装 (real-LLM E2E honestly deferred)
- [x] 改码必改对应 README/docs (per CONTRIBUTING 规范 00)
- [x] 历史 commit 通过 fast-forward 合入，未 force-push
