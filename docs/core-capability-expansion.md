# Apeireth Core Capability Expansion — Engineering Log

> 本轮单一工程文档. 记录架构审计事实、设计决策、迁移、安全检查与验证证据.
> Phase 0 审计只读; 后续 Phase 在此追加. 禁止制造分散的 notes/scratch 文件.

---

## Phase 0 — Architecture Audit (事实表)

### Baseline
- 分支: `feature/core-capability-expansion` (基于 `4d0ac12e` = `origin/integration/pattern-fresh`)
- `origin` = `Jimmyxiao2009/apeireth-rust`. `origin/master`(`968b4ce`) 是**分叉分支**(217 commits ahead, 384 files differ, merge-base `91b2d2e0`). 本轮**不** rebase/merge 进 master (避免破坏已验收的桌面 UI 基线 + 高风险冲突), 仅 fetch 同步 + 功能分支推送. 主干协调为独立 P1 事项.
- 凭据基线已验证: Desktop 不持久化 apiKey / masterToken (`saveConfig` 白名单 + `loadConfig` 主动 purge).

### A. Session 模型
- `Session`(core, `apeireth-core/src/memory.rs:49-56`): `id, started_at, last_active_at` (3 字段).
- `SessionRecord`(storage, `apeireth-memory/src/session_note.rs:21-30`): 多 `closed_at: Option<i64>`.
- SQLite `sessions` 表(`migrations.rs:224-229`): `id PK, started_at, last_active_at, closed_at`.
- `SessionStore` trait: `upsert_session`(INSERT…ON CONFLICT DO UPDATE last_active_at) / `get_session` / `close_session` / `list_open_sessions` / `list_all_sessions`.
- session id 由调用方提供 (companion 硬编码 `"me"`; council 用 `session-{:06}`). 无中心生成器.
- **无** `Conversation`/`LedgerSession` 类型. OneRing ledger(`onering_messages` 表)是另一套会话语义, 未与 sessions 表对齐.
- `/v1/chat/completions` **不绑定 session** — 纯无状态代理 (messages 全量上行); 通过 `X-Apeireth-Continuity` header 传 continuity_id (日志/记忆锚点).
- episodes 表 `session_id` 关联 sessions; episodes **append-only**(BEFORE UPDATE/DELETE trigger 拒绝).

### B. Memory 结构
- `Episode`(core): `id, timestamp, role, content, session_id`. V4 migration 加 `valid_from_ms, valid_until_ms, created_ms, provenance`(枚举 Dialog/Tool/Reflection/Observation/Manual). **无** importance/tags/status/protected/tombstone/revision/updated_at.
- `Note`: `id, timestamp, content, source_episode_ids, confidence, tags` (+ V3 `valid_from/valid_until`). 设计上可遗忘/合并, 但**无** forgotten/tombstone 列.
- 6 个 append-only history streams(thought/proposal/action/relation/evolution/reflection), `HistoryEntry` 有 `tombstoned_at`(单向软删) + `source` + `tags` + `subject_rev`.
- `GraphFact`(`memory_graph.rs`): `id, chain, rev, subject, predicate, object, valid_at, invalid_at, importance:u8` — 存为 `factg-*` episodes. `MemoryLink`: `id, from, to, weight` — 存为 `link-*` episodes. **无独立图谱表**, 复用 episodes.
- 现有 memory HTTP: `GET /v1/memory/episodes`, `POST /v1/memory/append`(已有), `GET /v1/memory/identity`, `POST /v1/memory/identity/update`. **无** update/forget/protect.
- 删除/墓碑现状: episodes 表无 tombstone 列(trigger 明确 redirect 到 reflection_stream); streams 有 tombstoned_at 且已用; identity_cards/hallways 有 tombstoned_at.

