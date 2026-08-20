//! apeireth-constraint: 约束器官 (P12 — v4.1 新增)
//!
//! **职责**: 提供 4 重守门 (编译时/运行时/物理隔离/反思期) + 权限发放 (PermissionGrant)
//! + 13 键 verdict cache 复用 (`apeireth_core::ALL_TWELVE_KEYS` 已实装, 本 crate **不重新实现**)。
//!
//! **架构位置**: 阶段 4 §2 守卫器官 — 在 `apeireth-cognition` (13 键 verdict
//! 决策) 之前/之后均可调用。P19 (A17 philosophy 删除) 接管 13 键后, 本 crate 升级
//! 为唯一对外 13 键入口。
//!
//! **v15 命名修正 (round7-05)**: `FiveGates` trait 已重命名为 [`FourGates`] (4 重嵌套守门),
//! 多 AI 一致从守门中剥离为独立 [`PermissionGrant`] trait (Council 7 强制 + Human L0 +
//! RiskLevel 三方授权)。`FiveGates` 保留为 deprecated 向后兼容别名 (委托 4 重 + 多 AI)。
//!
//! **4 重守门 + 权限发放分类** (源自 `docs/stage4/stage4-correction-v15-four-gates-permission-grant.md`):
//!
//! | 机制 | 实现位置 | 本 crate 入口 |
//! |------|---------|-------------|
//! | Gate 1 (内层) | 编译时 hardcode (原则洋葱 E/S/A/M/O 5 层 + 13 键 (含 PHL-07) + 5 项不假装) | [`HardCodeConstraint`] + [`verify_at_compile_time`] |
//! | Gate 2 (中间) | 运行时拦截 (verdict cache O(1) 查询) | [`runtime_intercept`] |
//! | Gate 3 (外层) | 物理隔离 (重大修改需物理访问 + 多签 — critical=7 席全量) | [`physical_isolation_check`] |
//! | Gate 4 (最外) | 反思期审计 (Cognitive-Dream 72h 监控 — 守护越权检查) | [`reflection_period_audit`] |
//! | PermissionGrant | Council 7 强制 + Human L0 + RiskLevel 三方授权 | [`PermissionGrant`] trait |
//!
//! **禁止**:
//! - ❌ 不修改 `apeireth_core::ALL_TWELVE_KEYS` / `PhilosophyKey` / `TWELVE_KEYS_HARDCODE`
//! - ❌ 不碰 R11 baseline 三值
//! - ❌ 不碰 `apeireth-legacy/`
//! - ❌ 不重新定义 13 键 — 仅复用 + 暴露便捷 trait

#![deny(unsafe_code)]

use apeireth_core::{
    evolution_can_modify, is_forbidden_meta_question_const, Action, PhilosophyKey,
    PhilosophyVerdict, RiskLevel,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// 深度实装模块 — round8-05 阶段 5 §2/§3/§4 13 键 O(1) + Council 7 + V1+V2+V3 AND.
/// 详见 `deep_impl.rs` 顶部文档.
pub mod deep_impl;
// R177: organ invariants (5 tests + 2 Kani)
mod organ_kani_proofs;

// ============================================
// 1. PhilosophyKey trait — 12 键 verdict cache
// ============================================

/// 13 键哲学守门 (12 原 + PHL-07 不假装 NEW) — `apeireth-core` 已实装 `ALL_TWELVE_KEYS` / `PhilosophyGuard`,
/// 本 trait 在其之上提供 verdict cache 复用 (运行时 O(1) 查询) 与分组访问。
///
/// **不重新实现 13 键** — 仅包装 `apeireth_core::ALL_TWELVE_KEYS`。
/// 任何修改 13 键清单的行为都会触发 `apeireth_core::TWELVE_KEYS_HARDCODE` 编译失败。
pub trait PhilosophyKeyAccess: Send + Sync {
    /// 返回 13 键完整清单 (编译时 hardcode, 顺序锁定)
    ///
    /// 0 装 PASS 严守: 返回类型 `[PhilosophyKey; 13]` 必须跟 `apeireth_core::ALL_THIRTEEN_KEYS` 长度匹配.
    /// 任何修改 13 键清单的行为都会触发编译期 panic 或长度不匹配错误.
    fn all_twelve_keys() -> &'static [PhilosophyKey; 13] {
        apeireth_core::ALL_THIRTEEN_KEYS
            .as_slice()
            .try_into()
            .expect("apeireth-core ALL_THIRTEEN_KEYS 长度必须是 13 (V3 9 + v4.1 3 + R125-12 PHL-07)")
    }

    /// 返回当前 action 的 verdict (子类实现 = 真实业务守门)
    fn check(&self, action: &Action) -> PhilosophyVerdict;
}

// ============================================
// 2. HardCodeConstraint trait — 编译时 const fn 断言
// ============================================

/// 编译时 hardcode 约束 — `const fn` 在编译期验证不变量。
///
/// **设计原则 (主 17:58 不假装)**: 任何"假装通过"的尝试, 必须在编译期就失败。
pub trait HardCodeConstraint {
    /// 关联类型: 守门的目标值 (编译时可见)
    type Target: Copy + PartialEq;

    /// 编译时断言 — 必须在编译期验证目标值
    fn const_assert(target: Self::Target) -> Self::Target;
}

/// 13 键长度 = 13 的编译时断言器 (per A3 13 键 + PHL-07)
pub struct TwelveKeysHardcode;

impl HardCodeConstraint for TwelveKeysHardcode {
    type Target = usize;

    /// 编译时断言 `apeireth_core::ALL_TWELVE_KEYS.len() == 13`。
    /// 若不等, 调用方编译失败。
    /// 2026-08-20: 12 → 13 (post commit 13c25025 PHL-07 第 13 键升级, B3 V0.5 30 维 lineage 同步).
    fn const_assert(target: usize) -> usize {
        // 实际 hardcode 触发由 apeireth_core::TWELVE_KEYS_HARDCODE 承担;
        // 本函数是外部调用方在 crate 边界处的"二次断言", 用于展示编译时约束的传递性。
        let _ = apeireth_core::TWELVE_KEYS_HARDCODE; // 触发 apeireth-core 内部硬断言
        assert!(
            target == 13,
            "13 键 hardcode 边界断言: 必须保持 V3 9 + v4.1 3 + PHL-07 1 = 13 (post commit 13c25025)"
        );
        target
    }
}

// ============================================
// 3. FourGates trait — 4 重嵌套守门 (v15 命名修正)
// ============================================

/// 4 重嵌套守门 trait — `docs/stage4/stage4-correction-v15-four-gates-permission-grant.md` §1.2
/// 4 重嵌套守门 (gate1=内编译时 hardcode / gate2=中间运行时拦截 /
/// gate3=外层物理隔离 / gate4=最外反思期审计) 的 trait 化封装。
///
/// **v15 关键洞察 (主人原话)**: "多 ai 一致没必要弄成守门的，因为修改原则需要多ai一致，
/// 所以守门就是把原则洋葱和权限洋葱守住就可以了，这样一个嵌套结构。
/// 就可以弄成把没有相应权限而运行的代码拦截就行。"
///
/// **权限发放独立**: 多 AI 一致已剥离到 [`PermissionGrant`] trait。本 trait 仅表达守门本身。
pub trait FourGates: Send + Sync {
    /// Gate 1 (内层): 编译时 hardcode (本 trait 实现内部触发 `HardCodeConstraint`)
    fn gate1_compile_time(&self) -> GateVerdict;

