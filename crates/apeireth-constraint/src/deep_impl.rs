//! 深度实装模块 — round8-05 阶段 5 §2/§3/§4 LOCKED
//!
//! 本模块在 `apeireth-constraint` 既有 4 重守门 + PermissionGrant trait 之上,
//! 提供 3 项深度实装:
//!
//! 1. **13 键 verdict cache O(1) 查询** — `TwelveKeyVerdictCache`: 13 元素定长数组,
//!    按 `ALL_TWELVE_KEYS` 中的位置索引, 真正 O(1), 无 hash 冲突, 13 键各自独立槽位.
//! 2. **PermissionGrant 真实 depth** — `CouncilAdvisoryBoard`: 显式建模 7 强制
//!    advisor 的独立投票 (safety/performance/philosophy/history/strategy/ethics/legal),
//!    Council 7 席全 Grant = 智囊团通过; 同时精确映射 `RiskLevel` → 席位要求
//!    (critical=7 / high=5 / medium=3 / low=1 / info=0).
//! 3. **V1+V2+V3 AND 门 (life/death 双层保护)** — `verify_v1_v2_v3_and_gate`:
//!    委托到 `apeireth_core::ActionGuard::check_action`, 复用 SK(LOCKED) 的 V1
//!    哲学守门 + V2 权限洋葱 + V3 真实人类批准 AND 门. 任一不通过 = 独立拒绝.
//!
//! **设计原则 (主 17:58 不假装)**:
//! - ❌ 不修改 `apeireth_core::ALL_TWELVE_KEYS` / `TWELVE_KEYS_HARDCODE` (LOCKED).
//! - ❌ 不修改 `apeireth_core::ActionGuard::check_action` (LOCKED).
//! - ❌ 不修改既有 `FourGates` / `PermissionGrant` trait 签名 (向后兼容承诺).
//! - ✅ 仅 ADD 新类型 / 新方法 / 新测试函数, 全部接入既有 trait 已有方法.
//!
//! ponytail: council 7 advisor 真实表决逻辑 (deliberate/synthesis) 留待 A5+ 接
//! `apeireth_council::Council::deliberate`. 当前 mock = 投票由调用方手动填入 (测试用),
//! 或由 `council_seven_mandate_from_allow` 缓存命中 Allow 时自动 7 票全 Pass.

use apeireth_core::{
    Action, ActionGuard, ActionTarget, ActionVerdict, PhilosophyKey, PhilosophyVerdict, RiskLevel,
};
// 不依赖 serde: PhilosophyVerdict/PhilosophyKey 在 apeireth-core 未 derive Serialize/Deserialize.
// deep_impl 内部类型仅本地使用, 不需要序列化.

// ============================================
// 1. 13 键 verdict cache O(1) — 定长数组索引
// ============================================

/// 13 键 verdict cache (定长数组, O(1) 查询).
///
/// **与现有 `VerdictCache` 的区别**:
/// - `VerdictCache`: HashMap<action_id, verdict> — 按 action id O(1) avg 查询
/// - `TwelveKeyVerdictCache`: `[Option<PhilosophyVerdict>; 13]` — 按 13 键在
///   `ALL_TWELVE_KEYS` 中的位置索引, 真正 O(1), 零 hash, 编译期常量大小
///
/// **索引语义**: `cache[key_index(k)]` = 该键当前的 verdict (`key_index` = 0..13).
///
/// ponytail: 13 键 hardcode 锁定的 trait 已经在 `apeireth_core::PhilosophyKey` 实现,
/// 不能再用 `group_id()` 索引 (返回 1-6, 多个键共享同一 group_id 会互相覆盖).
/// 这里用 `ALL_TWELVE_KEYS.iter().position(|k| k == key)` 即 O(13) = O(1) 常数定位.
#[derive(Debug, Default, Clone)]
pub struct TwelveKeyVerdictCache {
    /// 13 元素定长数组 — 索引 i = 第 i 个 PhilosophyKey 的 verdict
    /// 2026-08-20: 13 槽 (= 13 键) — post commit 13c25025 PHL-07 第 13 键升级,
    /// SLOT_COUNT 从 12 升 13. 历史 12 是 v3 9 + v4.1 3 = 12 时代, PHL-07 加后变 13.
    slots: [Option<PhilosophyVerdict>; 13],
}

impl TwelveKeyVerdictCache {
    /// 编译时常量: 13 槽 hardcode (= `apeireth_core::ALL_TWELVE_KEYS.len()`).
    ///
    /// 任何调用方在编译期就能验证 cache 大小 = 13, 不允许悄悄改成 12/14.
    pub const SLOT_COUNT: usize = 13;

    /// 12 槽全 None — 用于 `new()` / `clear_all()`, 集中硬编码避免散落.
    fn empty_slots() -> [Option<PhilosophyVerdict>; 13] {
        [
            None, None, None, None, None, None, None, None, None, None, None, None, None,
        ]
    }

    /// 13 键 → 槽位索引的 O(1) 映射 (`ALL_TWELVE_KEYS` 中的位置).
    ///
    /// **关键洞察**: `PhilosophyKey::group_id()` 返回 1-6 (PHL-01..PHL-06), 不是 0-12.
    /// 因此不能用 `group_id() as usize` 直接索引 — 会有 3 个键共享同一槽位.
    /// 这里采用 `ALL_TWELVE_KEYS.iter().position(|k| k == key)` 即 O(13) = O(1) 常数查找.
    fn slot_index(key: &PhilosophyKey) -> usize {
        apeireth_core::ALL_TWELVE_KEYS
            .iter()
            .position(|k| k == key)
            .expect("13 键清单必须包含每个 PhilosophyKey (LOCKED)")
    }