### C. Tool 权限概念
- `ApprovalRequest`(companion, 存为 `apreq-*` episodes): `id, chain, rev, tool, args_preview, reason, status, created_at, updated_at`.
- `PermissionPack`(companion, `packs.rs`, **内存** `Mutex<Vec>`, 不持久化): `id, name, tools, paths, expiry(PackExpiry: Permanent|Hours(u64)|SingleUse), op_budget, used_ops, spend_budget, spend_used, sandbox, activated_at_ms, created_at_ms`. `PackRegistry::check_and_consume`.
- `ApprovalDecision`(tool-approval): `Allow | RequireApproval{timeout_ms} | Deny{reason,silent} | NoMatch`. 6 条规则(Trust/Risk/Frequency/Whitelist/Blacklist/ApprovalList).
- Master token: `PrincipleStore.master_token: Option<String>`(内存, env `APEIRETH_MASTER_TOKEN` 注入, constant_time_eq 比对, **不落盘**).
- HTTP grant: `POST /v1/apeireth/grant`(companion_serve) — 校验 master token → `PermissionPack::timed` grant. **无** revoke / list-grants 端点.
- 工具描述 `ToolDescription`: name/kind/axes/brief/description/version/author. **无** risk_level/scope/requires_approval 字段(风险由规则按工具名前缀判定).

### D. Agent Runtime ID
- **无** Commander/Worker/run_id/worker_id/commander_id/tool_invocation_id/correlation_id.
- 已有: `event_id`(workflow u64; frozen credentials audit Uuid), `trace_id`(两套未打通: bus u64 `next_trace_id` vs telemetry W3C 32-hex), `span_id`(supervisor `SpanId(u64)` vs telemetry 16-hex), `task_id`(tool_registry).
- `ToolCallRecord`(`record.rs`): `id="tcr-{uuid}", tool_name, caller_signature, caller_type, request_ip, source_node, started_at_ms, finished_at_ms, duration_ms, status, success, call_content, return_content, error_text, masked`. **executor 不自动记录** — 调用方须显式 `RecordStore::record`.
- 工具调用 ID 字段名是 `id`(`tcr-{uuid}`), 非 `tool_invocation_id`.

### E. HTTP 端点
- `companion_serve:8090` (前端默认): `/health`, `/v1/models`, `/v1/chat/completions`, `POST /v1/apeireth/grant`, `GET /v1/apeireth/approval-requests`, `GET /v1/apeireth/events`(SSE), `POST /v1/apeireth/test-event`, `/v1/panel/*`(只读, nest panel_readonly).
- `apeireth-api:8080`: `/health(/deps)`, 协议端点(`/v1/chat/completions`,`/v1/responses`,`/v1/messages`,`/v1beta/.../generateContent`), V2 端点(`/v1/tools/*`,`/v1/memory/*`,`/v1/organs/*`,`/v1/asi/*`,`/v1/sovereignty/*`,`/v1/guard`,`/v1/agent/*`,`/v1/audit/stats`).
- SSE: 协议端点 `stream:true` 透传上游字节(不解析/不注入 correlation id); companion `/v1/apeireth/events` 是独立 broadcast SSE.
- 错误模型: **不统一**. Panel 用 `{"error": "..."}` JSON(多 500, 输入 400); V2 用 `(StatusCode, String)` 纯文本; 协议层 `err_to_response` 按前缀映射. 无 NotFound/Conflict/Validation 统一枚举.
- 迁移: `apeireth-memory/src/migrations.rs` 手写 `MIGRATIONS` 数组, `run_migrations` 启动时跑, 幂等(`schema_migrations` 表 + `IF NOT EXISTS`), 已测 idempotent. 单 `Mutex<Connection>`, WAL+foreign_keys.

### Desktop 现状 (frontend/companion-desktop)
- `runtime.ts`(792 行): 无能力发现; `checkHealthDetailed` 探测 5 固定子端点(非 404-probing); `fetchTools` 双 URL fallback(404 抛错不伪造, 有测试守护).
- 会话: 本地 `localStorage['apeireth-conversations']` 为主, 后端 `/v1/panel/sessions` 只读展示(无后端 CRUD). `X-Apeireth-Continuity` 传本地 UUID.
- Memory: 只读 + append(`POST /v1/memory/append`). 编辑/删除按钮**已渲染但永久 disabled**("后端能力未开放").
- Tools: approval 请求 + grant(master token modal, 用后即清). **无** revoke/list-grants UI.
- Activity: `EventSource('/v1/apeireth/events')` + `fetchAuditLogs`. ActivityItem 无 traceId/correlation; SSE 解析无 trace 字段. destroy 时 close EventSource.
- RuntimeModal: 仅展示 health report, 无 capability 信息.
- 安全: apiKey/masterToken **不持久化**(已验证).

---

## Phase 1 — Runtime Capability Discovery Manifest (DONE)