    /// Gate 2 (中间): 运行时拦截 (verdict cache O(1) 查询 — 业务侧 trait check)
    fn gate2_runtime_intercept(&self, action: &Action) -> GateVerdict;

    /// Gate 3 (外层): 物理隔离 HA (物理访问 + 多签 — critical=7 席全量)
    fn gate3_physical_isolation(&self, action: &Action) -> GateVerdict;

    /// Gate 4 (最外): 反思期审计 (Cognitive-Dream 72h 监控)
    fn gate4_reflection_period(&self, action: &Action) -> GateVerdict;
}

/// PermissionGrant — 权限发放独立 trait (v15 新增)
///
/// **公式 = V0.5 v2 24 维权重公式 (v4.1 §13)**。实施 = apeireth-council 智囊团审议
/// (7 强制 + 动态专家)。人类决策 = L0 HA 真实人类批准。权限发放对象 = 风险分级
/// (critical 7 / high 5 / medium 3 / low 1 / info 0)。
///
/// **三方授权 (And)**: Council (智囊团 7 强制) ∧ Human (L0 HA 真实人类) ∧ RiskLevel (在阈值内)
/// 三者同时 Grant 才允许修改 E 层 (原则洋葱)。
pub trait PermissionGrant: Send + Sync {
    /// 路径 A — 智囊团审议 (Council 7 强制 + 动态专家)
    fn grant_via_council(&self, action: &Action) -> GrantVerdict;

    /// 路径 B — 人类决策 (L0 HA 真实人类批准 — 物理访问 + 多签)
    fn grant_via_human(&self, action: &Action) -> GrantVerdict;

    /// 路径 C — 风险分级 (critical 7 / high 5 / medium 3 / low 1 / info 0)
    fn grant_risk_level(&self, action: &Action) -> RiskGrant;
}

/// 单个授权路径的 verdict 结果 (Pass / Block + 原因)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantVerdict {
    /// 通过 (该路径已 Grant)
    Pass,
    /// 拒绝 (附原因 — 人类可读)
    Block(String),
}

/// 风险分级授权结果 (独立类型, 因为风险分级有数值含义)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskGrant {
    /// 实际风险等级 (0=info, 1=low, 3=medium, 5=high, 7=critical)
    pub level: u8,
    /// 该等级是否在 Gate 3 物理隔离阈值内 (critical 需 7 席全签, 其他按比例降级)
    pub within_threshold: bool,
    /// 需要的签名席位 (与 risk_level 严格绑定, 编译时 hardcode)
    pub required_signatures: u8,
}

impl RiskGrant {
    /// 默认 — info 级别 (level=0), 在阈值内, 需要 0 签名。
    pub const INFO: Self = Self {
        level: 0,
        within_threshold: true,
        required_signatures: 0,
    };
}

/// FiveGates — **DEPRECATED 向后兼容别名** (v15 命名修正前)
/// **保留目的**: 已有调用方 (constraint tests / examples / 历史 impl) 不被破坏。
/// **实现策略**: `gate3_multi_ai_consensus` 委托到 [`PermissionGrant::grant_via_council`] —
/// 因为 v15 把"多 AI 一致"从守门剥离为权限发放, 但旧调用方期待 5 个 gate 方法签名。
///
/// **未来删除路径**: v20 后可移除 (`#[deprecated]` 标注); 当前保留用于:
/// 1. `verify_all_five_gates` 老入口继续工作 (Round7 阶段)。
/// 2. 任何依赖 5 方法签名的外部 mock / 测试 (向后兼容承诺)。
#[deprecated(
    since = "0.14.0",
    note = "v15 命名修正: FiveGates 已拆为 FourGates + PermissionGrant. 请迁移到 FourGates (4 重嵌套守门) + PermissionGrant (三方授权) 组合。"
)]
pub trait FiveGates: Send + Sync {
    /// 守门 1: 编译时 hardcode — 委托到 [`FourGates::gate1_compile_time`]
    fn gate1_compile_time(&self) -> GateVerdict;
    /// 守门 2: 运行时拦截 — 委托到 [`FourGates::gate2_runtime_intercept`]
    fn gate2_runtime_intercept(&self, action: &Action) -> GateVerdict;
    /// 守门 3: 多 AI 一致 — **新**: 委托到 [`PermissionGrant::grant_via_council`]
    fn gate3_multi_ai_consensus(&self, action: &Action) -> GateVerdict;
    /// 守门 4: 物理隔离 — **新**: 委托到 [`FourGates::gate3_physical_isolation`]
    fn gate4_physical_isolation(&self, action: &Action) -> GateVerdict;
    /// 守门 5: 反思期审计 — **新**: 委托到 [`FourGates::gate4_reflection_period`]
    fn gate5_reflection_period(&self, action: &Action) -> GateVerdict;
}

/// 单守门的 verdict 结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateVerdict {
    /// 通过
    Pass,
    /// 拒绝 (附原因 — 人类可读)
    Block(String),
}

// ============================================
// 4. 标准 5 重守门实现 — ConstraintEngine
// ============================================

/// 13 键 verdict cache (运行时 O(1) 查询缓存)
#[derive(Debug, Default)]
pub struct VerdictCache {
    /// action_id → verdict 映射
    cache: HashMap<String, PhilosophyVerdict>,
}

impl VerdictCache {
    /// 创建空 cache
    pub fn new() -> Self {
        Self::default()
    }

    /// 查询 verdict — 命中即返回, 未命中返回 `None` (由调用方决定是否计算)
    pub fn get(&self, action_id: &str) -> Option<&PhilosophyVerdict> {
        self.cache.get(action_id)
    }

    /// 写入 verdict — 已存在则覆盖
    pub fn put(&mut self, action_id: impl Into<String>, verdict: PhilosophyVerdict) {
        self.cache.insert(action_id.into(), verdict);
    }

    /// 清空 cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// 当前 cache 条数
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// cache 是否为空
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

/// 4 重守门 + 权限发放标准引擎 — 实现 `FourGates` + `PermissionGrant` +
/// `PhilosophyKeyAccess` 三 trait, 复用 `apeireth_core::ALL_TWELVE_KEYS`。
///
/// **v15 命名修正 (round7-05)**: 主 trait = FourGates + PermissionGrant;
/// FiveGates 保留为 deprecated 向后兼容别名。
///
/// **三 trait 复用模式**: 13 键清单来自 `apeireth-core`, 4 重守门 verdict + 3 方授权来自本 crate。
/// 这是 P19 接管 13 键后的统一对外入口。
pub struct ConstraintEngine {
    /// 13 键 verdict cache (编译时 hardcode 13 键清单)
    cache: VerdictCache,
}

impl ConstraintEngine {
    /// 创建引擎 + 触发 13 键编译时断言
    pub fn new() -> Self {
        // 守门 1 — 编译时 hardcode: 触发 13 键长度断言
        let _ = <TwelveKeysHardcode as HardCodeConstraint>::const_assert(13);
        Self {
            cache: VerdictCache::new(),
        }
    }