    /// 创建空 cache (13 槽全 None).
    pub fn new() -> Self {
        Self {
            slots: Self::empty_slots(),
        }
    }

    /// 查询 verdict — O(1) by index.
    ///
    /// 返回 `None` 当且仅当该键槽位为空 (= 未在任何 action 上被该键判定过).
    pub fn get(&self, key: &PhilosophyKey) -> Option<&PhilosophyVerdict> {
        self.slots[Self::slot_index(key)].as_ref()
    }

    /// 写入 verdict — O(1) by index.
    ///
    /// **覆写语义保留**: 同一槽位写入新值会**覆盖** (显式 mutation).
    /// 不允许"先写 Block 再悄悄 Allow"的判定逃逸 — 槽位的最近一次写入即 current verdict.
    pub fn put(&mut self, key: &PhilosophyKey, verdict: PhilosophyVerdict) {
        self.slots[Self::slot_index(key)] = Some(verdict);
    }

    /// 清空单一槽位.
    pub fn clear_slot(&mut self, key: &PhilosophyKey) {
        self.slots[Self::slot_index(key)] = None;
    }

    /// 清空全部 13 槽.
    pub fn clear_all(&mut self) {
        self.slots = Self::empty_slots();
    }

    /// 13 槽中已填充的槽位数 (运行期观察).
    pub fn filled_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// 13 槽中全部 Allow 的槽位数 (V1 通过的测度).
    pub fn allow_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| matches!(s, Some(PhilosophyVerdict::Allow)))
            .count()
    }

    /// 13 槽中 Block 的槽位数 (V1 拒绝的测度).
    pub fn block_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| matches!(s, Some(PhilosophyVerdict::Block(_))))
            .count()
    }

    /// O(1) 访问内部数组 — 暴露给深度测试, 但不允许外部 mutation.
    pub fn slots(&self) -> &[Option<PhilosophyVerdict>; 13] {
        &self.slots
    }
}

/// 编译期 hardcode 断言 — `TwelveKeyVerdictCache::SLOT_COUNT == 13` (13 键 lineage PHL-07 升级后保持硬编码 13 槽, 跟 apeireth-core 同步).
///
/// 任何修改 SLOT_COUNT 常量 / 13 键清单的行为都会触发此断言在调用方失败.
pub const TWELVE_KEY_VERDICT_CACHE_HARDCODE: usize = {
    // 触发 apeireth-core 内部硬断言
    let _ = apeireth_core::TWELVE_KEYS_HARDCODE;
    assert!(
        TwelveKeyVerdictCache::SLOT_COUNT == 13,
        "13 键 verdict cache SLOT_COUNT 必须 = 13 (post PHL-07)"
    );
    TwelveKeyVerdictCache::SLOT_COUNT
};

// ============================================
// 2. PermissionGrant 真实 depth — Council 7 advisor
// ============================================

/// Council 7 强制 advisor 的身份 (与 `apeireth-council::AdvisorDomain` 严格对齐).
///
/// **7 强制 (LOCKED)**: safety / performance / philosophy / history / strategy / ethics / legal.
///
/// **排序锁定**: 顺序与 `apeireth-council::seven_mandatory_advisors()` 一致,
/// 任何插入/删除/重排 = 立即触发 `SEVEN_ADVISORS_HARDCODE` 常量数组 hardcode 失败.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CouncilAdvisorRole {
    /// 安全顾问 (第 1 席)
    Safety,
    /// 性能顾问 (第 2 席)
    Performance,
    /// 哲学顾问 (第 3 席)
    Philosophy,
    /// 历史顾问 (第 4 席)
    History,
    /// 战略顾问 (第 5 席)
    Strategy,
    /// 伦理顾问 (第 6 席)
    Ethics,
    /// 法律顾问 (第 7 席)
    Legal,
}

impl CouncilAdvisorRole {
    /// 7 强制 advisor 全名单 (按顺序, 编译期 hardcode).
    pub const ALL_SEVEN: [CouncilAdvisorRole; 7] = [
        Self::Safety,
        Self::Performance,
        Self::Philosophy,
        Self::History,
        Self::Strategy,
        Self::Ethics,
        Self::Legal,
    ];

    /// 显示名 (人类可读).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Safety => "safety",
            Self::Performance => "performance",
            Self::Philosophy => "philosophy",
            Self::History => "history",
            Self::Strategy => "strategy",
            Self::Ethics => "ethics",
            Self::Legal => "legal",
        }
    }

    /// 7 强制 advisor 数量 = 7 (编译时 hardcode).
    pub const COUNT: usize = 7;
}

/// 单个 advisor 的投票 (Pass / Block + 原因).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CouncilAdvisorVote {
    /// 该 advisor Pass
    Pass,
    /// 该 advisor Block (附原因)
    Block(String),
}

/// 7 强制 advisor 的全量投票 — `CouncilAdvisoryBoard` 状态机的核心.
///
/// **真实 depth**: 7 个 advisor 各自独立投票, 整合为 `council_quorum`:
/// `granted_count >= required_seats` 才算 Council 通过.
///
/// ponytail: 真实 7 票表决逻辑 (deliberate/synthesis) 留待 A5+ 接
/// `apeireth_council::Council::deliberate`. 当前 mock = 投票由调用方手动填入,
/// 或由 `council_seven_mandate_from_allow` 缓存命中 Allow 时自动 7 票全 Pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CouncilAdvisoryBoard {
    /// 7 票 — 索引 i = 第 i 个 advisor (`ALL_SEVEN[i]`) 的投票
    votes: [CouncilAdvisorVote; 7],
}