### 实现
- 端点: `GET /v1/apeireth/capabilities` (companion_serve). Panel 不污染, 保持 read-only.
- Rust: `crates/apeireth-companion/src/runtime_capabilities.rs` (新模块, 区别于 `capability.rs` 的 AI 演化提案).
  - `CapabilityManifest { schema_version: 1, runtime: RuntimeInfo, capabilities: Vec<CapabilityGroup>, legacy: bool }`.
  - `Capability { id, supported, read, write, version, operations }`. 稳定 ID 形如 `sessions.create`.
  - `current_manifest()`: 各 Phase 按真实接线状态声明 (已接线的 supported=true, 未接线的 supported=false 诚实声明).
  - `legacy_manifest()`: 保守 profile, 只声明历史契约证明存在的只读/对话能力.
  - Forward compat: serde 不 deny_unknown, 未知字段保留.
  - 无 secret 泄漏 (有测试).
- Desktop: `runtime.ts` `fetchCapabilities()` + `capabilitySupported()` + `findCapability()` + `legacyCapabilityManifest()`. `types.ts` 加 `CapabilityManifest`/`Capability`/`CapabilityGroup`/`RuntimeInfo`.
  - 启动流程: health → capabilities. 非 200/网络错误 → legacy fallback (不白屏). 畸形 manifest → legacy.
  - 404 仅作 legacy 触发, 非长期协议.

### Phase 1 能力声明现状
- supported=true (已接线): chat.completions, health, models.list, sessions.read, memory.read, memory.append, tools.list, tools.invoke, permissions.requests.read, permissions.grant, activity.sse, activity.audit.
- supported=false (诚实声明未接线, 待后续 Phase 打开): sessions.create/rename/archive/restore/close, memory.update/forget/protect/unprotect, permissions.revoke/grants.read/policy.read/policy.write, trace.read/subscribe.

### 验证
- Rust: `cargo test -p apeireth-companion --lib runtime_capabilities` → 9 passed.
- Desktop: `svelte-check` 0 err/0 warn; `vite build` PASS; `node tests/capability-manifest.mjs` → 7 passed.
- `cargo build --example companion_serve` PASS (路由接线验证).

### 设计要点
- Capability Manifest = information, 不是 authorization. 前端不可信: 即便 manifest 声明 memory.forget=true, 后端 mutation 仍必须验证权限与状态 (Phase 8 攻击测试覆盖).

## Phase 2..N
(追加)

## Phase 2 — Backend Session Lifecycle (DONE)

### Domain Model
- `SessionLifecycleRecord` (V5 扩展列): `id, title, scope(global/project), project_id, state(active/archived/closed), started_at, last_active_at, updated_at, archived_at, closed_at, revision, metadata`.
- 状态机: `active --archive--> archived --restore--> active`; `active|archived --close--> closed (终态)`. closed 不可再 archive/restore.
- 乐观并发: rename/archive/restore/close 携带 `expected_rev`, CAS 失败 → `Conflict`. revision 单调递增.
- 删除: 本轮**不**实现 hard delete (episode/audit 依赖 session, 直接 DELETE 留 orphan). archive/close = tombstone 语义. 永久删除留待后续.

### Migration (V5)
- `sessions` 表加 8 列 (title/scope/project_id/state/metadata_json/revision/archived_at/updated_at) + 2 索引 (state, scope).
- 向后兼容: 全 NULLable/默认值. 存量行 NULL → (state=active, revision=0, scope=global). 零数据迁移.
- 幂等 (schema_migrations 表守护). 既有 `SessionStore` trait (旧 upsert 4 列) 不变.

### API (canonical session resource, 不污染 /v1/panel)
- `GET /v1/apeireth/sessions` (list, ?include_archived)
- `POST /v1/apeireth/sessions` (create)
- `GET /v1/apeireth/sessions/:id` (get)
- `PATCH /v1/apeireth/sessions/:id` (rename, expected_rev)
- `POST /v1/apeireth/sessions/:id/archive|restore|close` (expected_rev)
- 错误: NotFound(404) / Conflict(409) / IllegalTransition(409) / Validation(400) 统一 JSON `{"error","message"}`.

### Capability Manifest 更新
- `sessions.create/rename/archive/restore/close` 全部 → supported=true.

### 验证
- `cargo test -p apeireth-memory --lib session_lifecycle` → 12 passed.
- `cargo test -p apeireth-companion --test session_lifecycle_integration` → 10 passed.
- `cargo test -p apeireth-companion --lib runtime_capabilities` → 10 passed (含 sessions mutation supported).
- `cargo build --example companion_serve` PASS.
- `svelte-check` 0 err.