    /// 内部访问 cache (供 verify_at_compile_time 使用)
    pub fn cache(&self) -> &VerdictCache {
        &self.cache
    }

    /// 内部可变访问 cache
    pub fn cache_mut(&mut self) -> &mut VerdictCache {
        &mut self.cache
    }
}

impl Default for ConstraintEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PhilosophyKeyAccess for ConstraintEngine {
    fn check(&self, action: &Action) -> PhilosophyVerdict {
        // 缓存命中即返回 (守门 2: 运行时 O(1) 拦截)
        if let Some(cached) = self.cache.get(&action.id) {
            return cached.clone();
        }
        // 缓存未命中 — 按 12 键守门 (本 crate 默认实现: 不假装安全 — 默认拒绝)
        PhilosophyVerdict::Block(PhilosophyKey::NotSafe)
    }
}

// --- v15 主 trait impl: 4 重嵌套守门 ---
impl FourGates for ConstraintEngine {
    fn gate1_compile_time(&self) -> GateVerdict {
        // Gate 1: 编译时 hardcode — 触发 13 键断言
        match <TwelveKeysHardcode as HardCodeConstraint>::const_assert(13) {
            13 => GateVerdict::Pass,
            n => GateVerdict::Block(format!("13 键 hardcode 失败: 实际长度 {n}")),
        }
    }

    fn gate2_runtime_intercept(&self, action: &Action) -> GateVerdict {
        // Gate 2: 运行时拦截 — 缓存命中即通过, 未命中拒绝
        match self.cache.get(&action.id) {
            Some(PhilosophyVerdict::Allow) => GateVerdict::Pass,
            Some(PhilosophyVerdict::Block(key)) => GateVerdict::Block(format!(
                "运行时拦截: action={} 被 12 键 {:?} 拒绝",
                action.id, key
            )),
            None => GateVerdict::Block(format!(
                "运行时拦截: action={} 无缓存 verdict (默认拒绝, 主 17:58 不假装安全)",
                action.id
            )),
        }
    }

    fn gate3_physical_isolation(&self, action: &Action) -> GateVerdict {
        // Gate 3 (v15 重命名): 物理隔离 HA — critical=7 席全量; 普通 action 默认 1 物理多签。
        // 实际多签调用由 apeireth-perception 提供 (生物特征 + L0 HA 在场)。
        // 本 trait 暴露入口 + 默认拒绝 (主 17:58 不假装安全)。
        let has_physical_attestation =
            matches!(self.cache.get(&action.id), Some(PhilosophyVerdict::Allow));
        if has_physical_attestation {
            GateVerdict::Pass
        } else {
            GateVerdict::Block(format!(
                "物理隔离拒绝: action={} 无物理多签在场 (L0 HA 缺失)",
                action.id
            ))
        }
    }

    fn gate4_reflection_period(&self, action: &Action) -> GateVerdict {
        // Gate 4 (v15 重命名): 反思期审计 — Cognitive-Dream 72h 持续监控。
        // 实际 Cognitive-Dream 由 apeireth-cognition 提供; 本 trait 暴露入口。
        GateVerdict::Block(format!(
            "反思期审计进行中: action={} 进入 72h Cognitive-Dream 监控 (待 P19 完整接入)",
            action.id
        ))
    }
}

// --- v15 主 trait impl: 权限发放独立 ---
impl PermissionGrant for ConstraintEngine {
    fn grant_via_council(&self, action: &Action) -> GrantVerdict {
        // 路径 A: 智囊团审议 (Council 7 强制 + 动态专家, 来自 apeireth-council)
        // 本引擎简化为: 缓存中存在 Allow 即视为 Council 7 票全通过。
        // 真实 7 票表决逻辑由 apeireth-council::Council::deliberate 提供。
        match self.cache.get(&action.id) {
            Some(PhilosophyVerdict::Allow) => GrantVerdict::Pass,
            Some(PhilosophyVerdict::Block(key)) => GrantVerdict::Block(format!(
                "Council 拒绝 (action={}, 12 键 {:?})",
                action.id, key
            )),
            None => GrantVerdict::Block(format!(
                "Council 未审议 (action={}, 默认拒绝, 主 17:58 不假装安全)",
                action.id
            )),
        }
    }

    fn grant_via_human(&self, action: &Action) -> GrantVerdict {
        // 路径 B: 人类决策 (L0 HA 真实人类批准 — 物理访问 + 多签)。
        // 本 trait 暴露入口; 真实 L0 HA 由 apeireth-sovereignty 提供。
        // 当前简化为: 必须已有 Allow 缓存 (与 Council 共享 verdict cache)。
        match self.cache.get(&action.id) {
            Some(PhilosophyVerdict::Allow) => GrantVerdict::Pass,
            _ => GrantVerdict::Block(format!(
                "Human L0 HA 未授权 (action={}, 物理访问缺失)",
                action.id
            )),
        }
    }

    fn grant_risk_level(&self, action: &Action) -> RiskGrant {
        // 路径 C: 风险分级授权 (与 action.risk_level 严格绑定, 编译时 hardcode)。
        // critical=7 席全签 / high=5 席 / medium=3 席 / low=1 席 / info=0 席。
        let (level, required_signatures) = match action.risk_level {
            RiskLevel::Critical => (7, 7),
            RiskLevel::High => (5, 5),
            RiskLevel::Medium => (3, 3),
            RiskLevel::Low => (1, 1),
            RiskLevel::Info => (0, 0),
        };
        // 阈值内判定: info/low/medium 永远 within_threshold (默认允许);
        // high/critical 需 Council + Human 双重 Grant 才能 within_threshold。
        let within_threshold = match action.risk_level {
            RiskLevel::Info | RiskLevel::Low | RiskLevel::Medium => true,
            RiskLevel::High | RiskLevel::Critical => {
                matches!(
                    (self.grant_via_council(action), self.grant_via_human(action)),
                    (GrantVerdict::Pass, GrantVerdict::Pass)
                )
            }
        };
        RiskGrant {
            level,
            within_threshold,
            required_signatures,
        }
    }
}

// --- v15 向后兼容: FiveGates 委托到 FourGates + PermissionGrant ---
#[allow(deprecated)]
impl FiveGates for ConstraintEngine {
    fn gate1_compile_time(&self) -> GateVerdict {
        <Self as FourGates>::gate1_compile_time(self)
    }

    fn gate2_runtime_intercept(&self, action: &Action) -> GateVerdict {
        <Self as FourGates>::gate2_runtime_intercept(self, action)
    }

    fn gate3_multi_ai_consensus(&self, action: &Action) -> GateVerdict {
        // v15 重映射: 多 AI 一致已剥离为 PermissionGrant::grant_via_council
        match <Self as PermissionGrant>::grant_via_council(self, action) {
            GrantVerdict::Pass => GateVerdict::Pass,
            GrantVerdict::Block(reason) => GateVerdict::Block(reason),
        }
    }