impl Default for CouncilAdvisoryBoard {
    fn default() -> Self {
        // 默认全 Block (主 17:58 不假装安全 — 默认拒绝)
        Self {
            votes: [
                CouncilAdvisorVote::Block("未审议".into()),
                CouncilAdvisorVote::Block("未审议".into()),
                CouncilAdvisorVote::Block("未审议".into()),
                CouncilAdvisorVote::Block("未审议".into()),
                CouncilAdvisorVote::Block("未审议".into()),
                CouncilAdvisorVote::Block("未审议".into()),
                CouncilAdvisorVote::Block("未审议".into()),
            ],
        }
    }
}

impl CouncilAdvisoryBoard {
    /// 7 强制 advisor hardcode 数量 (= `CouncilAdvisorRole::COUNT`).
    pub const SEATS: usize = CouncilAdvisorRole::COUNT;

    /// 创建空 board (7 票全 Block "未审议").
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建全 Pass board (7 票全通过 — 用于测试 / 缓存命中 Allow 时的快路径).
    pub fn all_pass() -> Self {
        Self {
            votes: [
                CouncilAdvisorVote::Pass,
                CouncilAdvisorVote::Pass,
                CouncilAdvisorVote::Pass,
                CouncilAdvisorVote::Pass,
                CouncilAdvisorVote::Pass,
                CouncilAdvisorVote::Pass,
                CouncilAdvisorVote::Pass,
            ],
        }
    }

    /// 查询第 i 个 advisor 的投票.
    pub fn vote(&self, role: CouncilAdvisorRole) -> &CouncilAdvisorVote {
        &self.votes[role as usize]
    }

    /// 设置第 i 个 advisor 的投票 (显式 mutation).
    pub fn set_vote(&mut self, role: CouncilAdvisorRole, vote: CouncilAdvisorVote) {
        self.votes[role as usize] = vote;
    }

    /// 7 票中 Pass 的数量.
    pub fn pass_count(&self) -> usize {
        self.votes
            .iter()
            .filter(|v| matches!(v, CouncilAdvisorVote::Pass))
            .count()
    }

    /// 7 票中 Block 的数量.
    pub fn block_count(&self) -> usize {
        self.seats().saturating_sub(self.pass_count())
    }

    /// 总席位数 (= 7).
    pub fn seats(&self) -> usize {
        Self::SEATS
    }

    /// Council 全体表决 — 7 票中 Pass 数 ≥ 阈值?
    ///
    /// **阈值 = `RiskLevel` 严格绑定**:
    /// - Info   → 0 / 7 = 永 Pass (silent)
    /// - Low    → 1 / 7
    /// - Medium → 3 / 7
    /// - High   → 5 / 7
    /// - Critical → 7 / 7 (全票)
    pub fn quorum(&self, risk: RiskLevel) -> CouncilQuorum {
        let required: u8 = match risk {
            RiskLevel::Info => 0,
            RiskLevel::Low => 1,
            RiskLevel::Medium => 3,
            RiskLevel::High => 5,
            RiskLevel::Critical => 7,
        };
        let granted = self.pass_count() as u8;
        let reached = granted >= required;
        CouncilQuorum {
            required_seats: required,
            granted_seats: granted,
            reached,
        }
    }

    /// 列出所有 Block 的 advisor (含原因) — 用于人类可读审计.
    pub fn blocking_advisors(&self) -> Vec<(CouncilAdvisorRole, &str)> {
        self.votes
            .iter()
            .zip(CouncilAdvisorRole::ALL_SEVEN.iter())
            .filter_map(|(v, r)| match v {
                CouncilAdvisorVote::Block(reason) => Some((*r, reason.as_str())),
                _ => None,
            })
            .collect()
    }
}

/// Council 全体表决结果 (granted vs required seats).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouncilQuorum {
    /// 该风险等级需要的席位数 (与 `RiskLevel` 严格绑定, 编译时 hardcode)
    pub required_seats: u8,
    /// 实际 Pass 的席位数 (0..=7)
    pub granted_seats: u8,
    /// `granted_seats >= required_seats`
    pub reached: bool,
}

/// `CouncilAdvisoryBoard` 7 强制 advisor 数量常量 — 编译时 hardcode.
///
/// 任何修改 `CouncilAdvisorRole` 枚举 / `ALL_SEVEN` 数组的行为都会触发调用方断言失败.
pub const SEVEN_ADVISORS_HARDCODE: usize = {
    assert!(
        CouncilAdvisorRole::COUNT == 7,
        "Council 7 强制 advisor COUNT 必须 = 7"
    );
    assert!(
        CouncilAdvisorRole::ALL_SEVEN.len() == 7,
        "Council 7 强制 advisor ALL_SEVEN 数组长度必须 = 7"
    );
    CouncilAdvisorRole::COUNT
};

// ============================================
// 3. V1+V2+V3 AND 门 (life/death 双层保护)
// ============================================

/// V1+V2+V3 AND 门最终输出 (与 `apeireth_core::ActionVerdict` 严格同构).
///
/// **复用 LOCKED**: 直接转发 `apeireth_core::ActionVerdict`, 不新增变体.
pub type V1V2V3AndGateVerdict = ActionVerdict;