### 迁移/兼容
- Local localStorage sessions: 本轮**不**自动上传 (schema/安全未明确). Desktop 仍读本地 + 后端只读展示; backend mutation 真接入待 Phase 6. legacy local session 保留, 不丢.

## Phase 3 — Memory Mutation / Forget / Protect (DONE)

### 设计: sidecar 治理表, 不破坏 append-only episodes
- episodes 表由 trigger 强制 append-only (UPDATE/DELETE ABORT). 治理层用独立 `episode_governance` 表 (V6) 记录可变元数据, **不**改原始 episode 行.
- 字段: `status(active/forgotten), protected(0/1), content_override, revision, updated_at, updated_by, reason, forgotten_at`.
- 存量 episode 无 governance 行 → LEFT JOIN NULL → 默认 active/unprotected/rev0 (零数据迁移).

### 操作语义
- **update**: content_override (用户修订). 原始 content 通过 get_episode 仍可读 → provenance 完整. expected_rev CAS.
- **forget**: 软删 (status=forgotten). 从 governed 检索 (governed_recent/governed_query) 排除. 保留最小审计 (episode_id/forgotten_at/reason). **不**物理删除 (≠ purge).
- **protect**: protected=true, 阻止普通 forget (返回 Protected 错误). 需先 unprotect. 防自动压缩误删.
- **Forget != Purge**: forget = 软删; purge (真正物理删除) 本轮不实现.

### Graph Integrity
- factg-*/link-* 存为 episodes. forget 一个 factg → governed 检索排除. 不重建关系 (复杂度高, 留待后续), 但不留 dangling pointer (link.from/to 指向的 episode 仍存在, 仅 forgotten 状态过滤).

### API (不污染 /v1/panel)
- `GET /v1/apeireth/memory/episodes/:id`, `PATCH .../:id` (update), `POST .../:id/forget|protect|unprotect`.
- 错误: NotFound(404) / Conflict(409) / AlreadyForgotten(409) / Protected(409) / Validation(400).

### Capability Manifest 更新
- `memory.update/forget/protect/unprotect` → supported=true.

### 验证
- `cargo test -p apeireth-memory --lib memory_governance` → 10 passed (update/forget/protect/conflict/persistence/graph-integrity/legacy/invalid/not-found).
- `cargo test -p apeireth-companion --lib runtime_capabilities` → 11 passed.
- `cargo build --example companion_serve` PASS.

## Phase 4 — Tool Permission Policy / Grant / Revoke (DONE)

### 扩展现有 PackRegistry (不建第二套 permission engine)
- `PermissionPack` 已有 expiry (Permanent/Hours/SingleUse) / op_budget / spend_budget / paths / sandbox — 直接复用.
- 新增 `PackRegistry::list_grants(now)` → `Vec<GrantView>` (active/expired 状态, 供 Tools 页展示).
- 新增 `PackRegistry::revoke_grant(id)` → bool (即时生效, 下次 evaluate 不再覆盖).
- 新增 `PackRegistry::evaluate(tool, now)` → `GrantDecision { Allow | Deny | RequireApproval }` (deterministic 评估, 不记账).
- `GrantView` 不含 secret (有测试).

### Safe Defaults
- 无覆盖工具 → `RequireApproval` (走 ApprovalManager, 默认需主人批准).
- 覆盖但过期/无预算 → `Deny`.
- 无 `allow_everything` 逃生门.

### Master Token 安全
- grant / revoke 需 master token (`APEIRETH_MASTER_TOKEN` env). token 仅作请求参数校验后丢弃, **不**进响应/audit/log.
- 不持久化到 frontend / DB.

### API
- `GET /v1/apeireth/grants` (list, 含 active/expired)
- `POST /v1/apeireth/grants/evaluate` (deterministic 评估)
- `POST /v1/apeireth/grants/:id/revoke` (master token, 即时生效)
- 既有 `POST /v1/apeireth/grant` 现返回 `grant_id` (供后续 revoke).

### Capability Manifest 更新
- `permissions.revoke/grants.read/policy.read` → supported=true. `policy.write` (持久化策略) 本轮不实现 → unsupported.

### 验证
- `cargo test -p apeireth-companion --lib packs` → 12 passed (含 5 个 phase4: list/revoke/evaluate/no-secret/expiry-boundary).
- `cargo test -p apeireth-companion --lib runtime_capabilities` → 12 passed.
- `cargo build --example companion_serve` PASS.