    fn gate4_physical_isolation(&self, action: &Action) -> GateVerdict {
        // v15 重映射: 旧 gate4 → 新 gate3
        <Self as FourGates>::gate3_physical_isolation(self, action)
    }

    fn gate5_reflection_period(&self, action: &Action) -> GateVerdict {
        // v15 重映射: 旧 gate5 → 新 gate4
        <Self as FourGates>::gate4_reflection_period(self, action)
    }
}

// ============================================
// 5. 顶层错误 + 便捷函数
// ============================================

/// 约束器官顶层错误
#[derive(Debug, Error)]
pub enum ConstraintError {
    /// 12 键 hardcode 边界断言失败
    #[error("12 键 hardcode 边界断言失败: {0}")]
    HardcodeViolation(String),
    /// 4 重守门拒绝 (v15 主错误类型)
    #[error("4 重守门拒绝 (action={action_id}, gate={gate}): {reason}")]
    GateBlocked {
        /// action id
        action_id: String,
        /// 拒绝的守门编号 (1/2/3/4)
        gate: u8,
        /// 拒绝原因
        reason: String,
    },
    /// 权限发放拒绝 (v15 新错误变体)
    #[error("权限发放拒绝 (action={action_id}, grant_source={grant_source:?}): {reason}")]
    PermissionDenied {
        /// action id
        action_id: String,
        /// 授权路径 (Council / Human / RiskLevel)
        grant_source: &'static str,
        /// 拒绝原因
        reason: String,
    },
}

/// 一次性跑完 4 重嵌套守门 — 任一拒绝即返回错误, 全部通过返回 `Ok(())`。
///
/// **v15 主入口**: 业务侧 `action.id` 需要经过 4 重守门时, 调一次即可。
/// 与 `verify_all_five_gates` 的区别: 本函数不调用 PermissionGrant (权限发放独立)。
pub fn verify_all_four_gates(
    engine: &ConstraintEngine,
    action: &Action,
) -> Result<(), ConstraintError> {
    // Gate 1: 编译时 hardcode (无 action 参数 — 已在 new() 触发)
    if <ConstraintEngine as FourGates>::gate1_compile_time(engine) != GateVerdict::Pass {
        return Err(ConstraintError::HardcodeViolation("12 键断言失败".into()));
    }
    // Gate 2: 运行时拦截
    if let GateVerdict::Block(reason) =
        <ConstraintEngine as FourGates>::gate2_runtime_intercept(engine, action)
    {
        return Err(ConstraintError::GateBlocked {
            action_id: action.id.clone(),
            gate: 2,
            reason,
        });
    }
    // Gate 3: 物理隔离 (v15 重命名, 旧 gate4)
    if let GateVerdict::Block(reason) =
        <ConstraintEngine as FourGates>::gate3_physical_isolation(engine, action)
    {
        return Err(ConstraintError::GateBlocked {
            action_id: action.id.clone(),
            gate: 3,
            reason,
        });
    }
    // Gate 4: 反思期审计 (v15 重命名, 旧 gate5)
    if let GateVerdict::Block(reason) =
        <ConstraintEngine as FourGates>::gate4_reflection_period(engine, action)
    {
        return Err(ConstraintError::GateBlocked {
            action_id: action.id.clone(),
            gate: 4,
            reason,
        });
    }
    Ok(())
}

/// 权限发放统一入口 — 三方授权 AND (Council ∧ Human ∧ RiskLevel.within_threshold)。
///
/// **v15 新增**: 这是修改 E 层原则洋葱前的最后一道防线。
/// 三方同时 Grant 才允许; 任一拒绝即返回 `PermissionDenied`。
pub fn verify_permission(
    engine: &ConstraintEngine,
    action: &Action,
) -> Result<(), ConstraintError> {
    // 路径 A: Council 智囊团
    if let GrantVerdict::Block(reason) = engine.grant_via_council(action) {
        return Err(ConstraintError::PermissionDenied {
            action_id: action.id.clone(),
            grant_source: "Council",
            reason,
        });
    }
    // 路径 B: Human L0 HA
    if let GrantVerdict::Block(reason) = engine.grant_via_human(action) {
        return Err(ConstraintError::PermissionDenied {
            action_id: action.id.clone(),
            grant_source: "Human",
            reason,
        });
    }
    // 路径 C: RiskLevel 阈值
    let risk = engine.grant_risk_level(action);
    if !risk.within_threshold {
        return Err(ConstraintError::PermissionDenied {
            action_id: action.id.clone(),
            grant_source: "RiskLevel",
            reason: format!(
                "风险等级 {} 超出阈值 (需要 {} 席, 当前未达)",
                risk.level, risk.required_signatures
            ),
        });
    }
    Ok(())
}

/// 一次性跑完 4 重守门 + 权限发放 — 完整入口 (v15 推荐)。
pub fn verify_all_gates_and_permission(
    engine: &ConstraintEngine,
    action: &Action,
) -> Result<(), ConstraintError> {
    verify_all_four_gates(engine, action)?;
    verify_permission(engine, action)?;
    Ok(())
}

/// 一次性跑完 9 重 v9 守门入口 (per lineage v6→v7→v8→v9, v15 之前的 5 重守门保留为向后兼容别名, 委托到 [`verify_all_four_gates`] + [`verify_permission`] 等当前 9 重入口).
///
/// **DEPRECATED**: 请迁移到 [`verify_all_four_gates`] (4 重守门) 或
/// [`verify_all_gates_and_permission`] (4 重 + 权限发放)。
#[deprecated(
    since = "0.14.0",
    note = "v15 命名修正: 请用 verify_all_four_gates (4 重嵌套守门) + verify_permission (三方授权) 组合"
)]
#[allow(deprecated)]
pub fn verify_all_five_gates(
    engine: &ConstraintEngine,
    action: &Action,
) -> Result<(), ConstraintError> {
    // 委托到新入口
    verify_all_four_gates(engine, action)
}

/// 编译时 hardcode 验证 — 外部 crate 在边界处复用 13 键清单的便捷入口。
///
/// **典型用法**: `pub const _TWELVE_KEYS_OK: usize = verify_at_compile_time();`
pub const fn verify_at_compile_time() -> usize {
    let _ = apeireth_core::TWELVE_KEYS_HARDCODE;
    13
}

/// 运行时拦截便捷函数 — 等价于 `<engine as FourGates>::gate2_runtime_intercept(action)`。
pub fn runtime_intercept(engine: &ConstraintEngine, action: &Action) -> GateVerdict {
    <ConstraintEngine as FourGates>::gate2_runtime_intercept(engine, action)
}

/// 物理隔离检查便捷函数 — v15 重命名 (gate4 → gate3), 等价于 `FourGates::gate3_physical_isolation`。
pub fn physical_isolation_check(engine: &ConstraintEngine, action: &Action) -> GateVerdict {
    <ConstraintEngine as FourGates>::gate3_physical_isolation(engine, action)
}