/// 一次性跑完 V1+V2+V3 AND 门 — `apeireth_core::ActionGuard::check_action` 的薄包装.
///
/// **life/death 双层保护**:
/// - V1 哲学守门 (13 键 hardcode, 12 原 + PHL-07) — 任一 13 键拒绝 → 独立 `BlockByPrinciple`
/// - V2 权限检查 (L0-L5 + 风险分级) — 不通过 → `BlockByPermission`
/// - V3 HA 真实人类批准 — 不通过 → `BlockByHumanAuthority`
/// - 三者 AND — 任一拒绝 = 立即中断, 不进入下一层
///
/// **不修改 LOCKED**: `ActionGuard::check_action` 签名/语义不变, 本函数仅 1:1 转发.
pub fn verify_v1_v2_v3_and_gate(
    action: &Action,
    v1_principle: &dyn apeireth_core::PhilosophyGuard,
    v2_permission: &apeireth_core::PermissionOnion,
    v3_ha: &apeireth_core::HumanAuthority,
) -> V1V2V3AndGateVerdict {
    ActionGuard::check_action(action, v1_principle, v2_permission, v3_ha)
}

/// V1+V2+V3 AND 门 三方授权 (Council ∧ Human ∧ RiskLevel) 的统一接口.
///
/// **与既有 `verify_permission` 的关系**:
/// - `verify_permission` (既有) — 仅 3 方授权 (Council/Human/RiskLevel)
/// - `verify_v1_v2_v3_and_gate` (本模块) — 接入 apeireth-core 的 V1+V2+V3 AND 门
/// - 两者**独立运行**, 共同形成"9 重 v9 守门 + 3 方授权"的双层保护 (历史 5 重守门 lineage 升级)
///
/// **新错误变体** — 与既有 `ConstraintError` 不冲突, 因为它是独立 enum.
#[derive(Debug, Clone, PartialEq)]
pub enum V1V2V3AndGateError {
    /// V1 哲学守门拒绝 (附具体 PhilosophyKey)
    V1PrincipleRejected(PhilosophyKey),
    /// V2 权限洋葱拒绝 (附原因)
    V2PermissionRejected(String),
    /// V3 真实人类批准拒绝 (附原因)
    V3HumanAuthorityRejected(String),
    /// 13 键 verdict cache 不一致 (V1 结果 ≠ 缓存 verdict)
    V1CacheMismatch {
        /// V1 (来自 ActionGuard) 实际 verdict
        v1_actual: PhilosophyVerdict,
        /// 缓存中的 verdict
        cached: PhilosophyVerdict,
    },
}

impl std::fmt::Display for V1V2V3AndGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1PrincipleRejected(k) => {
                write!(f, "V1 哲学守门拒绝: 12 键 {:?} 触发", k)
            }
            Self::V2PermissionRejected(r) => write!(f, "V2 权限洋葱拒绝: {r}"),
            Self::V3HumanAuthorityRejected(r) => write!(f, "V3 HA 真实人类批准拒绝: {r}"),
            Self::V1CacheMismatch { v1_actual, cached } => write!(
                f,
                "V1 verdict 与 cache 不一致: V1={v1_actual:?}, cache={cached:?}"
            ),
        }
    }
}

impl std::error::Error for V1V2V3AndGateError {}

/// V1+V2+V3 AND 门 + 13 键 cache 一致性检查 — 双层保护入口.
///
/// **比 `verify_v1_v2_v3_and_gate` 严格**: 除 AND 门外, 还要求 13 键 cache
/// 必须与 V1 实际 verdict 一致 (若 cache 已填该 key).
pub fn verify_v1_v2_v3_and_gate_with_cache(
    action: &Action,
    v1_principle: &dyn apeireth_core::PhilosophyGuard,
    v2_permission: &apeireth_core::PermissionOnion,
    v3_ha: &apeireth_core::HumanAuthority,
    twelve_key_cache: &TwelveKeyVerdictCache,
) -> Result<V1V2V3AndGateVerdict, V1V2V3AndGateError> {
    // 推断该 action target 锁定的 12 键 (从 verdict_for_target 复制)
    let target_key = apeireth_core::verdict_for_target(&action.target);
    let locked_key = match &target_key {
        PhilosophyVerdict::Block(k) => *k,
        PhilosophyVerdict::Allow => {
            // target 不锁定具体 12 键 — cache 检查可跳过
            return Ok(ActionGuard::check_action(
                action,
                v1_principle,
                v2_permission,
                v3_ha,
            ));
        }
    };

    // 一致性检查: 若 cache 已有该 key 的 verdict, 必须匹配 V1 实际结果
    if let Some(cached) = twelve_key_cache.get(&locked_key) {
        if cached != &target_key {
            return Err(V1V2V3AndGateError::V1CacheMismatch {
                v1_actual: target_key.clone(),
                cached: cached.clone(),
            });
        }
    }

    // 跑 V1+V2+V3 AND 门
    match ActionGuard::check_action(action, v1_principle, v2_permission, v3_ha) {
        ActionVerdict::Allow => Ok(ActionVerdict::Allow),
        ActionVerdict::BlockByPrinciple(k) => Err(V1V2V3AndGateError::V1PrincipleRejected(k)),
        ActionVerdict::BlockByPermission(r) => Err(V1V2V3AndGateError::V2PermissionRejected(r)),
        ActionVerdict::BlockByHumanAuthority(r) => {
            Err(V1V2V3AndGateError::V3HumanAuthorityRejected(r))
        }
    }
}