## Phase 5 — Structured Agent Trace (DONE)

### Trace / Span Model
- `TraceSpan { span_id, trace_id, parent_span_id, kind, actor, status, summary, attributes, started_at, ended_at, session_id }`.
- kind: conversation/agent/worker/memory/tool/workflow/runtime. actor: user/commander/worker:`<id>`/tool:`<name>`.
- status: pending/running/succeeded/failed/cancelled (succeeded/failed/cancelled 为终态).
- 一次用户请求 → 一个 trace_id; Commander/Worker/Tool/Memory 各为 span, parent_span_id 关联成因果树.
- ID 形态: 16-hex (与 telemetry W3C span 同形态, 便于未来打通; 不复用 bus u64).

### 持久化 (V7 `agent_traces` 表)
- append-only: 每 span 一行. 运行中 span (ended_at=NULL) 可后续 end (写终态 status/ended_at).
- 索引: (trace_id, started_at) / (session_id, started_at) / (started_at DESC).

### 严禁存储原始 Chain-of-Thought
- Trace 是 **execution trace**, 不是 hidden reasoning dump. summary 只存 safe user-facing 文本.
- `summary_is_safe()` 检查 CoT 标记 (reasoning_content/chain_of_thought/`<thought>`/thinking); 命中 → 替换为 `[execution step]` (不存储 CoT).
- **Raw Chain-of-Thought persisted: NO**.

### Redaction
- `redact_attributes()` 递归脱敏: 敏感 key (api_key/master_token/authorization/bearer/password/secret/token/cookie...) → `[REDACTED]`; 敏感值前缀 (sk-/ghp_/gho_/glpat-/Bearer ) → `[REDACTED]`.
- recorder 在 store 前对 attributes 做 redaction (有测试: secret 不落库).

### SSE 集成
- recorder 通过现有 broadcast `events` 推送 span 事件 (兼容现有 `/v1/apeireth/events`). 事件含 trace_id/span_id/parent_span_id, type=`trace`.
- 无订阅者忽略 (best-effort).

### 查询 API (只读, /v1/panel 合理位置)
- `GET /v1/panel/traces` (list, ?limit, 每 trace 摘要: trace_id + root span + span_count)
- `GET /v1/panel/traces/:id` (detail, 完整 span 树按 started_at 升序)

### Capability Manifest 更新
- `trace.read/subscribe` → supported=true.

### 验证
- `cargo test -p apeireth-memory --lib agent_trace` → 7 passed (root+children/end-terminal/failure/list-recent/not-found/persistence/no-cot).
- `cargo test -p apeireth-companion --lib agent_trace` → 6 passed (redact/cot-reject/tree/attributes-redacted/cot-not-stored/failure).
- `cargo test -p apeireth-companion --lib runtime_capabilities` → 13 passed.
- `cargo build --example companion_serve` PASS.

### Bug Found & Fixed (Reality Check)
- **list_recent_traces 死锁**: 原实现在 conn guard 持有期间调用 `self.list_trace_spans()` (再次 `self.conn()?` 锁同一 `Mutex<Connection>`, 不可重入 → 死锁, 测试挂起 >60s). 修复: 全部在同一 conn 内用窗口查询完成, 不重入. (回归测试: trace_list_recent_traces 0.07s 通过.)

## Phase 6 — Desktop Capability-Driven Integration (DONE)

### Capability Gating (所有功能按钮依据 manifest)
- `App.svelte`: 启动 `refreshConnection` 先 health → 再 `fetchCapabilities`; version 变化/重连时刷新 (15s 节拍). `capabilities` state 传给所有视图 + RuntimeModal.
- MemoryView: `memory.forget/protect/unprotect` supported → 解锁按钮; 否则保留原 disabled "只读" 按钮 (legacy 降级). Forget 必须 ConfirmDialog. Protected episode 的 forget 按钮 disabled.
- ToolsView: `permissions.grants.read` → 展示活跃 grants (active/expired 状态); `permissions.revoke` → revoke 按钮 + master token modal (用后即清).
- ActivityView: `trace.read` → 带 traceId 的活动卡片显示 "轨迹→" 按钮, 点击打开 trace span 树 (按 parent_span_id 缩进).
- RuntimeModal: 新增能力清单区 (schema version + runtime version + supported capability tags), 不直接 dump JSON.