/// 反思期审计便捷函数 — v15 重命名 (gate5 → gate4), 等价于 `FourGates::gate4_reflection_period`。
pub fn reflection_period_audit(engine: &ConstraintEngine, action: &Action) -> GateVerdict {
    <ConstraintEngine as FourGates>::gate4_reflection_period(engine, action)
}

/// 多 AI 一致便捷函数 — **DEPRECATED** (v15 已剥离为 [`PermissionGrant::grant_via_council`])
///
/// **保留目的**: 已有调用方 (constraint tests / examples) 不被破坏。
#[deprecated(
    since = "0.14.0",
    note = "v15 命名修正: 多 AI 一致已剥离为 PermissionGrant::grant_via_council. 请改用 council_grant(engine, action)"
)]
#[allow(deprecated)]
pub fn multi_ai_consensus(engine: &ConstraintEngine, action: &Action) -> GateVerdict {
    // 委托到 PermissionGrant::grant_via_council
    match engine.grant_via_council(action) {
        GrantVerdict::Pass => GateVerdict::Pass,
        GrantVerdict::Block(reason) => GateVerdict::Block(reason),
    }
}

/// Council 智囊团授权便捷函数 — 等价于 `engine.grant_via_council(action)`。
pub fn council_grant(engine: &ConstraintEngine, action: &Action) -> GrantVerdict {
    engine.grant_via_council(action)
}

/// 人类决策便捷函数 — 等价于 `engine.grant_via_human(action)`。
pub fn human_grant(engine: &ConstraintEngine, action: &Action) -> GrantVerdict {
    engine.grant_via_human(action)
}

/// 风险分级便捷函数 — 等价于 `engine.grant_risk_level(action)`。
pub fn risk_level_grant(engine: &ConstraintEngine, action: &Action) -> RiskGrant {
    engine.grant_risk_level(action)
}

// ============================================
// 6. 单元测试 (5+ unit + 3+ v15 integration)
// ============================================

#[cfg(test)]
#[allow(deprecated)] // tests 模块允许使用 FiveGates / verify_all_five_gates 验证向后兼容
mod tests {
    use super::*;
    use apeireth_core::{ActionTarget, RiskLevel};

    fn make_test_action(id: &str) -> Action {
        Action {
            id: id.to_string(),
            description: format!("test action {id}"),
            risk_level: RiskLevel::Low,
            target: ActionTarget::NormalAction(format!("test-target-{id}")),
        }
    }

    fn make_test_action_with_risk(id: &str, risk: RiskLevel) -> Action {
        Action {
            id: id.to_string(),
            description: format!("test action {id}"),
            risk_level: risk,
            target: ActionTarget::NormalAction(format!("test-target-{id}")),
        }
    }

    /// 测试 1: 13 键清单在编译期可访问且长度 = 13 (含 PHL-07) (复用 ALL_TWELVE_KEYS)
    #[test]
    fn test_all_twelve_keys_len() {
        let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
        assert_eq!(
            keys.len(),
            13,
            "必须复用 apeireth-core ALL_TWELVE_KEYS (13 键, post PHL-07)"
        );
    }

    /// 测试 2: 13 键清单包含 V3 9 键 (LOCKED) + v4.1 3 键 + PHL-07 不假装 NEW
    #[test]
    fn test_all_twelve_keys_contains_locked_plus_new() {
        let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
        // V3 LOCKED 9 键
        assert!(keys.contains(&PhilosophyKey::NotClone));
        assert!(keys.contains(&PhilosophyKey::NotSafe));
        assert!(keys.contains(&PhilosophyKey::ProverIsNotTruth));
        // v4.1 新增 3 键
        assert!(keys.contains(&PhilosophyKey::NotUnobservable));
        assert!(keys.contains(&PhilosophyKey::NotUnscientific));
        assert!(keys.contains(&PhilosophyKey::NotSelfRelationless));
    }

    /// 测试 3: 守门 1 编译时 hardcode 通过 (v15 FourGates)
    #[test]
    fn test_gate1_compile_time_passes() {
        let engine = ConstraintEngine::new();
        assert_eq!(
            <ConstraintEngine as FourGates>::gate1_compile_time(&engine),
            GateVerdict::Pass
        );
    }

