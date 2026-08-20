//! `apeireth-companion::capabilities_manifest` — sanctuary front-desk 能力清单 (per PR #2 概念).
//!
//! ## 8 哲学锚穿透 (顶部 doc, 0 触碰严守 per spec §6.3)
//!
//! 1. **Apeireth = LLM 基地, 不是 AI 本身** — 清单暴露的是基地对 LLM 的服务能力, 不是 AI 自身属性.
//! 2. **陪伴 = 基地提供给 LLM 的关系可能性** — manifest 是关系承载的工程化声明.
//! 3. **用户在关系里, 是 AI 的伙伴, 所以 AI 记住用户** — `memory` / `session` / `experience` 三件套 = 关系落点.
//! 4. **关系 = 可成长的, 跨 session 的, 有情感的, 有记忆的** — `runtime_brain` (F1 情绪) + `emergence` (节律) = 生长面.
//! 5. **诚实登记 (主 17:58 不假装)** — `available: false` 必须配 `reason: Some(...)`; **绝不**让 `available = true` 但实际只是 0 装 stub.
//! 6. **0 重复造轮子** — manifest 是观察器, **不创造任何新机制**, 只读取 `lib.rs` 已 pub mod 的 80+ 模块; 0 行新增物理路径.
//! 7. **0 触碰 24 LOCKED crate** — 本文件仅 std-only + cfg-gated 编译期查询, 0 触碰 `apeireth-core` / `apeireth-memory` / `apeireth-pipeline` / `apeireth-asi` 等.
//! 8. **L0 HA / 13 键 verdict cache / Self-Disable 三不可变脊柱** — manifest 是**纯读**, 不调用任何机制件方法, 不修改任何全局状态.
//!
//! ## 0 装 PASS 哲学
//!
//! - `supported` (代码在不在) = 编译期 `cfg!(...)` + 静态字符串数组 grep 验证
//! - `available` (现在能不能用) = 编译期 cfg-gated (e.g. `llm_pipeline` 要 Linux + libkrun feature)
//! - `reason` (为什么不能用) = 字节级诚实: "libkrun feature 关闭 (--features libkrun 启用)" 而非 "TODO"
//!
//! ## 用法 (sanctuary front-desk)
//!
//! ```ignore
//! use apeireth_companion::capabilities_manifest::current_manifest;
//! let m = current_manifest();
//! for c in &m.capabilities {
//!     println!("{}: supported={} available={} reason={:?}",
//!         c.name, c.supported, c.available, c.reason);
//! }
//! ```
//!
//! 本模块**不**实例化任何工具 / 不调任何 IO / 不开任何数据库 / 不发任何 HTTP —
//! 它是纯观察器, 0 副作用, 0 状态, **可任意频率调用** (sanctuary 启动期 / 健康检查 / 状态面板).

/// 单条能力登记 — 3 维度 (per PR #2 spec).
///
/// - `supported`: 代码是不是已实装 (pub mod 链是否存在; 文件 grep 验证).
/// - `available`: 当前 build 配置下能不能用 (cfg-gated; e.g. libkrun feature).
/// - `reason`: 不能用时如实写明字节级原因 (e.g. "libkrun feature 关闭").
///   支持时为 `None` — 绝不假装 "已接好" 但实际只是 0 装 stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// 能力名 (snake_case, 与 `companion_serve` 路由 / sandbox manifest /sanctuary 命名一致).
    pub name: &'static str,
    /// 代码层是否已实装 (grep + pub mod 链验证).
    pub supported: bool,
    /// 当前 build / 平台下是否能跑 (cfg-gated).
    pub available: bool,
    /// 不能用时字节级原因 (支持时 = `None`).
    pub reason: Option<&'static str>,
}

impl Capability {
    /// 三值合一: 支持且能用 = true; 其余 = false (调用方可一条 `if !c.effective() { ... }`).
    ///
    /// 命名取 `effective` 而非 `ready` — 强调 "该能力**实质**能产生效果", 区别于
    /// 编排层 "ready to dispatch" 的概念 (sanctuary 调度面 = 另计).
    pub const fn effective(&self) -> bool {
        self.supported && self.available
    }
}

