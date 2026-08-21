# Apeireth Core Capabilities — Contract & Invariants

> 本轮 Core Capability Expansion 落地的正式能力契约. 重点写 contract / invariants / security.
> 实现细节见 `docs/core-capability-expansion.md` (工程日志) 与源码.

## 1. Capability Manifest (能力发现契约)

**Endpoint**: `GET /v1/apeireth/capabilities`

**Contract**:
- `schema_version: 1` (仅不兼容变更才递增; 新增可选 capability/字段 = 兼容, 不递增).
- `runtime: {service, version}` — 仅 public 信息, **绝不**含 DB path / API key / master token / 内部路径.
- `capabilities: [{name, capabilities: [{id, supported, read, write, version, operations}]}]`.
- 能力 ID 稳定: 形如 `group.op` (如 `sessions.create`, `memory.forget`), 仅小写/数字/点/下划线.
- `legacy: bool` — true = runtime 无原生 manifest 端点, 客户端构造的保守声明.

**Invariants**:
- Backend owns capabilities. Frontend presents capabilities. Frontend **不** 404-probe.
- Manifest 是 **information, 不是 authorization**. 即便 manifest 声明 `memory.delete=true`, 后端 mutation 仍必须验证权限与状态. 前端不可信.
- 未实现的能力**诚实**声明 `supported: false`. 不假装.
- Forward compat: 未知字段保留 (serde 不 deny_unknown). 旧客户端读新 manifest 不崩.
- 未知能力 ID 一律 `supported: false` (保守).

**Legacy Compatibility**:
- 旧 runtime (无 `/v1/apeireth/capabilities`) → 客户端回落 `legacy_manifest()`, 只声明历史契约证明存在的只读/对话能力, **不**推测 mutation. UI 降级为只读, 不白屏.

## 2. Session Lifecycle (会话生命周期)

**Endpoint**: `/v1/apeireth/sessions` (canonical; `/v1/panel/sessions` 保持只读).

**Operations**: create / get / list / rename / archive / restore / close.

**State Machine**:
```
(create) -> active --archive--> archived --restore--> active
              |                    |
              +----close-----------+----close-----> closed (terminal)
```
- `closed` 是终态: 不可再 archive/restore.
- `archive` = 软隐藏 (默认列表排除, 数据不删).

**Invariants**:
- **No hard delete** this round. memory/episode/audit 依赖 session; 直接 `DELETE FROM sessions` 留 orphan. archive/close = tombstone. 永久删除需明确 cascading/retention (后续).
- **Optimistic concurrency**: rename/archive/restore/close 携带 `expected_rev`; CAS 失败 → 409 Conflict. revision 单调递增, 不静默覆盖.
- Legacy client (旧 4 列 upsert) 的 session 仍可被生命周期操作: NULL state→active, NULL revision→0, NULL scope→global (零数据迁移).

## 3. Memory Mutation (记忆治理)

**Endpoint**: `/v1/apeireth/memory/episodes/:id` (PATCH update) + `/:id/{forget,protect,unprotect}`.

**Design**: sidecar `episode_governance` 表 (V6). episodes 表 append-only (trigger 强制), 治理层**不**改原始 episode 行.

**Operations**:
- **update**: `content_override` (用户修订). 原始 content 通过 `get_episode` 仍可读 → provenance 完整. `expected_rev` CAS.
- **forget**: 软删 (`status=forgotten`). 从 governed 检索 (`governed_recent_episodes`/`governed_query`) 排除. 保留最小审计 (episode_id/forgotten_at/reason). **不**物理删除.
- **protect**: `protected=true`, 阻止普通 forget (返回 409 Protected). 需先 unprotect.
- **unprotect**: 解除保护.

**Invariants**:
- **Forget != Purge**. forget = 软删; purge (物理删除) 本轮不实现.
- **Provenance 不破坏**: update 不覆盖 id/timestamp/role/session_id/provenance. 只 override content + 记录 updated_at/updated_by.
- **Graph integrity**: factg-*/link-* 存为 episodes. forget 一个 factg → governed 检索排除, 不留 dangling pointer.
- Legacy episode (无 governance 行) → 默认 active/unprotected/rev0 (LEFT JOIN NULL, 零迁移).