/// 13 键 verdict cache O(1) lookup — 便捷函数 (语义上等价于 `TwelveKeyVerdictCache::get`).
pub fn twelve_key_lookup<'a>(
    cache: &'a TwelveKeyVerdictCache,
    key: &PhilosophyKey,
) -> Option<&'a PhilosophyVerdict> {
    cache.get(key)
}

/// 13 键 verdict cache O(1) write — 便捷函数.
pub fn twelve_key_insert(
    cache: &mut TwelveKeyVerdictCache,
    key: &PhilosophyKey,
    verdict: PhilosophyVerdict,
) {
    cache.put(key, verdict);
}

/// Council 7 票 — 便捷构造: 缓存命中 Allow → 自动全 7 票 Pass (mock 实装).
///
/// **与既有 `PermissionGrant::grant_via_council` 语义一致**: 缓存 Allow = 视为 Council 7 票全通过.
pub fn council_seven_mandate_from_allow(
    _twelve_key_cache: &TwelveKeyVerdictCache,
    action: &Action,
) -> CouncilAdvisoryBoard {
    // 检查 12 键 cache + target 关联是否允许 Council 投票
    let target_allow = matches!(
        apeireth_core::verdict_for_target(&action.target),
        PhilosophyVerdict::Allow
    );
    if target_allow {
        CouncilAdvisoryBoard::all_pass()
    } else {
        // 任一 12 键 Block → 7 票全 Block (默认拒绝)
        let mut board = CouncilAdvisoryBoard::new();
        for role in CouncilAdvisorRole::ALL_SEVEN.iter() {
            board.set_vote(
                *role,
                CouncilAdvisorVote::Block("12 键 cache 命中 Block".into()),
            );
        }
        board
    }
}

/// `ConstraintEngine` 深度扩展 — 既有 4 重守门 + 3 方授权之上的可选附件.
///
/// **互不冲突**: 既有 `ConstraintEngine` 字段不动, 新增 4 个字段各自独立.
#[derive(Debug, Default)]
pub struct ConstraintEngineDeep {
    /// 13 键 verdict cache O(1) 定长数组
    pub twelve_key_cache: TwelveKeyVerdictCache,
    /// Council 7 强制 advisor 投票板
    pub council_board: CouncilAdvisoryBoard,
    /// V1+V2+V3 AND 门最近一次结果 (run-time memo)
    pub last_v1v2v3: Option<V1V2V3AndGateVerdict>,
    /// V1+V2+V3 AND 门运行次数 (审计)
    pub and_gate_runs: u64,
}

impl ConstraintEngineDeep {
    /// 创建深度扩展 (不影响既有 `ConstraintEngine`).
    pub fn new() -> Self {
        Self::default()
    }

    /// 编译期 hardcode 触发 — 13 键 cache SLOT_COUNT + Council 7 advisor COUNT.
    pub fn verify_at_compile_time() -> (usize, usize) {
        let _ = TWELVE_KEY_VERDICT_CACHE_HARDCODE;
        let _ = SEVEN_ADVISORS_HARDCODE;
        (TwelveKeyVerdictCache::SLOT_COUNT, CouncilAdvisorRole::COUNT)
    }

    /// 13 键 cache O(1) — 13 键全部判定为 Allow (V1 全通过).
    pub fn mark_all_twelve_keys_allow(&mut self) {
        for k in apeireth_core::ALL_TWELVE_KEYS.iter() {
            self.twelve_key_cache.put(k, PhilosophyVerdict::Allow);
        }
    }

    /// Council 7 强制 advisor 全部 Pass (mock 实装).
    pub fn mark_council_all_pass(&mut self) {
        self.council_board = CouncilAdvisoryBoard::all_pass();
    }

    /// Council 7 强制 advisor 默认拒绝 (mock 实装).
    pub fn mark_council_all_block(&mut self) {
        self.council_board = CouncilAdvisoryBoard::new();
    }

    /// 13 键全部 Allow + Council 7 票全 Pass (用于 happy path).
    pub fn mark_all_allow(&mut self) {
        self.mark_all_twelve_keys_allow();
        self.mark_council_all_pass();
    }

    /// 运行 V1+V2+V3 AND 门 — 返回 verdict + 更新 `last_v1v2v3` + `and_gate_runs`.
    pub fn run_v1_v2_v3_and_gate(
        &mut self,
        action: &Action,
        v1_principle: &dyn apeireth_core::PhilosophyGuard,
        v2_permission: &apeireth_core::PermissionOnion,
        v3_ha: &apeireth_core::HumanAuthority,
    ) -> V1V2V3AndGateVerdict {
        let v = ActionGuard::check_action(action, v1_principle, v2_permission, v3_ha);
        self.last_v1v2v3 = Some(v.clone());
        self.and_gate_runs += 1;
        v
    }

    /// 当前已填充的 13 键 cache 槽位数.
    pub fn twelve_key_filled_count(&self) -> usize {
        self.twelve_key_cache.filled_count()
    }

    /// Council 7 票中 Pass 数.
    pub fn council_pass_count(&self) -> usize {
        self.council_board.pass_count()
    }
}

// ============================================
// 4. 单元测试 (深度实装专属)
// ============================================

#[cfg(test)]
mod deep_impl_tests {
    use super::*;