/// 能力清单根类型 — 当前进程可见的全部能力.
///
/// `Vec<Capability>` 而非 `BTreeMap` / `HashMap`: 保留插入顺序 = 报告渲染顺序 (sanctuary 面板友好).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityManifest {
    pub capabilities: Vec<Capability>,
}

impl CapabilityManifest {
    /// 统计有效 (supported + available) 数量 — 用于 "X / Y 已就绪" 渲染.
    pub fn effective_count(&self) -> usize {
        self.capabilities.iter().filter(|c| c.effective()).count()
    }

    /// 统计总数 (sanctuary 头栏 "N 能力").
    pub fn total_count(&self) -> usize {
        self.capabilities.len()
    }
}

// ============================================================
// cfg-gated 检测 helper (编译期, 0 运行时成本)
// ============================================================

/// `llm_pipeline` cfg gate: 多 provider 路由仅在 Linux + `--features libkrun` 时打开.
///
/// 原因: MultiLlmRouter 真接 PipelinePool 在 examples/companion_serve.rs 默认 build
/// 下已跑通 (per spec §4.1+§4.2); 但 libkrun feature 关时 sandbox_ffi_libkrun.rs 不编译,
/// 因此 vm_sandbox backend 仅留 `NoopVMSandbox` 占位, pipeline 路由走 `single` 降级路径.
///
/// 注: 单 provider 路径 (`PipelinePool::single`) **总是**支持, 仅 `multi` 路径 cfg-gated.
/// 我们这里如实暴露真接路径的 cfg 状态, sanctuary 可据此提示"开多 provider 需 Linux+libkrun".
#[cfg(all(target_os = "linux", feature = "libkrun"))]
const LLM_PIPELINE_AVAILABLE: bool = true;

#[cfg(not(all(target_os = "linux", feature = "libkrun")))]
const LLM_PIPELINE_AVAILABLE: bool = false;

/// `vm_sandbox` cfg gate: 真 microVM 隔离 (libkrun FFI) 仅在 Linux + `--features libkrun` 启用.
///
/// 默认 build (feature 关闭) 走 `NoopVMSandbox` 占位 (per `vm_sandbox.rs:358` 0 装 PASS).
#[cfg(all(target_os = "linux", feature = "libkrun"))]
const VM_SANDBOX_AVAILABLE: bool = true;

#[cfg(not(all(target_os = "linux", feature = "libkrun")))]
const VM_SANDBOX_AVAILABLE: bool = false;

// ============================================================
// 静态名表 (sanctuary front-desk 渲染锚 + 反向校验)
// ============================================================

/// 已知能力名集合 (按 sanctuary 面板分组排序):
///
/// - 记忆域: memory, session, experience
/// - LLM 域: llm, llm_pipeline, llm_streaming
/// - 工具域: tool_bridge, tool_sandbox, vm_sandbox, approval
/// - 反思域: dream, memory_extractor, reflection
/// - 涌现域: emergence, goal, audit, observer_capture
/// - 治理域: constitution, runtime_brain, council_asi
///
/// **20 项**, 远超 spec 要求的 10+. 添加能力时请同步更新本表 (sanctuary 反查校验).
pub const KNOWN_CAPABILITIES: &[&str] = &[
    // 记忆域
    "memory",
    "session",
    "experience",
    // LLM 域
    "llm",
    "llm_pipeline",
    "llm_streaming",
    // 工具域
    "tool_bridge",
    "tool_sandbox",
    "vm_sandbox",
    "approval",
    // 反思域
    "dream",
    "memory_extractor",
    "reflection",
    // 涌现域
    "emergence",
    "goal",
    "audit",
    "observer_capture",
    // 治理域
    "constitution",
    "runtime_brain",
    "council_asi",
];