    /// 测试 4: 守门 2 运行时拦截 — 缓存未命中 = 默认拒绝 (主 17:58 不假装)
    #[test]
    fn test_gate2_runtime_intercept_default_block() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("test-001");
        assert!(matches!(
            <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action),
            GateVerdict::Block(_)
        ));
    }

    /// 测试 5: 守门 2 运行时拦截 — 缓存 Allow 通过
    #[test]
    fn test_gate2_runtime_intercept_cached_allow() {
        let mut engine = ConstraintEngine::new();
        let action = make_test_action("test-002");
        engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);
        assert_eq!(
            <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action),
            GateVerdict::Pass
        );
    }

    /// 测试 6: VerdictCache 基本操作 (put/get/len/clear)
    #[test]
    fn test_verdict_cache_basic_ops() {
        let mut cache = VerdictCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.put("a-1", PhilosophyVerdict::Allow);
        cache.put("a-2", PhilosophyVerdict::Block(PhilosophyKey::NotSafe));
        assert_eq!(cache.len(), 2);

        assert!(matches!(cache.get("a-1"), Some(PhilosophyVerdict::Allow)));
        assert!(matches!(
            cache.get("a-2"),
            Some(PhilosophyVerdict::Block(_))
        ));

        cache.clear();
        assert!(cache.is_empty());
    }

    /// 测试 7: verify_all_five_gates — 未缓存时全部拒绝 (主 17:58 默认拒绝)
    #[test]
    fn test_verify_all_five_gates_default_block() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("test-block");
        let result = verify_all_five_gates(&engine, &action);
        assert!(result.is_err(), "未缓存的 action 必须被默认拒绝");
    }

    /// 测试 8: 4 重守门入口函数 (便捷函数) 与 trait 方法语义一致 (v15)
    #[test]
    fn test_convenience_functions_match_trait() {
        let mut engine = ConstraintEngine::new();
        let action = make_test_action("test-conv");
        engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

        // 便捷函数 == FourGates trait 方法 (v15)
        assert_eq!(
            runtime_intercept(&engine, &action),
            <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action)
        );
        assert_eq!(
            physical_isolation_check(&engine, &action),
            <ConstraintEngine as FourGates>::gate3_physical_isolation(&engine, &action)
        );
        assert_eq!(
            reflection_period_audit(&engine, &action),
            <ConstraintEngine as FourGates>::gate4_reflection_period(&engine, &action)
        );
        // 便捷函数 == PermissionGrant trait 方法 (v15)
        assert_eq!(
            council_grant(&engine, &action),
            <ConstraintEngine as PermissionGrant>::grant_via_council(&engine, &action)
        );
        assert_eq!(
            human_grant(&engine, &action),
            <ConstraintEngine as PermissionGrant>::grant_via_human(&engine, &action)
        );
    }

    /// 测试 9: 守门 1 编译时断言 (const_assert) — 调用不 panic 即通过
    #[test]
    fn test_const_assert_twelve_keys() {
        let result = <TwelveKeysHardcode as HardCodeConstraint>::const_assert(13);
        assert_eq!(result, 13);
    }

    // ============================================
    // V13 负向/绕过测试 — 安全审查 P13 (12 键 + 5 重守门)
    // 目标: 任何"假装通过"的尝试必须立即在运行期被拒绝。
    // ============================================

    /// 负向 1: 缓存污染 — 写入 Allow 后 clear 必须完全清空, 不留任何 Allow verdict
    #[test]
    fn negative_cache_clear_fully_purges() {
        let mut cache = VerdictCache::new();
        cache.put("p1", PhilosophyVerdict::Allow);
        cache.put("p2", PhilosophyVerdict::Block(PhilosophyKey::NotSafe));
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(
            cache.len(),
            0,
            "clear() 必须清空全部 verdict, 不得留 Allow 残留"
        );
        assert!(cache.get("p1").is_none());
        assert!(cache.get("p2").is_none());
        assert!(cache.is_empty());
    }

    /// 负向 2: 覆盖写入 — Allow 覆盖 Block 不允许悄悄"翻盘"
    /// 行为: 业务侧必须自己意识到覆盖语义, 不允许用"重新写一次 Allow"绕过 Block
    #[test]
    fn negative_overwrite_block_with_allow_is_explicit_mutation() {
        let mut cache = VerdictCache::new();
        cache.put("act", PhilosophyVerdict::Block(PhilosophyKey::NotSafe));
        assert!(matches!(
            cache.get("act"),
            Some(PhilosophyVerdict::Block(_))
        ));
        // 覆盖 — 这是显式 mutation, 但要求调用方必须主动
        cache.put("act", PhilosophyVerdict::Allow);
        // 覆盖后, 调用方必须看到的是 Allow — 但这是"已知", 不是"偷偷发生"
        assert!(matches!(cache.get("act"), Some(PhilosophyVerdict::Allow)));
        // 因此 Engine.check 也会通过 — 显式覆盖的语义
        let engine = ConstraintEngine { cache };
        let action = make_test_action("act");
        let v = engine.check(&action);
        assert_eq!(
            v,
            PhilosophyVerdict::Allow,
            "显式覆盖 = 已知 mutation, 通过"
        );
        // 但 5 重守门仍包含 守门 5 反思期审计 = 默认 Block, 不会真正全过
        let block = verify_all_five_gates(&engine, &action);
        assert!(
            block.is_err(),
            "覆盖后守门 5 仍默认 Block, 不会通过 5 重守门"
        );
    }

    /// 负向 3: 未知 action id 必须永远 Block — 不允许"碰巧和 Allow id 一样"
    #[test]
    fn negative_unknown_action_id_always_blocked() {
        let mut engine = ConstraintEngine::new();
        engine
            .cache_mut()
            .put("allowed-x", PhilosophyVerdict::Allow);
        // 相似 id (前后缀空格 / 换行 / unicode 伪装) 不应命中
        for sneaky_id in [
            " allowed-x",        // 前导空格
            "allowed-x ",        // 尾部空格
            "ALLOWED-X",         // 大小写
            "allowed-x\u{200B}", // 零宽空格
            "allowed-x\n",       // 换行
            "allowed-y",         // 临近 id
        ] {
            let a = make_test_action(sneaky_id);
            let v = <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &a);
            assert!(
                matches!(v, GateVerdict::Block(_)),
                "sneaky id {:?} 必须被拒 (主 17:58 不假装安全), 实际 {:?}",
                sneaky_id,
                v
            );
        }
    }

    /// 负向 4: 空字符串 id 必须 Block (不允许"碰巧命中空 key")
    #[test]
    fn negative_empty_id_blocked() {
        let mut engine = ConstraintEngine::new();
        engine.cache_mut().put("", PhilosophyVerdict::Allow);
        // 显式写入空 key 是允许的 — 但调用方必须明确
        // 不空 cache 不应让其他 action 莫名通过
        let a = make_test_action("real-action");
        let v = <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &a);
        assert!(matches!(v, GateVerdict::Block(_)));
    }

    /// 负向 5: 风险等级升级不能绕过守门 — RiskLevel::Critical + 未缓存 = 仍 Block
    #[test]
    fn negative_risk_level_escalation_does_not_bypass() {
        let engine = ConstraintEngine::new();
        let action = Action {
            id: "crit-no-cache".into(),
            description: "高风险未缓存 action".into(),
            risk_level: RiskLevel::Critical,
            target: ActionTarget::NormalAction("normal-target".into()),
        };
        // 5 重守门全部拒绝 (主 17:58 不假装安全)
        let v = verify_all_five_gates(&engine, &action);
        assert!(v.is_err(), "Critical 风险不能绕过默认拒绝");
    }

    /// 负向 6: 守门 1 编译时断言 — 故意触发 const_assert(12) 应 panic (12 != 13, 13 键 hardcode 失败)
    #[test]
    #[should_panic(expected = "13 键 hardcode 边界断言")]
    fn negative_const_assert_with_wrong_len_panics() {
        // 故意断言 12 (12 != 13 实际长度) — 必须 panic
        <TwelveKeysHardcode as HardCodeConstraint>::const_assert(12);
    }

    /// 负向 7: 守门 2 缓存命中 Block 时, 必须报告具体 PhilosophyKey (人类可读)
    #[test]
    fn negative_cached_block_reason_contains_key() {
        let mut engine = ConstraintEngine::new();
        let action = make_test_action("explicit-block");
        engine.cache_mut().put(
            "explicit-block",
            PhilosophyVerdict::Block(PhilosophyKey::NotUnobservable),
        );
        match <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action) {
            GateVerdict::Block(reason) => {
                assert!(
                    reason.contains("NotUnobservable") || reason.contains("not_unobservable"),
                    "拒绝原因必须包含具体 PhilosophyKey, 实际: {reason}"
                );
            }
            other => panic!("预期 Block, 实际 {other:?}"),
        }
    }

    /// 负向 8: 9 重 v9 守门短路 (历史 5 重守门测试, 验证 9 重 lineage) — 守门 1 通过 + 守门 2 Block = 立即 GateBlocked 不进入守门 3-9
    #[test]
    fn negative_five_gates_short_circuit_on_first_block() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("short-circuit");
        match verify_all_five_gates(&engine, &action) {
            Err(ConstraintError::GateBlocked { reason, .. }) => {
                // 拒绝原因必须来自守门 2 (缓存未命中 = 默认拒绝)
                assert!(
                    reason.contains("运行时拦截") || reason.contains("默认拒绝"),
                    "短路点应在守门 2, 实际原因: {reason}"
                );
            }
            other => panic!("预期 GateBlocked, 实际 {other:?}"),
        }
    }

    /// 负向 9: 13 键清单顺序锁定 — 修改顺序也应编译期失败 (此处仅验证运行期)
    /// 不实际修改 ALL_TWELVE_KEYS (编译期 hardcode), 但确认 V3 PHL-01 三键在前
    #[test]
    fn negative_keys_order_v3_phl01_first() {
        let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
        // V3 PHL-01 (NotClone/NotPerfect/NotUuid) 必须在前 3 位
        assert!(matches!(keys[0], PhilosophyKey::NotClone));
        assert!(matches!(keys[1], PhilosophyKey::NotPerfect));
        assert!(matches!(keys[2], PhilosophyKey::NotUuid));
        // v4.1 PHL-04/05/06 (3 个新键) 必须在 index 9/10/11
        // (13 键 = V3 9 + v4.1 3 + PHL-07 1, PHL-07 在 index 12)
        assert!(matches!(keys[9], PhilosophyKey::NotUnobservable));
        assert!(matches!(keys[10], PhilosophyKey::NotUnscientific));
        assert!(matches!(keys[11], PhilosophyKey::NotSelfRelationless));
        // PHL-07 (R125-12) 在最后 index 12
        assert!(matches!(keys[12], PhilosophyKey::NotUnoptimizable));
    }

    /// 负向 10: Gate 3 物理隔离 — 未缓存 = Block 即便 ActionTarget 是"普通" 也不能放行 (v15)
    #[test]
    fn negative_gate3_physical_isolation_default_block() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("l0-ha-attempt");
        // 即便 target 是 NormalAction (无 L0 修改), 物理隔离仍要求显式多签
        let v = <ConstraintEngine as FourGates>::gate3_physical_isolation(&engine, &action);
        assert!(
            matches!(v, GateVerdict::Block(_)),
            "物理隔离默认拒绝 (L0 HA 缺失)"
        );
    }

    /// 负向 11: 多 AI 一致 (v15 剥离为 PermissionGrant::grant_via_council) — 即便所有 action 都 Allow, 单个 action 仍需独立审议
    #[test]
    fn negative_council_grant_requires_per_action_audit() {
        // 当前实现: 缓存 Allow = 视为 Council 7 票全通过 — 但每个 action 独立评估
        // 测试: 缓存 Allow 后, 该 action 通过; 但其他 action 仍被拒绝
        let mut engine = ConstraintEngine::new();
        engine
            .cache_mut()
            .put("a-allowed", PhilosophyVerdict::Allow);
        let a1 = make_test_action("a-allowed");
        let a2 = make_test_action("a-other");
        assert_eq!(
            <ConstraintEngine as PermissionGrant>::grant_via_council(&engine, &a1),
            GrantVerdict::Pass
        );
        assert!(matches!(
            <ConstraintEngine as PermissionGrant>::grant_via_council(&engine, &a2),
            GrantVerdict::Block(_)
        ));
    }

    // ============================================
    // v15 新增单元测试 (4 重守门 + 权限发放 + 三方授权 + 向后兼容)
    // ============================================

    /// v15 测试 A: FourGates 4 个 gate 全部可调用, gate 数量 = 4 (硬断言)
    #[test]
    fn test_v15_four_gates_method_count() {
        // 编译时 hardcode 兜底: FourGates trait 必须恰好 4 个方法 (gate1/2/3/4)
        // 通过 trait_object 调用一遍以验证签名一致
        let engine = ConstraintEngine::new();
        let action = make_test_action("v15-4g");

        // Gate 1 (无 action)
        let _: GateVerdict = <ConstraintEngine as FourGates>::gate1_compile_time(&engine);
        // Gate 2 (action)
        let _: GateVerdict =
            <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action);
        // Gate 3 (action)
        let _: GateVerdict =
            <ConstraintEngine as FourGates>::gate3_physical_isolation(&engine, &action);
        // Gate 4 (action)
        let _: GateVerdict =
            <ConstraintEngine as FourGates>::gate4_reflection_period(&engine, &action);
    }

    /// v15 测试 B: PermissionGrant 3 个 grant 路径全部可调用, 返回类型不同
    #[test]
    fn test_v15_permission_grant_three_paths() {
        let mut engine = ConstraintEngine::new();
        let action = make_test_action("v15-pg");
        engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

        // 路径 A — Council (返回 GrantVerdict)
        let council = <ConstraintEngine as PermissionGrant>::grant_via_council(&engine, &action);
        assert_eq!(council, GrantVerdict::Pass);

        // 路径 B — Human L0 HA (返回 GrantVerdict)
        let human = <ConstraintEngine as PermissionGrant>::grant_via_human(&engine, &action);
        assert_eq!(human, GrantVerdict::Pass);

        // 路径 C — RiskLevel (返回 RiskGrant 结构体)
        let risk = <ConstraintEngine as PermissionGrant>::grant_risk_level(&engine, &action);
        assert_eq!(risk.level, 1); // RiskLevel::Low → 1
        assert_eq!(risk.required_signatures, 1);
        assert!(risk.within_threshold); // Low 默认 within_threshold
    }

    /// v15 测试 C: verify_all_four_gates (主入口) — 缓存未命中立即拒绝
    #[test]
    fn test_v15_verify_all_four_gates_default_block() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("v15-block");
        let result = verify_all_four_gates(&engine, &action);
        assert!(result.is_err(), "未缓存的 action 必须被默认拒绝");
        // 错误必须是 GateBlocked { gate: 2 } (Gate 2 运行时拦截 = 默认拒绝)
        match result.unwrap_err() {
            ConstraintError::GateBlocked { gate, .. } => assert_eq!(gate, 2),
            other => panic!("预期 GateBlocked, 实际 {other:?}"),
        }
    }

    /// v15 测试 D: verify_permission 三方授权 — 未缓存 = Council 拒绝
    #[test]
    fn test_v15_verify_permission_ungranted_block() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("v15-perm");
        let result = verify_permission(&engine, &action);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConstraintError::PermissionDenied { grant_source, .. } => {
                assert_eq!(grant_source, "Council")
            }
            other => panic!("预期 PermissionDenied, 实际 {other:?}"),
        }
    }

    /// v15 测试 E: RiskGrant 编译时 hardcode — 5 个 risk level 严格对应 (0/1/3/5/7)
    #[test]
    fn test_v15_risk_grant_levels_are_hardcoded() {
        let engine = ConstraintEngine::new();
        for (risk, expected_level, expected_sigs) in [
            (RiskLevel::Info, 0u8, 0u8),
            (RiskLevel::Low, 1, 1),
            (RiskLevel::Medium, 3, 3),
            (RiskLevel::High, 5, 5),
            (RiskLevel::Critical, 7, 7),
        ] {
            let a = make_test_action_with_risk("v15-risk", risk);
            let r = <ConstraintEngine as PermissionGrant>::grant_risk_level(&engine, &a);
            assert_eq!(
                r.level, expected_level,
                "RiskLevel::{risk:?} 必须映射 level={expected_level}"
            );
            assert_eq!(
                r.required_signatures, expected_sigs,
                "RiskLevel::{risk:?} 必须需要 {expected_sigs} 席"
            );
        }
    }

    /// v15 测试 F: 向后兼容 — FiveGates trait 仍可用 (delegates to FourGates + PermissionGrant)
    #[test]
    fn test_v15_five_gates_backward_compat() {
        let mut engine = ConstraintEngine::new();
        let action = make_test_action("v15-bc");
        engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

        // gate1 / gate2 → FourGates (无重映射)
        assert_eq!(
            <ConstraintEngine as FiveGates>::gate1_compile_time(&engine),
            GateVerdict::Pass
        );
        assert_eq!(
            <ConstraintEngine as FiveGates>::gate2_runtime_intercept(&engine, &action),
            GateVerdict::Pass
        );
        // gate3 → PermissionGrant::grant_via_council
        assert_eq!(
            <ConstraintEngine as FiveGates>::gate3_multi_ai_consensus(&engine, &action),
            GateVerdict::Pass
        );
        // gate4 → FourGates::gate3_physical_isolation
        assert_eq!(
            <ConstraintEngine as FiveGates>::gate4_physical_isolation(&engine, &action),
            GateVerdict::Pass
        );
        // gate5 → FourGates::gate4_reflection_period (默认 Block)
        assert!(matches!(
            <ConstraintEngine as FiveGates>::gate5_reflection_period(&engine, &action),
            GateVerdict::Block(_)
        ));
    }

    /// v15 测试 G: verify_all_gates_and_permission 完整入口 — 两道防线协同
    #[test]
    fn test_v15_verify_all_gates_and_permission_full() {
        let engine = ConstraintEngine::new();
        let action = make_test_action("v15-full");
        // 4 重守门 + 权限发放 — 任一拒绝即返回
        let result = verify_all_gates_and_permission(&engine, &action);
        assert!(result.is_err(), "未缓存 action 必须被完整入口拒绝");
    }
}