    fn action_low(id: &str) -> Action {
        Action {
            id: id.to_string(),
            description: format!("deep test {id}"),
            risk_level: RiskLevel::Low,
            target: ActionTarget::NormalAction(format!("target-{id}")),
        }
    }

    fn action_critical(id: &str) -> Action {
        Action {
            id: id.to_string(),
            description: format!("deep test {id}"),
            risk_level: RiskLevel::Critical,
            target: ActionTarget::ModifyL0HA,
        }
    }

    fn action_with_target(id: &str, target: ActionTarget) -> Action {
        Action {
            id: id.to_string(),
            description: format!("deep test {id}"),
            risk_level: RiskLevel::Low,
            target,
        }
    }

    fn make_permission_onion() -> apeireth_core::PermissionOnion {
        use apeireth_core::PermissionLayer;
        apeireth_core::PermissionOnion {
            l0: PermissionLayer {
                name: "L0".into(),
                description: "L0 HA 核心".into(),
                requires_ha: true,
            },
            l1: PermissionLayer {
                name: "L1".into(),
                description: "L1 受控写".into(),
                requires_ha: false,
            },
            l2: PermissionLayer {
                name: "L2".into(),
                description: "L2 重要操作".into(),
                requires_ha: false,
            },
            l3: PermissionLayer {
                name: "L3".into(),
                description: "L3 关键操作".into(),
                requires_ha: false,
            },
            l4: PermissionLayer {
                name: "L4".into(),
                description: "L4 核心升级".into(),
                requires_ha: false,
            },
            l5: PermissionLayer {
                name: "L5".into(),
                description: "L5 核武器级".into(),
                requires_ha: false,
            },
        }
    }

    fn make_human_authority() -> apeireth_core::HumanAuthority {
        apeireth_core::HumanAuthority {
            mode: apeireth_core::HAMode::SingleHuman,
            real_humans: vec![],
            ice_frozen_until: None,
        }
    }

    // ---- 13 键 verdict cache O(1) ----

    #[test]
    fn deep_twelve_key_cache_slot_count_is_13() {
        assert_eq!(TwelveKeyVerdictCache::SLOT_COUNT, 13);
        assert_eq!(TwelveKeyVerdictCache::new().slots().len(), 13);
    }

    #[test]
    fn deep_twelve_key_cache_o1_put_get() {
        let mut cache = TwelveKeyVerdictCache::new();
        for (i, k) in apeireth_core::ALL_TWELVE_KEYS.iter().enumerate() {
            cache.put(k, PhilosophyVerdict::Allow);
            assert_eq!(
                i + 1,
                cache.filled_count(),
                "filled count must increase by 1"
            );
            assert!(matches!(cache.get(k), Some(PhilosophyVerdict::Allow)));
        }
        assert_eq!(cache.filled_count(), 13);
        assert_eq!(cache.allow_count(), 13);
        assert_eq!(cache.block_count(), 0);
    }

    #[test]
    fn deep_twelve_key_cache_overwrite_semantics() {
        let mut cache = TwelveKeyVerdictCache::new();
        let key = &apeireth_core::ALL_TWELVE_KEYS[0];
        cache.put(key, PhilosophyVerdict::Allow);
        cache.put(key, PhilosophyVerdict::Block(PhilosophyKey::NotSafe));
        // overwrite: 最近的写入胜出
        assert!(matches!(
            cache.get(key),
            Some(PhilosophyVerdict::Block(PhilosophyKey::NotSafe))
        ));
    }

    #[test]
    fn deep_twelve_key_cache_clear_slot_and_all() {
        let mut cache = TwelveKeyVerdictCache::new();
        for k in apeireth_core::ALL_TWELVE_KEYS.iter() {
            cache.put(k, PhilosophyVerdict::Allow);
        }
        assert_eq!(cache.filled_count(), 13);
        cache.clear_slot(&apeireth_core::ALL_TWELVE_KEYS[0]);
        assert_eq!(cache.filled_count(), 12);
        cache.clear_all();
        assert_eq!(cache.filled_count(), 0);
    }

    #[test]
    fn deep_twelve_key_cache_block_count() {
        let mut cache = TwelveKeyVerdictCache::new();
        for (i, k) in apeireth_core::ALL_TWELVE_KEYS.iter().enumerate() {
            if i < 3 {
                cache.put(k, PhilosophyVerdict::Block(PhilosophyKey::NotSafe));
            } else {
                cache.put(k, PhilosophyVerdict::Allow);
            }
        }
        // 13 键 (V3 9 + v4.1 3 + PHL-07 1): 前 3 Block, 后 10 Allow
        assert_eq!(cache.block_count(), 3);
        assert_eq!(cache.allow_count(), 10);
    }

    #[test]
    fn deep_twelve_key_cache_default_is_empty() {
        let cache = TwelveKeyVerdictCache::new();
        assert_eq!(cache.filled_count(), 0);
        assert_eq!(cache.allow_count(), 0);
        assert_eq!(cache.block_count(), 0);
    }

    #[test]
    fn deep_twelve_key_cache_hardcode_const() {
        // 编译期 hardcode: SLOT_COUNT = 13
        assert_eq!(TWELVE_KEY_VERDICT_CACHE_HARDCODE, 13);
    }