## 4. Permissions (授权)

**Endpoints**: `POST /v1/apeireth/grant` (返回 grant_id) / `GET /v1/apeireth/grants` / `POST /v1/apeireth/grants/:id/revoke` / `POST /v1/apeireth/grants/evaluate`.

**Model**: 扩展现有 `PackRegistry` (PermissionPack: expiry/budget/paths/sandbox). 不建第二套 engine.

**Decision** (deterministic): `Allow` (匹配活跃包) / `Deny` (覆盖但过期/无预算) / `RequireApproval` (无覆盖, 走 ApprovalManager).

**Invariants**:
- **Safe defaults**: 危险操作默认 RequireApproval. 无 `allow_everything` 逃生门.
- **Revoke 即时生效**: revoke 后下一次 evaluate 立即不再覆盖.
- **Expiry**: Permanent / Hours(n) / SingleUse. 90 天续签提醒 (Permanent).
- **Master token**: 仅用于授权动作 (grant/revoke). constant_time_eq 比对. **不**持久化 frontend/DB, **不**进响应/audit/log/trace.

## 5. Agent Trace (执行轨迹)

**Endpoints**: `GET /v1/panel/traces` (list) / `GET /v1/panel/traces/:id` (detail). 实时事件 via SSE `/v1/apeireth/events` (type=trace).

**Model**: `TraceSpan { span_id, trace_id, parent_span_id, kind, actor, status, summary, attributes, started_at, ended_at, session_id }`.
- 一次请求 → 一个 trace_id; Commander/Worker/Tool/Memory 各为 span, parent_span_id 关联成树.
- ID: 16-hex (与 telemetry W3C span 同形态, 便于未来打通).
- kind: conversation/agent/worker/memory/tool/workflow/runtime. status: pending/running/succeeded/failed/cancelled.

**Invariants**:
- **NEVER persist raw Chain-of-Thought**. Trace 是 execution trace, 不是 reasoning dump. `summary_is_safe()` 检查 CoT 标记 (reasoning_content/chain_of_thought/`<thought>`/thinking), 命中 → 替换 `[execution step]`. **Raw CoT persisted: NO**.
- **Redaction**: `redact_attributes()` 递归脱敏敏感 key (api_key/master_token/authorization/bearer/password/secret/token/cookie) + 值前缀 (sk-/ghp_/gho_/glpat-/Bearer ) → `[REDACTED]`. store 前必 redact.
- 持久化: append-only `agent_traces` 表 (V7). 运行中 span 可后续 end.

## 6. Security Invariants (跨能力)

- API Key: **NOT persisted** by Desktop (in-memory only).
- Master Token: **NOT persisted** (从不进 ApeirethConfig, 仅作请求参数, 用后即清).
- Trace / Audit / Activity / Grant / Manifest: 均**不**记录 secret (有攻击测试).
- Capability Manifest 不泄漏 DB path / API key / master token / 内部路径.
- 错误响应不回显 secret.
- 本轮**未**放宽既有安全边界 (PermissionPack paths/sandbox, CredentialGate fail-closed, Sandbox).

## 7. Error Model (统一)

NotFound(404) / Conflict(409) / IllegalTransition(409) / AlreadyForgotten(409) / Protected(409) / Validation(400) / Forbidden(403, master token) / Internal(500).
统一 JSON `{"error": code, "message": ...}`. Frontend 显示具体错误码, 非"请求失败".

## 8. Migration Invariants

- Migrations append-only (不改历史 entry). V1-V7.
- 新列全 NULLable/默认值 → 零数据迁移 (存量行默认语义).
- 幂等 (schema_migrations 守护). 每条 DDL 在事务内, 失败回滚 (不 destroy DB).
- 测试: fresh DB (V1-V7 全应用) + reopen (数据不丢) + legacy (NULL 默认) + append-only trigger.