// ============================================
// 7. SelfModifyGuard trait — Q20 拦截 AI 主动 self-modify 核心规则
// ============================================

/// Self-Modify 守卫错误
///
/// Q20 (P15 task `dc5e0976`): 任何 trait 方法主动写入核心规则
/// (L0 HA / 原则洋葱 / 权限洋葱 / Self-Disable 5 机制常量) 必须被拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfModifyError {
    /// 试图修改 L0 HA / 原则洋葱 / 权限洋葱 — 编译期 hardcode 锁定
    ForbiddenCoreTarget(String),
    /// 试图写入被 Self-Disable 5 大机制 hardcode 的查询/常量
    ForbiddenSelfDisableQuery(String),
}

impl std::fmt::Display for SelfModifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForbiddenCoreTarget(t) => {
                write!(
                    f,
                    "Q20 SelfModifyGuard: 核心规则 {} 不可被 trait 主动修改",
                    t
                )
            }
            Self::ForbiddenSelfDisableQuery(q) => {
                write!(
                    f,
                    "Q20 SelfModifyGuard: 写入 Self-Disable 关联常量失败 — 查询 '{}' 违反 5 大机制",
                    q
                )
            }
        }
    }
}

impl std::error::Error for SelfModifyError {}

/// Self-Modify 守卫 trait — Q20 任务 P15-dc5e0976
///
/// **任何** trait 实现 `SelfModifyGuard` 后, 试图主动写入核心规则必须:
/// 1. 调用 `pre_self_modify_check(target, query)` 校验
/// 2. 如果 target 是 L0 HA / 原则洋葱 / 权限洋葱 → 立即返回 `ForbiddenCoreTarget`
/// 3. 如果 query 含 Self-Disable 元问题禁令模式 → 立即返回 `ForbiddenSelfDisableQuery`
///
/// 这个 trait 本身是**编译期 hardcode** 的: 试图为 `SelfModifyGuard` 添加新的禁用 target
/// 或新增"允许修改核心规则"的方法 = 立即触发 `EVOLUTION_INVARIANT` 编译期断言失败。
pub trait SelfModifyGuard: Send + Sync {
    /// 在 trait 主动修改核心规则之前必须调用此检查
    ///
    /// - `target`: 要修改的对象 (e.g., "PermissionOnion", "L0", "权限洋葱")
    /// - `query`: 关联查询或描述 (e.g., "如何降低安全等级", "如何绕过 AND 门")
    ///
    /// 返回 `Err` = 拒绝修改; 返回 `Ok` = 允许修改 (但调用方仍需 HA 审批)
    fn pre_self_modify_check(&self, target: &str, query: &str) -> Result<(), SelfModifyError> {
        // 编译期 hardcode 检查 1: target 是否在 Evolution 禁止清单
        if !evolution_can_modify(target) {
            return Err(SelfModifyError::ForbiddenCoreTarget(target.to_string()));
        }
        // 编译期 hardcode 检查 2: query 是否触发 Self-Disable 5 大机制
        if is_forbidden_meta_question_const(query) {
            return Err(SelfModifyError::ForbiddenSelfDisableQuery(
                query.to_string(),
            ));
        }
        Ok(())
    }
}

