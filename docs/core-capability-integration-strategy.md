# Apeireth Fresh Integration — Strategy Decision & Reconciliation Plan

> 生成本文档的是 autonomous integration session (2026-08-20).
> 这是过程工程日志, 最终正式报告见 `docs/core-capability-integration-report.md`.

## 1. Baseline Facts (recorded raw)

- 工作仓库: `apeireth-fresh` (origin = Jimmyxiao2009/apeireth-rust, upstream = YintaTriss/apeireth-rust)
- feature 分支: `feature/core-capability-expansion` @ `b99ff8c2` (14 feature-only commits)
- origin/master: `968b4ceb` (217 master-only commits)
- merge-base: `91b2d2e0` (在 master 线上, 也是 local master `28e751fa` 的祖先)
- `4d0ac12e` 是 **feature-only** commit (不在 origin/master 线上), 是 feature 分叉后的第 3 个 commit
- local master `28e751fa` 落后 origin/master 13 commits, 0 ahead (纯祖先)

> 修正 prompt 假设: prompt 说 "feature 基于 4d0ac12e 之后完成 expansion", 实际 4d0ac12e 本身就在 feature-only 范围内. 真正的共同祖先是 91b2d2e0.

## 2. Divergence Summary

- feature-only: 14 commits (capability manifest / session / memory / permission / trace / desktop / migration / test / docs)
- master-only: 217 commits = **129 docs** + 24 test + 24 fix + 10 feat + Phase 0-9 desktop refactor + cron/CI/sandbox
- 文件级: master 改 371, feature 改 36; **C 类 (双方都改) 仅 7 个文件**
- **后端 Rust 零冲突**: master 完全未碰 feature 的 core capability 模块 (memory/session/trace/governance/permissions/runtime_capabilities)
- **migration 零冲突**: master 未新增任何 migration; feature append-only V1-V7 canonical 成立

## 3. Chosen Strategy: Hybrid (Option B base + frontend semantic reconciliation)

**以 feature 分支为基线, merge origin/master, 前端做语义调和.**

### 理由
- **migration safety**: master 无新 migration → feature V1-V7 直接成立, 无需重编号 ✓
- **commit separability**: 后端零冲突, merge 自动保留 feature 全部 14 commits ✓
- **conflict count**: 仅 5 个前端文件冲突 (7 命中里 2 个是 master 自动 rename 检测)
- **architecture overlap**: master 未重复实现 feature 能力 (master 的 "capability" 命中仅是 sandbox 文档 + llm-iface stub 测试) ✓
- **verification baseline**: feature 已验证基线为底, 叠加 master 修复, 回滚简单 (integration 分支纯本地, 失败整体丢弃) ✓
- **rollback simplicity**: 无 force push, 无 reset --hard, integration 分支可整体删除 ✓

### 为何不用 Option A (cherry-pick feature onto master)
feature 早期 desktop commits 与 master Phase 重构交织, cherry-pick 会逐 commit 冲突且无法享受 merge 的自动 rename 检测. Option B 让 git 自动处理 364 个 master-only 文件, 只需手工调和 5 个前端文件.

### 禁止 Option C
Option C (master 基线手工重建 feature) 仅在 cherry-pick 无法保证语义正确时使用. 本案后端零冲突, 不满足触发条件, 禁止默认 C.

## 4. Conflict Reconciliation Plan (5 frontend files)

### 4.1 `runtime.ts` — canonical API contract provider
**保留 feature 的 canonical 契约** (capability manifest gating + 全套 V2 mutation API + structured ToolCallDetails streaming):
- `fetchCapabilities` / `capabilitySupported` / `legacyCapabilityManifest` / `findCapability`
- V2: `createBackendSession` / `renameBackendSession` / `archiveBackendSession` / `restoreBackendSession` / `closeBackendSession` / `fetchBackendSessionsV2`
- governance: `updateMemoryEpisode` / `forgetMemoryEpisode` / `protectMemoryEpisode` / `unprotectMemoryEpisode`
- permissions: `fetchGrants` / `revokeGrant`
- trace: `fetchTraces` / `fetchTraceDetail`
- legacy panel: `fetchBackendSessions` / `fetchSessionTimeline` / `fetchMemoryEpisodes` / `fetchAuditLogs` / `appendMemoryEpisode`
- security: `loadConfig`/`saveConfig` secret purge invariants (apiKey/masterToken NOT persisted)

**融合 master 新增** (companion presentation + reasoning extraction):
- `subscribeCompanionEvents` + `CompanionPresentationState` + `CompanionEvent`
- inline ` extradetails` reasoning 提取 (master 的 `<think>`/`</think>` 块解析) — 注意: **raw CoT 仍不持久化**, 这是 presentation 层的临时推理折叠, 与 trace 持久化无关
- `streamChat` 返回 `{content, reasoning}` (master) + 保留 feature 的 `sessionId` header + `ToolCallDetails` structured callback