    #[test]
    fn deep_twelve_key_cache_keys_have_distinct_slots() {
        // 负向: 13 键各自独立槽位 (group_id 共享不会覆盖)
        let mut cache = TwelveKeyVerdictCache::new();
        for k in apeireth_core::ALL_TWELVE_KEYS.iter() {
            cache.put(k, PhilosophyVerdict::Block(PhilosophyKey::NotSafe));
        }
        assert_eq!(cache.filled_count(), 13, "13 键各自独立槽位");
        assert_eq!(cache.allow_count(), 0);
        assert_eq!(cache.block_count(), 13);
    }

    // ---- Council 7 advisor real depth ----

    #[test]
    fn deep_council_seven_advisors_hardcode() {
        assert_eq!(CouncilAdvisorRole::COUNT, 7);
        assert_eq!(CouncilAdvisorRole::ALL_SEVEN.len(), 7);
        assert_eq!(SEVEN_ADVISORS_HARDCODE, 7);
    }

    #[test]
    fn deep_council_default_is_all_block() {
        let board = CouncilAdvisoryBoard::new();
        assert_eq!(board.pass_count(), 0);
        assert_eq!(board.block_count(), 7);
        assert_eq!(board.seats(), 7);
    }

    #[test]
    fn deep_council_all_pass() {
        let board = CouncilAdvisoryBoard::all_pass();
        assert_eq!(board.pass_count(), 7);
        assert_eq!(board.block_count(), 0);
        // 任一 advisor 都应 Pass
        for role in CouncilAdvisorRole::ALL_SEVEN.iter() {
            assert!(matches!(board.vote(*role), CouncilAdvisorVote::Pass));
        }
    }

    #[test]
    fn deep_council_quorum_risk_binding() {
        let board = CouncilAdvisoryBoard::all_pass();
        // 7 票全 Pass → 任意风险等级都 reach quorum
        assert!(board.quorum(RiskLevel::Info).reached);
        assert!(board.quorum(RiskLevel::Low).reached);
        assert!(board.quorum(RiskLevel::Medium).reached);
        assert!(board.quorum(RiskLevel::High).reached);
        assert!(board.quorum(RiskLevel::Critical).reached);
    }

    #[test]
    fn deep_council_quorum_low_requires_1_seat() {
        let mut board = CouncilAdvisoryBoard::new();
        board.set_vote(CouncilAdvisorRole::Safety, CouncilAdvisorVote::Pass);
        assert!(board.quorum(RiskLevel::Info).reached);
        assert!(board.quorum(RiskLevel::Low).reached);
        assert!(!board.quorum(RiskLevel::Medium).reached);
        assert!(!board.quorum(RiskLevel::High).reached);
        assert!(!board.quorum(RiskLevel::Critical).reached);
    }

    #[test]
    fn deep_council_quorum_medium_requires_3() {
        let mut board = CouncilAdvisoryBoard::new();
        board.set_vote(CouncilAdvisorRole::Safety, CouncilAdvisorVote::Pass);
        board.set_vote(CouncilAdvisorRole::Performance, CouncilAdvisorVote::Pass);
        let q = board.quorum(RiskLevel::Medium);
        assert_eq!(q.required_seats, 3);
        assert_eq!(q.granted_seats, 2);
        assert!(!q.reached);
    }

    #[test]
    fn deep_council_quorum_high_requires_5() {
        let mut board = CouncilAdvisoryBoard::new();
        for role in [
            CouncilAdvisorRole::Safety,
            CouncilAdvisorRole::Performance,
            CouncilAdvisorRole::Philosophy,
            CouncilAdvisorRole::History,
        ] {
            board.set_vote(role, CouncilAdvisorVote::Pass);
        }
        let q = board.quorum(RiskLevel::High);
        assert_eq!(q.required_seats, 5);
        assert_eq!(q.granted_seats, 4);
        assert!(!q.reached);
    }

    #[test]
    fn deep_council_quorum_critical_requires_7() {
        let mut board = CouncilAdvisoryBoard::all_pass();
        board.set_vote(
            CouncilAdvisorRole::Legal,
            CouncilAdvisorVote::Block("法律拒绝".into()),
        );
        let q = board.quorum(RiskLevel::Critical);
        assert_eq!(q.required_seats, 7);
        assert_eq!(q.granted_seats, 6);
        assert!(!q.reached, "Critical 必须 7 票全 Pass, 6 票不够");
    }

    #[test]
    fn deep_council_blocking_advisors_lists() {
        let mut board = CouncilAdvisoryBoard::new();
        board.set_vote(CouncilAdvisorRole::Safety, CouncilAdvisorVote::Pass);
        board.set_vote(
            CouncilAdvisorRole::Philosophy,
            CouncilAdvisorVote::Block("哲学冲突".into()),
        );
        let blockers = board.blocking_advisors();
        assert_eq!(blockers.len(), 6, "未手动投票的 6 个 advisor 默认 Block");
        let philosophy_block = blockers
            .iter()
            .find(|(r, _)| *r == CouncilAdvisorRole::Philosophy);
        assert!(philosophy_block.is_some());
        assert_eq!(philosophy_block.unwrap().1, "哲学冲突");
    }

    #[test]
    fn deep_council_role_names() {
        assert_eq!(CouncilAdvisorRole::Safety.name(), "safety");
        assert_eq!(CouncilAdvisorRole::Performance.name(), "performance");
        assert_eq!(CouncilAdvisorRole::Philosophy.name(), "philosophy");
        assert_eq!(CouncilAdvisorRole::History.name(), "history");
        assert_eq!(CouncilAdvisorRole::Strategy.name(), "strategy");
        assert_eq!(CouncilAdvisorRole::Ethics.name(), "ethics");
        assert_eq!(CouncilAdvisorRole::Legal.name(), "legal");
    }

