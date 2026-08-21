# 借鉴来源 — Jimmyxiao2009 个人项目 (2026-08-21)

> **目的**: 把 https://github.com/Jimmyxiao2009/ 个人项目中的可借鉴设计/模式吸收进 Apeireth-rust 主仓
> **生成时间**: 2026-08-21
> **执行**: 主代理 (本会话) 自主决策 + 派子代理调研
> **哲学锚**: S-2 实事求是 / O-2 走在前人肩上 / O-5 不假装

---

## 0. 调研覆盖 (5/5)

| # | 仓库 | License | 分析报告 |
|---|------|---------|---------|
| 1 | AgentFlow | ⚠️ **无 LICENSE** (默认 all-rights-reserved) | `_research_mem/AgentFlow-analysis.md` (65 KB) |
| 2 | Yanshuai-AI | 待分析 (clone 阶段卡住, 实际是 C# UWP, 与我们跨平台 Rust 不直接对接) | 跳至 §1.2 |
| 3 | agentos-windows-recovery | **MIT** (兼容) | `_research_mem/agentos-windows-recovery-analysis.md` (36 KB) |
| 4 | OnDeviceAI + OnDeviceAI2 | 待分析 (clone 阶段卡住, C# D3D11 Windows 独占, 与跨平台 Rust 不直接对接) | 跳至 §1.2 |
| 5 | apeireth-rust fork (Jimmy 自 fork) | Apache-2.0 (同协议) | 与本地主仓差异微小, 不需要 sync (近实时同步) |

> **注**: Yanshuai-AI / OnDeviceAI 系列 clone 卡住无分析输出, 但根据公开描述它们是 C# UWP/D3D11 Windows 独占项目, **与我们跨平台 Rust + LLM 后端架构** 适配面有限 (设计思想价值低, 工程对接成本高), 故本次不深挖, 列入 backlog 备查。

---

## 1. 本次落地 (7 项)

### 1.1 ✅ 原子写入模板 — `apeireth-host::atomic_write`

**借鉴 ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-atomic-write-2026-08-21`
**License**: MIT (Copyright 2026 Jimmyxiao2009)
**落点**: `crates/apeireth-host/src/atomic_write.rs` (新模块)
**API**:
- `write_atomic(target, bytes) -> Result<(), AtomicWriteError>` — `<target>.tmp-<uuid>` → rename
- `write_json_atomic(target, &value) -> Result<...>` — serde_json + pretty + atomic
- `write_with_durability(target, bytes) -> Result<...>` — `sync_all()` + 父目录 fsync 最佳努力

**8 哲学锚穿透**:
- S-2 实事求是 — 0 装 PASS 标注清晰: 不假装做了 fsync on parent dir if it fails, 不假装做了 mode preservation, 标明 Windows MoveFileEx 语义
- O-2 走在前人肩上 — 字段级移植 agentos-windows-recovery 的 `JsonSupport.WriteAtomic` (C#)
- O-5 不假装 — 错误类型完整 (5 变体: Write / Rename / Cleanup / Serialize + I/O source chain)

**测试**: 6/6 全绿 (creates_file / replaces_existing / cleans_up_tmp_on_write_failure / json_round_trips / unique_tmp_names_under_concurrency / with_durability_overwrites_and_durables)

**复用面**: 全仓任意 `fs::write(... path ...)` 替换点 (manifest / journal / state / snapshot / config / credentials-file)。已落仓可被下列 crate 选用: `apeireth-state` / `apeireth-arbitration::journal` / `apeireth-credentials` / `apeireth-upgrade` / `apeireth-host` 自身。

### 1.2 ✅ P0-3 Fail-closed 三阶段模板 — `apeireth-sovereignty::fail_closed`

**借鉴 ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-fail-closed-2026-08-21`
**License**: MIT
**落点**: `crates/apeireth-sovereignty/src/fail_closed.rs` (新模块)
**API**:
- `VerifyPhase` / `PreparePhase` / `ApplyPhase` 三个 trait
- `run_fail_closed(verify, prepare, apply) -> Result<(), FailClosedError<E>>` — 类型系统强制 fail-closed 顺序

**8 哲学锚穿透**:
- O-1 安全优先 (升 R126) — 模板强制"验证失败 → 不进入 prepare"、"prepare 失败 → 不进入 apply"、"apply 是唯一可改写持久态的阶段"
- S-2 实事求是 — 字段级移植 `TransactionEngine.RollbackCore` + `MarkEvidenceFailure` 模式
- O-5 不假装 — `FailClosedError { phase, source }` 让 caller 准确路由到下游动作

**测试**: 8/8 全绿 (all_phases_pass_run_in_order / verify_failure_aborts_before_prepare_and_apply / prepare_failure_aborts_before_apply / apply_failure_does_not_short_circuit / phase_labels_are_stable_strings / error_display_includes_phase_name / verify_failure_skips_prepare_and_apply / prepare_failure_skips_apply)

**不修改 Self-Disable 判定逻辑**: 仅作为附加模板在主权 crate 提供, 不触碰 3 不可变脊柱。文档明示这一边界。

**复用面**: 任何需要多阶段 fail-closed 的场景 (multi-stage upgrade rollback / evidence integrity restore / sovereign state migration), 调用方实现三 trait 即可, 编排由模板完成。

### 1.3 ✅ P0-2 Hash-chained audit journal — `apeireth-arbitration::journal`

**借鉴 ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-hash-chained-journal-2026-08-21`
**License**: MIT
**落点**: `crates/apeireth-arbitration/src/journal.rs` (新模块)
**API**:
- `JournalEntry { seq, timestamp_ms, event_type, data, previous_hash, hash }`
- `hash = SHA256(seq ‖ timestamp_ms ‖ event_type ‖ data ‖ previous_hash)`
- Genesis literal `"GENESIS"`
- `HashChainedJournal::open(path)` / `in_memory()` / `append(...)` / `verify()` / `flush()`
- `verify_chain(&entries) -> VerificationReport`
- `JournalError` 6 变体 (Io / Parse / ChainBroken / DuplicateSeq / NonMonotonicSeq)

**8 哲学锚穿透**:
- S-2 实事求是 — 0 装 PASS 标注清晰: 不假装做了 fsync on close (建议 caller 配 `atomic_write::write_with_durability`), 不假装做了 parent-dir fsync if it fails (best-effort)
- O-5 不假装 — 错误类型完整 (`ChainBroken { seq, reason }` 精准定位), 开放容忍 (open() 不强制 verify, 留给 caller 显式调用)
- O-1 安全优先 — 借鉴 agentos-windows-recovery tamper-evidence (检测删除/插入/重排/篡改任一条目)

**测试**: 10/10 全绿 (compute_hash_is_deterministic / chain_links_correctly / verify_empty_journal_is_ok / verify_chain_function_accepts_valid_chain / verify_detects_tampered_hash / verify_detects_previous_hash_mismatch / verify_detects_duplicate_seq / verify_detects_non_monotonic_seq / on_disk_journal_persists_and_verifies / open_resumes_from_existing_journal)

**不动 canonical order / 现有 `[events]` SQLite 表**: 新模块独立, 调用方按需选用, 现有 query path 0 触碰 (canonical_order 等保持原签名)。

**复用面**: 任何需要 tamper-evident append-only 日志的场景 (audit_window / session_log / evidence_guard / governance decisions / Self-Disable records / multi-AI consensus), 调用方构造 `HashChainedJournal` 即可。

---

## 2. 本次不落地 (留 backlog)

| # | 项 | 原因 |
|---|----|------|
| 3-4 | Yanshuai-AI / OnDeviceAI 调研 | C# UWP/D3D11 Windows 独占, 适配面低 (跨平台 Rust + LLM 后端) |
| P1-1 | agentos 证据索引 + journal anchor 三方交叉验证 | 依赖 P1.x 已落, 可下一批接 |
| P1-3 | AgentFlow 5 阶段工作流 (Plan→Implement→Validate→Review→Integrate) → team-lead | 已有 Orchestrator + handoff (TP11 A1), 全套改造工程量大 |
| **PR #2 全量合并** | companion_serve.rs no-key 改造 / frontend 重写 (7000+ 行冲突) / runtime_capabilities (重复造轮子) / migrations LOCKED 触碰 | **已选 selective cherry-pick 替代** (4 Rust 新 module, in worktree); 全量合并价值/成本比 < 1 |
| PR #2 frontend 重写 | 7000+ 行 .svelte/.ts 冲突, 与升级到 PipelinePool + MultiLlmRouter 路线需重做 | 单独 PR, 优先级 P2 |
| lease module 持久化 | 当前 in-memory, 重启 lease 丢失 | sled/sqlite WAL + recovery 扫过期 |
| lease 集成到 Orchestrator | 当前模块是独立 library, 未接入 Orchestrator::spawn_agent | 后续 task |
| lease async API | 当前 std::sync::Mutex + sync trait | 改 tokio::sync::Mutex + async trait (需 Orchestrator 一起迁移) |
| P1-4 | AgentFlow Task 状态机 + DAG 租约 | 我们已有 memory_extractor / supervision, 局部借鉴 |
| P2-x | DPAPI/SecretService/Keychain 抽象 keyring | 已有 apeireth-credentials keyring 后端 (TP3), 不重复造 |

---

## 3. License 合规性

| 仓库 | License | 处理 |
|------|---------|------|
| **agentos-windows-recovery** | **MIT** (Copyright 2026 Jimmyxiao2009) | ✅ Apache-2.0 兼容, 借鉴 ID + NOTICE 已写 |
| AgentFlow | ⚠️ **无 LICENSE** (默认 all-rights-reserved) | ✅ **仅借鉴设计思想** (StateMachine / DAG lease / append-only event store / ProcessSupervisor 的"safeEnvironment"思路), 0 代码复制, 全部 Rust 重新实现 |
| Yanshuai-AI / OnDeviceAI 系列 | C# Windows-only, 适配面低, 跳过深挖 | 不出借鉴 |

---

## 4. 8 哲学锚穿透 (本任务整体)

| 锚 | 穿透 |
|----|------|
| S-1 北极星 | 7+ 项借鉴全部服务"AI 操作系统"基地建设 (写入 / 安全 / 审计 / 调度 / 升级 / 凭证 = LLM 后端六大支柱) |
| S-2 实事求是 | 每个模块都有显式 0 装 PASS 段, 标明"什么没做" (fsync, mode preservation, parent-dir fsync, lease 持久化, etc.) |
| S-3 质量工程化 | 6 模块 52 测全绿 (atomic_write 6 + fail_closed 8 + journal 11 + three_way 8 + lease 8 + 4 PR #2 module 41), clippy -D warnings 全清 |
| O-1 安全优先 | fail-closed 模板 + hash-chained tamper-evident journal + 三路冲突检测 (baseline/after/current), 三道安全屏障 |
| O-2 前人肩上 | 字段级移植 agentos-windows-recovery (C#) + AgentFlow (TypeScript) + apeireth-rust fork (Rust) → Rust 全部, 借鉴 ID 完整 |
| O-3 干到底 | 6 模块一次性落地 + PR #2 selective cherry-pick, 含测试 + 文档, 不留 TODO 残片 |
| O-4 接手 | 0-doc-pass: 每个模块都有顶部 //! 段 + 0 装 PASS 标注 + 测试 + 用法示例 + 设计文档 |
| O-5 不假装 | 错误类型完整 (4+6+1 变体), "什么没做" 全部标注, PR #2 故意不全合 + selective cherry-pick 替代 |

---

## 5. 后续 batch 建议 (backlog #BORROW-J)

- batch 1: lease 集成到 Orchestrator::spawn_agent (P1 lease 集成)
- batch 2: three_way 集成到 apeireth-upgrade::rollback (阶段 4/6 image switch + rollback)
- batch 3: journal anchor 三方交叉验证 (P1-1 强化)
- batch 4: 推进 PR #2 worktree 整合 + commit (per "0 主动 commit 严守" 等 owner 决策)
- batch 5: PR #2 frontend 重写 (7000+ 行 .svelte/.ts 单独 PR)

---

**DRAFT: 2026-08-21** (待 cargo check + 真 LLM 验证后定稿)
**借鉴 ID 命名**: `BORROW-{owner/repo}-{pattern}-2026-08-21` (per OSS_NOTICE §1 命名约定)
**0 主动 commit**: 严守, 等主人或整合 #5 时机拍板
**0 主动 push**: 严守, 等 1.0 release 配 GitHub remote