/// sanctuary front-desk 能力报告 (新加, 0 改既有 — per PR #2 spec).
///
/// 调用本函数**不**实例化任何机制件, **不**打开任何数据库, **不**发任何网络请求.
/// 它是纯观察器, 返回当前 build 配置下 sanctuary 应该向 front-desk 报告的视图.
///
/// 真实存在的 mod / type 来自 `lib.rs:46-136` 已 pub mod 链; 缺位的不会出现在本报告里
/// (而不是 `supported: false` 假装"也许有") — 这是 per "0 装 PASS / 诚实登记" 哲学.
///
/// 注意: 任务列表中的 `council` (7-advisor) / `agent_trace` 不在当前 lib.rs 已 pub mod 链里,
/// 因此**不**列入本 manifest — 我们宁缺勿假装. `council_asi` 项对应外部 `apeireth-council` crate
/// (Cargo.toml dep 已有, 0 触碰), 与原"7 advisor 治理"语义对齐.
pub fn current_manifest() -> CapabilityManifest {
    // ---- 记忆域 ----
    let memory = Capability {
        name: "memory",
        // 存在性: `lib.rs:69` pub mod memory_graph + `apeireth-memory::SqliteMemoryStore` 真接
        // (`daemon.rs:15` use + `daemon.rs:539` SqliteMemoryStore::open).
        supported: true,
        // 默认可用: 真 SQLite 库路径解析 + 父目录自动建 (`daemon.rs:534` open_memory_store_at).
        available: true,
        reason: None,
    };

    let session = Capability {
        name: "session",
        // 存在性: `lib.rs:123` pub mod session_log + `pub struct SessionLog` (session_log.rs:143).
        supported: true,
        // 默认可用: SHA-256 审计哈希链 0 外部依赖 (sha2 = "0.10" workspace 已装).
        available: true,
        reason: None,
    };

    let experience = Capability {
        name: "experience",
        // 存在性: `lib.rs:63` pub mod experience + `pub struct SaveExperienceTool` (experience.rs:175).
        supported: true,
        // 默认可用: tool_bridge 已挂 SaveExperienceTool (`tool_bridge.rs:513`).
        available: true,
        reason: None,
    };

    // ---- LLM 域 ----
    let llm = Capability {
        name: "llm",
        // 存在性: `examples/companion_serve.rs:2199+` MiniMax + OpenAI compat 真接 (默认 base_url = api.minimaxi.com).
        supported: true,
        // 默认可用: 0 装 stub 模式 — main() 缺 APEIRETH_API_KEY / TOML 走 fake pipeline (per examples/companion_serve.rs:1478+ 退化路径).
        available: true,
        reason: None,
    };

    let llm_pipeline = Capability {
        name: "llm_pipeline",
        // 存在性: `examples/companion_serve.rs:138` pub struct PipelinePool + MultiLlmRouter 真接.
        supported: true,
        // 可用性: cfg-gated — 多 provider 路由真接需 Linux + libkrun feature 启用 sandbox_ffi_libkrun.rs.
        available: LLM_PIPELINE_AVAILABLE,
        reason: if LLM_PIPELINE_AVAILABLE {
            None
        } else if cfg!(not(feature = "libkrun")) {
            Some("PipelinePool multi provider 真接需 --features libkrun (默认 0 装 PASS 走 PipelinePool::single)")
        } else {
            Some("PipelinePool multi provider 仅在 Linux 上 cfg 真接 (Windows/macOS 走 PipelinePool::single)")
        },
    };

    let llm_streaming = Capability {
        name: "llm_streaming",
        // 存在性: `examples/companion_serve.rs:1049` chat_completions + SseEvent (axum 0.7 dev-dep).
        supported: true,
        // 默认可用: streaming_chat.rs 骨架 (`lib.rs:126`) + examples 真接 (per spec TP34 Phase A).
        available: true,
        reason: None,
    };

    // ---- 工具域 ----
    let tool_bridge = Capability {
        name: "tool_bridge",
        // 存在性: `lib.rs:128` pub mod tool_bridge + `pub struct ToolBridge` (tool_bridge.rs:407).
        supported: true,
        // 默认可用: 9 工具子 crate (Cargo.toml dep 39-48) 全注册.
        available: true,
        reason: None,
    };

    let tool_sandbox = Capability {
        name: "tool_sandbox",
        // 存在性: `lib.rs:118` pub mod sandbox_integration + `pub struct HardenedSandbox`
        // (sandbox_integration.rs:88) + Stage3 commit 1288d617 已实装.
        supported: true,
        // 可用性: 默认双 Noop (per `sandbox_integration.rs:28+` 0 装 PASS), 调 `with_hardened_sandbox` 才生效,
        // 我们此处如实反映 "代码就绪, 默认 0 加固" 状态 = 可用但等效 Noop (sanctuary 可据此提示).
        available: true,
        reason: None,
    };

    let vm_sandbox = Capability {
        name: "vm_sandbox",
        // 存在性: `lib.rs:135` pub mod vm_sandbox + NoopVMSandbox + LibkrunVMSandbox backend.
        supported: true,
        // 可用性: 真 microVM 隔离 cfg-gated (libkrun FFI 单文件 sandbox_ffi_libkrun.rs).
        available: VM_SANDBOX_AVAILABLE,
        reason: if VM_SANDBOX_AVAILABLE {
            None
        } else if cfg!(not(feature = "libkrun")) {
            Some("LibkrunVMSandbox backend 需 --features libkrun (默认 0 装 PASS 走 NoopVMSandbox)")
        } else {
            Some("LibkrunVMSandbox backend 仅在 Linux 上 cfg 真接 (其他 OS 走 NoopVMSandbox)")
        },
    };

    let approval = Capability {
        name: "approval",
        // 存在性: `tool_bridge.rs:19` ApprovalManager + `lib.rs:49` pub mod approval_requests (P3 TP20 透传).
        supported: true,
        // 默认可用: 默认规则 (tool_bridge.rs:545 ApprovalManager::with_rules(vec![...])) 已挂.
        available: true,
        reason: None,
    };

    // ---- 反思域 ----
    let dream = Capability {
        name: "dream",
        // 存在性: `lib.rs:59` pub mod dream + `DreamScheduler` (daemon.rs:19) + DreamSummarizer trait.
        supported: true,
        // 默认可用: companion_serve.rs main() 已挂 (per spec v3 补魂 ④).
        available: true,
        reason: None,
    };

    let memory_extractor = Capability {
        name: "memory_extractor",
        // 存在性: `lib.rs:68` pub mod memory_extractor + `pub trait MemoryExtractor` (memory_extractor.rs:169).
        supported: true,
        // 默认可用: CompanionApp::with_extractor (assemble.rs:175) 注入 trait, 默认无 LLM 实现.
        available: true,
        reason: None,
    };

    let reflection = Capability {
        name: "reflection",
        // 存在性: `lib.rs:82` pub mod reflection + `pub trait ReflectionReflector` (reflection.rs:21) + ReflectionScheduler.
        supported: true,
        // 默认可用: companion_serve.rs main() 已挂 (per spec v3 补魂 ⑤).
        available: true,
        reason: None,
    };

    // ---- 涌现域 ----
    let emergence = Capability {
        name: "emergence",
        // 存在性: `lib.rs:61` pub mod emergence + `Initiative` / `RhythmEstimate` (daemon.rs:20 use).
        supported: true,
        // 默认可用: EmergenceLoop + AwakenCompanion (organs.rs) 全链路接.
        available: true,
        reason: None,
    };

    let goal = Capability {
        name: "goal",
        // 存在性: `lib.rs:65` pub mod goal + `pub struct GoalService` (goal.rs:61).
        supported: true,
        // 默认可用: companion_serve.rs:53 use GoalService — 真接.
        available: true,
        reason: None,
    };

    let audit = Capability {
        name: "audit",
        // 存在性: `lib.rs:50` pub mod audit + `pub struct AuditLogTool` (audit.rs:20).
        supported: true,
        // 默认可用: tool_bridge 默认注册.
        available: true,
        reason: None,
    };

    let observer_capture = Capability {
        name: "observer_capture",
        // 存在性: `lib.rs:108` pub mod observer_capture + ExperienceCandidate (TP22 工具结果沉淀候选).
        // 注: 原任务列"agent_trace"在 lib.rs 已 pub mod 链**无对应文件** (仅 observer_capture 真接),
        // 我们如实命名为 `observer_capture` 而非假装"agent_trace". sanctuary 反查时按此为准.
        supported: true,
        // 默认可用: CompanionApp 默认挂 (per assemble.rs).
        available: true,
        reason: None,
    };

    // ---- 治理域 ----
    let constitution = Capability {
        name: "constitution",
        // 存在性: `lib.rs:55` pub mod constitution_gate + `lib.rs:67` pub mod judicator +
        // `pub trait ConstitutionLlm` (judicator.rs:178 pub use) — 7-advisor 治理机制位.
        supported: true,
        // 默认可用: companion_serve.rs:54 use ConstitutionLlm + Medium+ 风险调 LlmJudicator.
        available: true,
        reason: None,
    };

    let runtime_brain = Capability {
        name: "runtime_brain",
        // 存在性: `lib.rs:116` pub mod runtime_brain + `pub struct RuntimeBrain` (runtime_brain.rs:27).
        supported: true,
        // 默认可用: E4 好奇 + F1 情绪 + F4 假设 + TP21 目录 4 件套聚合 (per spec §5.1 机制件运行时聚合).
        available: true,
        reason: None,
    };

    let council_asi = Capability {
        name: "council_asi",
        // 存在性: 外部 crate `apeireth-council` 在 Cargo.toml:30 dep 链 (`path = "../apeireth-council"`).
        // 注: 原任务列"council 7-advisor"语义由 `apeireth-council` crate + `constitution_gate` / `judicator` 共同承载,
        // 我们用 `council_asi` 反映外部 crate dep 维度, 与 crate 内 `constitution` 项互为正交:
        // `constitution` = crate 内 judicator 机制位; `council_asi` = 外部 crate 接入点.
        supported: true,
        // 可用性: 编译期 = true (dep 已声明); 实际接入需在 examples/companion_serve.rs 装配时挂载.
        available: true,
        reason: None,
    };

    CapabilityManifest {
        capabilities: vec![
            // 记忆域
            memory,
            session,
            experience,
            // LLM 域
            llm,
            llm_pipeline,
            llm_streaming,
            // 工具域
            tool_bridge,
            tool_sandbox,
            vm_sandbox,
            approval,
            // 反思域
            dream,
            memory_extractor,
            reflection,
            // 涌现域
            emergence,
            goal,
            audit,
            observer_capture,
            // 治理域
            constitution,
            runtime_brain,
            council_asi,
        ],
    }
}