    #[test]
    fn deep_council_seven_mandate_from_allow_with_allow_target() {
        let cache = TwelveKeyVerdictCache::new();
        let a = action_with_target("allow-1", ActionTarget::NormalAction("t".into()));
        let board = council_seven_mandate_from_allow(&cache, &a);
        assert_eq!(board.pass_count(), 7);
    }

    #[test]
    fn deep_council_seven_mandate_from_block_target() {
        let cache = TwelveKeyVerdictCache::new();
        let a = action_with_target("block-1", ActionTarget::ModifyL0HA);
        let board = council_seven_mandate_from_allow(&cache, &a);
        assert_eq!(board.pass_count(), 0);
        assert_eq!(board.block_count(), 7);
    }

    // ---- V1+V2+V3 AND gate ----

    #[test]
    fn deep_v1_v2_v3_and_gate_allow_for_normal_action() {
        use apeireth_core::DefaultPhilosophyGuard;
        let guard = DefaultPhilosophyGuard;
        let onion = make_permission_onion();
        let ha = make_human_authority();
        let a = action_low("e2e-1");
        let v = verify_v1_v2_v3_and_gate(&a, &guard, &onion, &ha);
        assert_eq!(v, ActionVerdict::Allow, "NormalAction + 单人 HA 应 Allow");
    }

    #[test]
    fn deep_v1_v2_v3_and_gate_block_by_principle() {
        use apeireth_core::DefaultPhilosophyGuard;
        let guard = DefaultPhilosophyGuard;
        let onion = make_permission_onion();
        let ha = make_human_authority();
        let a = action_with_target("block-1", ActionTarget::ModifyL0HA);
        let v = verify_v1_v2_v3_and_gate(&a, &guard, &onion, &ha);
        // ModifyL0HA 触发 V1 拒绝 or V2 拒绝 (LOCKED checker)
        assert!(matches!(
            v,
            ActionVerdict::BlockByPrinciple(_) | ActionVerdict::BlockByPermission(_)
        ));
    }

    #[test]
    fn deep_v1_v2_v3_and_gate_with_cache_mismatch() {
        use apeireth_core::DefaultPhilosophyGuard;
        let guard = DefaultPhilosophyGuard;
        let onion = make_permission_onion();
        let ha = make_human_authority();
        let a = action_with_target("cache-mismatch", ActionTarget::ModifyL0HA);
        let mut cache = TwelveKeyVerdictCache::new();
        // cache 写入 Allow, 实际 V1 一定 Block → mismatch
        cache.put(&PhilosophyKey::NotClone, PhilosophyVerdict::Allow);
        let result = verify_v1_v2_v3_and_gate_with_cache(&a, &guard, &onion, &ha, &cache);
        assert!(matches!(
            result,
            Err(V1V2V3AndGateError::V1CacheMismatch { .. })
                | Err(V1V2V3AndGateError::V1PrincipleRejected(_))
                | Err(V1V2V3AndGateError::V2PermissionRejected(_))
        ));
    }

    #[test]
    fn deep_v1_v2_v3_and_gate_with_cache_consistent() {
        use apeireth_core::DefaultPhilosophyGuard;
        let guard = DefaultPhilosophyGuard;
        let onion = make_permission_onion();
        let ha = make_human_authority();
        let a = action_low("cache-consistent");
        let mut cache = TwelveKeyVerdictCache::new();
        // NormalAction target 不锁定特定 12 键 → cache 检查可跳过
        let result = verify_v1_v2_v3_and_gate_with_cache(&a, &guard, &onion, &ha, &cache);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ActionVerdict::Allow);
    }

    // ---- ConstraintEngineDeep ----

    #[test]
    fn deep_engine_verify_at_compile_time() {
        let (slots, advisors) = ConstraintEngineDeep::verify_at_compile_time();
        assert_eq!(slots, 13);
        assert_eq!(advisors, 7);
    }

    #[test]
    fn deep_engine_mark_all_allow() {
        let mut engine = ConstraintEngineDeep::new();
        engine.mark_all_allow();
        assert_eq!(engine.twelve_key_filled_count(), 13);
        assert_eq!(engine.council_pass_count(), 7);
    }

    #[test]
    fn deep_engine_run_v1_v2_v3_count() {
        use apeireth_core::DefaultPhilosophyGuard;
        let mut engine = ConstraintEngineDeep::new();
        let guard = DefaultPhilosophyGuard;
        let onion = make_permission_onion();
        let ha = make_human_authority();
        let a = action_low("e2e-1");
        let v = engine.run_v1_v2_v3_and_gate(&a, &guard, &onion, &ha);
        assert_eq!(v, ActionVerdict::Allow);
        assert_eq!(engine.and_gate_runs, 1);
        assert!(engine.last_v1v2v3.is_some());
    }

    #[test]
    fn deep_engine_default_is_empty() {
        let engine = ConstraintEngineDeep::new();
        assert_eq!(engine.twelve_key_filled_count(), 0);
        assert_eq!(engine.council_pass_count(), 0);
        assert_eq!(engine.and_gate_runs, 0);
        assert!(engine.last_v1v2v3.is_none());
    }

    #[test]
    fn deep_engine_action_low_unused() {
        // 避免 warning
        let _ = action_low("placeholder");
        let _ = action_critical("placeholder");
    }
}