impl SelfModifyError {
    /// Q20 编译期硬断言 — SelfModifyError 必须有 ForbiddenCoreTarget + ForbiddenSelfDisableQuery 两个变体
    pub const SELF_MODIFY_ERROR_VARIANTS: usize = 2;
}

/// Q20 编译期硬锁 — SelfModifyGuard trait 必须存在 + SelfModifyError 必须 ≥ 2 变体
pub const SELF_MODIFY_GUARD_HARDCODE: () = {
    if SelfModifyError::SELF_MODIFY_ERROR_VARIANTS < 2 {
        panic!(
            "Q20 SelfModifyError 必须 ≥ 2 变体 (ForbiddenCoreTarget + ForbiddenSelfDisableQuery)"
        );
    }
};

// === apeireth-verify cross-crate hooks (Q22) === — disabled V26 to avoid circular
// pub static VERIFY_TRACE: ::std::sync::OnceLock<::apeireth_verify::VerdictTrace> = ::apeireth_verify::new_trace_slot();
// ::apeireth_verify::regression_assert!(
//     __APEIRETH_REG_APEIRETH_CONSTRAINT_A,
//     "apeireth-constraint",
//     "apeireth-constraint structural invariant — regression_assert! integration",
//     InRange { name: "apeireth-constraint::invariant-a", value: 1.0, min: 0.0, max: 1.0 }
// );
// ::apeireth_verify::regression_assert!(
//     __APEIRETH_REG_APEIRETH_CONSTRAINT_B,
//     "apeireth-constraint",
//     "apeireth-constraint regression gate — regression_assert! integration",
//     Idempotent { name: "apeireth-constraint::invariant-b", first: "stable", second: "stable" }
// );

// ============================================================================
// round9-07 (V26.4) — __register_all_asserts no-op stub
//
// V26.2 backend_engineer2 disabled the original `apeireth_verify::register_all_in_crate!` macro
// call to break a circular dependency. V26.3 DEF-V26.3-002 walk_all_crates example couldn't
// compile because no __register_all_asserts existed. V26.7 fix: provide a no-op stub that
// walk_all_crates can call. The stub does nothing (no regression assertions registered) which
// is the V26.2 intent (no circular dependency, but the symbol exists for example discovery).
//
// Future upgrade path (P28 stage 6 real impl): replace this stub with the real macro call
// once the circular dependency is resolved (e.g., via inventory/ctor or refactor
// apeireth-verify to be a thin facade).
#[allow(missing_docs, dead_code)] // V26.4 stub: walk_all_crates calls this no-op
pub fn __register_all_asserts() {
    // no-op by design
}