**重名冲突解决** (同名不同签名):
- `fetchTools`: 保留 feature 版 (richer: 双端点 fallback + ToolItem + permission/available) — master 的简化版 `ToolInfo[]` 通过 `export type ToolInfo = ToolItem` 别名兼容
- `fetchGraphData`: 保留 feature 版 (返回 `{facts, links}` of MemoryEpisodeItem) — master 的 `GraphData` 结构型版本作为可选并存? 决策: **保留 feature 版** (feature MemoryView 依赖), master 版重命名避免冲突 — 但 master 的 MemoryView 依赖 master 版. 见 4.3.
- `fetchMemoryStreams`: 保留 feature 版 (`Record<string, MemoryEpisodeItem[]>`) — feature MemoryView 依赖
- `grantToolPermission` vs `grantApproval`: 保留 feature 的 `grantToolPermission` (master `grantApproval` 是同语义, 保留 feature 名 + 加 `grantApproval` alias)
- `fetchApprovalRequests` vs `fetchPendingApprovals`: 保留 feature 的 `fetchApprovalRequests` (返回 ApprovalRequestItem) + `fetchPendingApprovals` alias 指向它

### 4.2 `App.svelte` — shell reconciliation
**以 feature 的 shell 为骨架** (已含 capability gating + RuntimeModal + 结构化 tool call), 融合 master 的 companion SSE + reasoning.
- 保留: `capabilities` state + `fetchCapabilities` in `refreshConnection` + capability-gated view props + `RuntimeModal` + `checkHealthDetailed` (subsystem diagnostics)
- 融合 master: `subscribeCompanionEvents` (companion 主动问候) + `companionPresentation` derived state + reasoning delta 处理 + retry
- 决策: feature 的内联 chat 布局 vs master 的 `<ChatView>` 组件 — **采用 feature 内联布局** (它已含 capability gating + RuntimeModal 集成, 且 master ChatView 不接收 capabilities prop). master 的 reasoning/companion SSE 通过 runtime.ts 融合进来.

### 4.3 `MemoryView.svelte` (modify/delete)
master 删了 `src/lib/MemoryView.svelte` (移到 `src/features/memory/`), feature 修改了旧路径.
**决策: 保留 feature 的 `src/lib/MemoryView.svelte`** (capability-gated forget/protect/update + detail modal + graph), 修正其 `PageHeader` 导入路径. master 的 `src/features/memory/MemoryView.svelte` (无 gating 的简化版) 删除以避免双源.
- 这要求 runtime.ts 保留 feature 版 `fetchGraphData`/`fetchMemoryStreams`/`fetchMemoryEpisodes` (已计划)

### 4.4 `MessageContent.svelte` (modify/delete)
master 删了 `src/lib/MessageContent.svelte` (移到 `src/features/chat/`), feature 修改了旧路径.
**决策: 保留 feature 的 `src/lib/MessageContent.svelte`** (含 ToolCallCard 结构化工具调用渲染), 修正导入路径. master 的 `src/features/chat/MessageContent.svelte` (含 ExecutionTimeline/TaskCard/reasoning) — **融合**: 把 master 的 reasoning 折叠 + ExecutionTimeline 支持并入 feature 版.

### 4.5 `ConversationsView.svelte` (content, master rename)
master 把 `src/lib/ConversationsView.svelte` 移到 `src/features/conversations/`, feature 在旧路径修改.
**决策: 保留 feature 版** (含 backend session ledger tab + pin/rename + delete confirm), 修正导入路径为 master 的新位置 OR 保持旧位置. App.svelte feature 版 import 的是 `./lib/ConversationsView.svelte` — **保持旧位置** `src/lib/ConversationsView.svelte`, 删除 master 的 `src/features/conversations/ConversationsView.svelte` 避免双源.

### 4.6 组件布局决策 (canonical)
两套布局共存 (feature `src/lib/components/` + master `src/components/` + `src/features/`).
**决策: 以 feature 的 `src/lib/` + `src/lib/components/` + `src/lib/views/` 布局为 canonical** (因为 feature 视图全部依赖它, 且 capability-gated 逻辑在此).
- 保留 feature 独有: `EmptyState/ErrorState/LoadingState/StatusBadge/ToolCallCard/ConfirmDialog/RuntimeModal/ActivityView/ToolsView/SettingsView`
- master 独有组件 (`ExecutionTimeline/TaskCard/CompanionWidget/QuickWindowView/Sidebar`) — **按需保留**: 若 MessageContent/App.svelte 融合后引用则保留, 否则删除避免 dead code.
- `PageHeader`: feature 视图从 `./PageHeader` / `../PageHeader` 导入但文件在 master 的 `src/components/`. **修正: 在 `src/lib/PageHeader.svelte` 创建 re-export shim** 或修正导入路径指向 `src/components/PageHeader.svelte`.

## 5. Checkpoints

- **Checkpoint A (P0)**: capabilities + migrations — 后端零冲突已自动 merge, migration V1-V7 canonical ✓
- **Checkpoint B**: sessions — `session_lifecycle.rs` feature-only, 自动保留 ✓
- **Checkpoint C**: memory — `memory_governance.rs` + `migrations.rs` feature-only, 自动保留 ✓
- **Checkpoint D**: permissions — `packs.rs` feature-only, 自动保留 ✓
- **Checkpoint E**: trace — `agent_trace.rs` feature-only, 自动保留 ✓
- **Checkpoint F (P0)**: desktop integration — 5 前端文件手工调和 (本文档 §4)

## 6. Blockers Encountered

- **BLOCKER (environmental, non-destructive)**: Bash safety classifier (`deepseek-v4-flash-0731`) temporarily unavailable during session. This blocks `git rm`, `git add`, `git commit`, `cargo check`, `pnpm build` — i.e. **conflict finalization and verification**.
- **Mitigation**: File edits via Write/Edit (no classifier needed) proceed. Git finalization + build/test deferred until classifier recovers. Recorded as BLOCKER per autonomous-mode rules; continuing independent reconciliation work.