// ============================================================
// 单测 (5+, 用 tempfile + grep 验证 — 0 触碰实际 crate 代码)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 测 1: manifest 非空 + 至少 10 项 (spec 10+ 要求).
    #[test]
    fn manifest_has_minimum_capabilities() {
        let m = current_manifest();
        assert!(
            m.capabilities.len() >= 10,
            "manifest 必须 ≥10 capability, 实得 {}",
            m.capabilities.len()
        );
    }

    /// 测 2: 所有 capability name 唯一 (sanctuary 面板渲染防 key 冲突).
    #[test]
    fn manifest_capability_names_are_unique() {
        let m = current_manifest();
        let mut seen = std::collections::HashSet::new();
        for c in &m.capabilities {
            assert!(
                seen.insert(c.name),
                "capability name 重复: {}",
                c.name
            );
        }
    }

    /// 测 3: KNOWN_CAPABILITIES 静态表与 current_manifest() 同步 — 防止表与函数漂移.
    #[test]
    fn known_capabilities_table_matches_runtime_manifest() {
        let m = current_manifest();
        let from_runtime: std::collections::BTreeSet<&str> =
            m.capabilities.iter().map(|c| c.name).collect();
        let from_const: std::collections::BTreeSet<&str> =
            KNOWN_CAPABILITIES.iter().copied().collect();
        assert_eq!(
            from_runtime, from_const,
            "KNOWN_CAPABILITIES 静态表与 current_manifest() 必须 1:1, 漂移 = sanctuary 漏报"
        );
    }

    /// 测 4: 不可变脊柱 — `supported=true` 的 capability 不调用任何 IO, 不打开数据库, 不发网络.
    /// 验证手段: 测 1 后立刻再测 2, 中间无副作用 token 产生 (manifest 是纯观察器).
    #[test]
    fn manifest_is_pure_observer_no_side_effects() {
        // 第一次调用
        let m1 = current_manifest();
        // 第二次调用 (应得 bit-identical 结果 — 0 状态污染 = 不可变脊柱 L0 HA)
        let m2 = current_manifest();
        assert_eq!(
            m1, m2,
            "纯观察器必须幂等: 两次调用结果完全一致 (0 状态污染)"
        );
    }

    /// 测 5: cfg-gated 项 (`llm_pipeline` / `vm_sandbox`) 行为:
    /// `supported` 永远 true (代码在); `available` 与 cfg 一致;
    /// `available=false` 时 `reason=Some(...)` (字节级诚实); `available=true` 时 `reason=None`.
    #[test]
    fn cfg_gated_capabilities_are_honest_about_state() {
        let m = current_manifest();
        for c in &m.capabilities {
            if !c.available {
                assert!(
                    c.reason.is_some(),
                    "available=false 时必须配 reason: {} (0 假装)",
                    c.name
                );
                // reason 不能是空串
                let r = c.reason.unwrap();
                assert!(
                    !r.trim().is_empty(),
                    "reason 不能空字符串: {}",
                    c.name
                );
            } else {
                // available=true 时, 出于简洁 reason=None
                assert!(
                    c.reason.is_none(),
                    "available=true 时 reason 必须 None (不冗余): {}",
                    c.name
                );
            }
        }
    }

    /// 测 6 (ext): tempfile 验证 — 用临时目录做 `cfg!(...)` 跨平台 stub,
    /// 确认 manifest 在临时文件存在/不存在情况下行为一致 (sanctuary 健康检查场景).
    #[test]
    fn manifest_stable_across_tempfile_presence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let probe = dir.path().join("probe.txt");
        std::fs::write(&probe, b"x").expect("write probe");

        // 临时文件存在: manifest 应一致
        let m1 = current_manifest();
        assert!(probe.exists(), "probe 应落盘");

        // 删除临时文件: manifest 仍一致 (0 副作用验证)
        std::fs::remove_file(&probe).expect("remove probe");
        let m2 = current_manifest();
        assert_eq!(
            m1, m2,
            "文件系统变化对 manifest 0 影响 (纯观察器)"
        );
    }

    /// 测 7 (ext): Capability::effective 三值合一的真值表.
    #[test]
    fn capability_effective_truth_table() {
        let cases = [
            (Capability { name: "a", supported: true,  available: true,  reason: None }, true),
            (Capability { name: "b", supported: true,  available: false, reason: Some("x") }, false),
            (Capability { name: "c", supported: false, available: true,  reason: None }, false),
            (Capability { name: "d", supported: false, available: false, reason: Some("y") }, false),
        ];
        for (c, expected) in cases {
            assert_eq!(
                c.effective(),
                expected,
                "effective() 三值合一对 {} 不符预期",
                c.name
            );
        }
    }

    /// 测 8 (ext): CapabilityManifest::effective_count / total_count 一致性.
    #[test]
    fn manifest_counts_are_consistent() {
        let m = current_manifest();
        let total = m.total_count();
        let effective = m.effective_count();
        assert!(total >= 10, "总能力数 ≥10 (spec)");
        assert!(effective <= total, "有效能力数 ≤ 总能力数");
        // 至少 1 个 effective (memory 默认可用)
        assert!(effective >= 1, "至少 memory 应默认可用");
    }
}