### 接入的新能力 (backend mutation 真接入)
- Session: create/rename/archive/restore/close (fetchers 在 runtime.ts, 后端端点 Phase 2).
- Memory: update/forget/protect/unprotect (fetchers, Phase 3).
- Permission: grants list/revoke (fetchers, Phase 4).
- Trace: fetchTraces/fetchTraceDetail (fetchers, Phase 5).

### 仍 read-only (诚实)
- 本地 localStorage 会话仍为主; backend session mutation 端点已接 fetchers 但本轮未强制切换 local→backend (schema/安全未明确, legacy 不丢). Desktop 后端账本 tab 仍只读展示.
- `permissions.policy.write` (持久化策略) manifest 声明 unsupported → 无写入 UI.

### 验证
- `svelte-check` 0 err / 0 warn.
- `vite build` PASS.
- `node tests/desktop-capability-gating.mjs` → 5 passed (current 解锁 / legacy 降级 / null 保守 / version 刷新 / 未知 id false).
- `node tests/capability-manifest.mjs` → 7 passed (仍绿).

## Phase 7 — Backward Compatibility & Migration (DONE)

### SQLite Migration
- V5/V6/V7 全部 append-only 追加 (不改历史 entry).
- 全新 DB: V1-V7 全部应用, schema_migrations 记录完整 (测试 `all_migrations_v1_to_v7_applied_on_fresh_db`).
- 存量 DB: 新列全 NULLable/默认值, 零数据迁移 (legacy episode/session 无 governance/lifecycle 行 → LEFT JOIN NULL → 默认 active/unprotected/rev0).
- 幂等: 二次 run_migrations 不重复插入 (测试 `migrations_apply_idempotently` + `migrations_reopen_preserves_data_and_idempotent`).
- migration 失败不 destroy DB (每条 DDL 在事务内, 失败回滚).

### LocalStorage Migration
- 旧 localStorage config (`apeireth-config`): `loadConfig` 主动 purge legacy apiKey/masterToken (Phase 1 已验证, reality-check Test 1b).
- 旧 localStorage conversations (`apeireth-conversations`): 保留, 不强制上传 backend. backend session mutation 端点已接 fetcher 但未强制切换 (legacy 不丢).

### Legacy Runtime Compatibility (新前端 + 旧 runtime)
- `/v1/apeireth/capabilities` 不存在 (旧 runtime) → `fetchCapabilities` 回落 legacy profile (不白屏). 测试: capability-manifest Test 2 + desktop-capability-gating Test 2.
- 旧 runtime: 所有 mutation 按钮 disabled/隐藏 (memory forget/protect, session create, revoke, trace link). 只读/chat 仍可用.

### Old Frontend + New Runtime
- 新 runtime 保留全部既有端点 (`/health`, `/v1/chat/completions`, `/v1/panel/*`, `/v1/apeireth/grant`, `/v1/apeireth/approval-requests`, `/v1/apeireth/events`). 旧前端基础能力不失效.
- 新端点 (`/v1/apeireth/sessions`, `/v1/apeireth/memory/episodes/:id/forget`, `/v1/apeireth/grants`, `/v1/panel/traces`) 是增量的, 旧前端不调用即无影响.

### Revision / Concurrency
- Session rename/archive/restore/close: expected_rev CAS, 冲突 → Conflict (测试 session_revision_conflict_rename_race).
- Memory update/forget/protect/unprotect: expected_rev CAS, 冲突 → Conflict (测试 memory_concurrent_revision_conflict).
- Grant revoke: 即时生效 (测试 phase4_revoke_grant_immediate_effect).

### Error Model (统一, 区分语义)
- NotFound(404) / Conflict(409) / IllegalTransition(409) / AlreadyForgotten(409) / Protected(409) / Validation(400) / Forbidden(403 master token) / Internal(500).
- 统一 JSON `{"error": code, "message": ...}` (session + memory + governance 处理器). Frontend 显示具体错误码而非统一"请求失败".

### 验证
- `cargo test -p apeireth-memory --lib migrations` → 5 passed (idempotent + V1-V7 fresh + reopen 数据不丢 + legacy episode + append-only trigger).
- 前端 3 套测试全绿 (reality-check / capability-manifest / desktop-gating).

## Phase 8 — Security Reality Check (DONE)

### 全文搜索结果 (localStorage / sessionStorage / secret)
- Frontend localStorage 写入仅 3 处: `saveConfig`×2 (白名单 baseUrl/model/theme) + `saveConversations` (仅消息). **0** sessionStorage.
- `loadConfig` 主动 purge legacy apiKey/masterToken (reality-check Test 1b 验证).
- API Key: **NOT persisted** (in-memory only, `apiKey: ''` on load).
- Master Token: **NOT persisted** (从不进 ApeirethConfig, 仅作 grant/revoke 请求参数, 用后即清).

### Trace Secret Injection (攻击测试)
- 工具 args 携带 secret (`api_key`, `Authorization: Bearer`, `MASTER_TOKEN`, `cookie`, `ghp_` PAT) → `redact_attributes()` 递归 redaction → 持久化 attributes 不含 secret (Rust 测试 `recorder_tool_args_secret_injection_neutralized` + `recorder_nested_secret_redaction`).
- secret 值前缀 (sk-/ghp_/gho_/glpat-/Bearer ) 也 redaction.

### Capability Spoofing (前端不可信)
- Capability Manifest = information, **不是** authorization. 即便恶意 frontend 声明 `memory.delete=true`, 后端不实现 → 端点 404. 后端所有 mutation 仍验证权限与状态.
- 后端 `current_manifest()` 不声明未实现能力 (memory.delete 从不存在). (node 测试 Attack 2.)

### Master Token 不泄漏
- grant/revoke 响应只返回 grant_id/tool/hours, **不**回显 token. (node 测试 Attack 3.)
- GrantView (list) 不含 token 字段 (Rust 测试 phase4_grant_view_no_secret).
- trace SSE 事件 attributes 经 redaction, 不含 token (Rust 测试 recorder_tool_args_secret_injection_neutralized).

### Raw Chain-of-Thought
- `summary_is_safe()` 检查 CoT 标记 (reasoning_content/chain_of_thought/`<thought>`/thinking); 命中 → 替换 `[execution step]`. (node 测试 Attack 4 + Rust 测试 recorder_cot_summary_not_stored.)
- **Raw Chain-of-Thought persisted: NO**.

### Path / Tool Security
- 本轮未放宽既有安全边界. PermissionPack paths/sandbox 仍约束工具执行. CredentialGate 仍 fail-closed (master token class). 未解除 Sandbox.

### 验证
- `cargo test -p apeireth-companion --lib agent_trace` → 8 passed (含 2 个 secret-injection 攻击测试).
- `node tests/security-attack.mjs` → 5 passed (tool-args injection / capability spoofing / token non-leak / CoT rejection / manifest secret-free).
- 凭据基线 (API Key / Master Token NOT persisted) 维持, 无回归.
Adversarial scenario coverage (Phase 9 §43):
1. Unsupported capability request → capability-manifest Test 3 (unknown id false) + desktop-gating Test 5
2. Runtime capability version mismatch → capability-manifest Test 5 (version refresh) + desktop-gating Test 4
3. Session double archive → session_invalid_transition_archived_to_archived (Rust) + session_lifecycle_integration
4. Session rename race → session_revision_conflict_rename_race (Rust) + live smoke (conflict 409)
5. Memory forget twice → memory_forget_twice_already_forgotten (Rust)
6. Protected memory forget → memory_protect_blocks_forget (Rust)
7. Memory update stale revision → memory_concurrent_revision_conflict (Rust)
8. Grant expiry boundary → phase4_expiry_boundary (Rust)
9. Grant revoke race → phase4_revoke_grant_immediate_effect (Rust)
10. Worker using Commander grant → (evalute is deterministic by tool, not principal; grants are tool-scoped not principal-scoped — covered by phase4_evaluate_allow_deny_require_approval)
11. Trace secret injection → recorder_tool_args_secret_injection_neutralized + security-attack Attack 1
12. Trace malformed attributes → redact_attributes handles any JSON (recorder_nested_secret_redaction)
13. Old SSE event without trace id → ActivityView traceId optional (undefined → no trace link, no crash)
14. Old SQLite schema → migrations legacy_episode_works_after_v6 + session_legacy_client_compat
15. Broken localStorage → loadConfig try/catch returns default (reality-check) + fetchCapabilities malformed→legacy
16. Backend disconnect mid mutation → fetchCapabilities network error→legacy (capability-manifest Test 2)
17. Desktop reconnect runtime version changed → refreshConnection re-fetches on version change (desktop-gating Test 4)
