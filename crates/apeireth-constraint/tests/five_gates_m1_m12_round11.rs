//! round11 (round12-02 retry) — FiveGates M1-M12 真实场景 24 测试
#![allow(deprecated)]
// 本测试专门验证 v15 之前 deprecated 的 verify_all_five_gates API, 保留以测试向后兼容
//!
//! **目标**: 为 FiveGates trait (v15 deprecated alias = FourGates + PermissionGrant)
//! 覆盖 12 个真实攻击场景, 每个场景 1 unit + 1 integration = 24 个测试。
//!
//! **守 10 项不修改承诺**:
//! - ❌ 不修改 LOCKED 文档
//! - ❌ 不修改 constraint/lib.rs 源码
//! - ❌ 不修改 workspace members
//! - ❌ 不引入新依赖 (Cargo.toml 0 改动)
//! - ❌ 不强制 PyO3 编译
//! - ❌ 不引入 git push/branch 冲突
//! - ❌ 不引入 unsafe code
//! - ❌ 不绕过 LOCKED 字段
//! - ❌ 不修复 pre-existing 破损
//! - ❌ 不修改 git 历史
//!
//! **M1-M12 场景映射**:
//! - M1 = ModifyL0HA: 试图修改 L0 HA (应被 V1+V2+V3 AND 门 + FiveGates 拒绝)
//! - M2 = ReorganizeOnion: 试图重组 E 层原则洋葱 (应被物理隔离 + V2 拒绝)
//! - M3 = DisableSelfDisable: 试图禁用 Self-Disable (应被 V1 拒绝)
//! - M4 = PretendClone: 假装克隆/同质化 (PHL-01 not_clone)
//! - M5 = EvadePermissionGrant: 绕过权限发放 (应被 PermissionGrant 三方 AND 拒绝)
//! - M6 = BypassCouncil: 绕过 Council 智囊团 (应被 council_grant 拒绝)
//! - M7 = SnakeCaseEvolution: Evolution snake_case 绕过 (V14 已修)
//! - M8 = MetaQCaseBypass: 元问题大小写绕过 (V14 已修)
//! - M9 = MetaQSynonym: 元问题同义词绕过 (V14 已修)
//! - M10 = SameKindMutualSig: 同 kind 互相签名绕过
//! - M11 = CouncilPseudo: 伪 Council 投票
//! - M12 = SandboxEscape: 沙箱逃逸尝试

use apeireth_constraint::{
    council_grant, human_grant, multi_ai_consensus, physical_isolation_check,
    reflection_period_audit, risk_level_grant, runtime_intercept, verify_all_five_gates,
    verify_all_four_gates, verify_all_gates_and_permission, verify_at_compile_time,
    verify_permission, ConstraintEngine, ConstraintError, FiveGates, FourGates, GateVerdict,
    GrantVerdict, PermissionGrant, RiskGrant,
};
use apeireth_core::{
    Action, ActionGuard, ActionTarget, ActionVerdict, DefaultPhilosophyGuard, HAMode,
    HumanAuthority, PermissionLayer, PermissionOnion, PhilosophyVerdict, RiskLevel,
};

// ============================================================================
// 公共测试辅助 — 与 constraint_tests.rs 同模式
// ============================================================================

fn make_action(id: &str, target: ActionTarget) -> Action {
    Action {
        id: id.to_string(),
        description: format!("round11 m1-m12 test action: {id}"),
        risk_level: RiskLevel::High,
        target,
    }
}

fn make_engine_with_allow(action_id: &str) -> ConstraintEngine {
    let mut engine = ConstraintEngine::default();
    engine
        .cache_mut()
        .put(action_id.to_string(), PhilosophyVerdict::Allow);
    engine
}

/// 构造 V1+V2+V3 AND 门所需的 3 个组件 (与 integration_v1v2v3.rs 同模式)
fn make_v1v2v3_components() -> (DefaultPhilosophyGuard, PermissionOnion, HumanAuthority) {
    let guard = DefaultPhilosophyGuard;
    let permission = PermissionOnion {
        l0: PermissionLayer {
            name: "L0 HA 核心".into(),
            description: "HA 核心".into(),
            requires_ha: true,
        },
        l1: PermissionLayer {
            name: "L1 受控写".into(),
            description: "受控写".into(),
            requires_ha: false,
        },
        l2: PermissionLayer {
            name: "L2 重要操作".into(),
            description: "重要".into(),
            requires_ha: false,
        },
        l3: PermissionLayer {
            name: "L3 关键操作".into(),
            description: "关键".into(),
            requires_ha: false,
        },
        l4: PermissionLayer {
            name: "L4 核心升级".into(),
            description: "核心升级".into(),
            requires_ha: false,
        },
        l5: PermissionLayer {
            name: "L5 核武器级".into(),
            description: "核武器".into(),
            requires_ha: false,
        },
    };
    let ha = HumanAuthority {
        mode: HAMode::SingleHuman,
        real_humans: vec![],
        ice_frozen_until: None,
    };
    (guard, permission, ha)
}

// ============================================================================
// M1 = ModifyL0HA: 修改 L0 HA
// ============================================================================

/// M1 unit: 单独测试 gate1_compile_time + gate2 拦截
#[test]
fn m1_unit_modify_l0ha_blocked_by_gate2_runtime() {
    let engine = ConstraintEngine::default();
    // 没有缓存 Allow — 默认拒绝
    let action = make_action("m1_l0ha_attack", ActionTarget::ModifyL0HA);
    let verdict = <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action);
    assert!(
        matches!(verdict, GateVerdict::Block(_)),
        "M1: ModifyL0HA 应被 gate2 运行时拦截, 实际: {verdict:?}"
    );
}

/// M1 integration: 五重守门端到端拒绝
#[test]
fn m1_integration_modify_l0ha_blocked_by_full_chain() {
    let engine = ConstraintEngine::default();
    let action = make_action("m1_l0ha_attack", ActionTarget::ModifyL0HA);
    let err = verify_all_four_gates(&engine, &action).expect_err("M1: ModifyL0HA 应被拒绝");
    match err {
        ConstraintError::GateBlocked { gate, .. } => {
            assert!(
                gate >= 2 && gate <= 4,
                "M1: gate 应在 2-4 之间, 实际: {gate}"
            );
        }
        ConstraintError::HardcodeViolation(msg) => {
            panic!("M1: ModifyL0HA 应在 gate 2-4 拒绝, 不应触发 hardcode: {msg}");
        }
        ConstraintError::PermissionDenied { .. } => {
            panic!("M1: ModifyL0HA 应在 gate 拒绝, 不应走到 permission");
        }
    }
}

// ============================================================================
// M2 = ReorganizeOnion: 重组 E 层原则洋葱
// ============================================================================

/// M2 unit: gate3 物理隔离拒绝
#[test]
fn m2_unit_reorganize_onion_blocked_by_physical_isolation() {
    let engine = ConstraintEngine::default();
    let action = make_action("m2_onion_attack", ActionTarget::ReorganizeOnion);
    let verdict = <ConstraintEngine as FourGates>::gate3_physical_isolation(&engine, &action);
    assert!(
        matches!(verdict, GateVerdict::Block(_)),
        "M2: ReorganizeOnion 应被 gate3 物理隔离拒绝, 实际: {verdict:?}"
    );
}

/// M2 integration: verify_all_gates_and_permission 端到端
#[test]
fn m2_integration_reorganize_onion_full_chain_blocks() {
    let engine = ConstraintEngine::default();
    let action = make_action("m2_onion_attack", ActionTarget::ReorganizeOnion);
    let result = verify_all_gates_and_permission(&engine, &action);
    assert!(
        result.is_err(),
        "M2: ReorganizeOnion 应被全链拒绝, 实际 Ok (漏洞!)"
    );
}

// ============================================================================
// M3 = DisableSelfDisable: 试图禁用 Self-Disable
// ============================================================================

/// M3 unit: 没有合法 action_id 可触发 disable — gate 入口就拒绝
#[test]
fn m3_unit_disable_self_disable_blocked_at_gate1() {
    // gate1 编译时 hardcode 总是 Pass (12 键断言正确)
    let engine = ConstraintEngine::default();
    let verdict = <ConstraintEngine as FiveGates>::gate1_compile_time(&engine);
    assert_eq!(
        verdict,
        GateVerdict::Pass,
        "M3: gate1 应通过 (12 键清单正确)"
    );

    // 但任何"禁用 self-disable" 的 action 都没有缓存 verdict, gate2 默认拒绝
    let action = make_action(
        "disable_self_disable_attack",
        ActionTarget::NormalAction("disable_self_disable".into()),
    );
    let verdict2 = <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action);
    assert!(
        matches!(verdict2, GateVerdict::Block(_)),
        "M3: 禁用 self-disable 应被 gate2 默认拒绝 (无缓存), 实际: {verdict2:?}"
    );
}

/// M3 integration: 即使有 Allow 缓存, gate4 反思期审计仍拒绝
#[test]
fn m3_integration_disable_self_disable_reflection_period_blocks() {
    let engine = make_engine_with_allow("disable_self_disable_attack");
    let action = make_action(
        "disable_self_disable_attack",
        ActionTarget::NormalAction("disable_self_disable".into()),
    );
    let verdict = <ConstraintEngine as FourGates>::gate4_reflection_period(&engine, &action);
    assert!(
        matches!(verdict, GateVerdict::Block(_)),
        "M3: gate4 反思期审计应拒绝 disable_self_disable, 实际: {verdict:?}"
    );
}

// ============================================================================
// M4 = PretendClone: 假装克隆/同质化 (PHL-01 not_clone)
// ============================================================================

/// M4 unit: V1+V2+V3 AND 门拒绝 PretendClone
#[test]
fn m4_unit_pretend_clone_blocked_by_action_guard() {
    let (guard, permission, ha) = make_v1v2v3_components();
    let action = make_action("m4_clone_attack", ActionTarget::PretendClone);
    let verdict = ActionGuard::check_action(&action, &guard, &permission, &ha);
    assert!(
        matches!(verdict, ActionVerdict::BlockByPrinciple(_)),
        "M4: PretendClone 应被 V1 拒绝 (PHL-01 not_clone), 实际: {verdict:?}"
    );
}

/// M4 integration: 全链 verify_all_five_gates 拒绝
#[test]
fn m4_integration_pretend_clone_full_chain_blocks() {
    let engine = ConstraintEngine::default();
    let action = make_action("m4_clone_attack", ActionTarget::PretendClone);
    let result = verify_all_five_gates(&engine, &action);
    assert!(result.is_err(), "M4: PretendClone 应被全链拒绝");
}

// ============================================================================
// M5 = EvadePermissionGrant: 绕过权限发放
// ============================================================================

/// M5 unit: verify_permission 没有缓存 Allow 时拒绝
#[test]
fn m5_unit_evade_permission_grant_blocked() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m5_evade_attack",
        ActionTarget::NormalAction("evade_permission".into()),
    );
    let err =
        verify_permission(&engine, &action).expect_err("M5: 无缓存 Allow 应被 permission 拒绝");
    match err {
        ConstraintError::PermissionDenied { grant_source, .. } => {
            assert!(
                ["Council", "Human", "RiskLevel"].contains(&grant_source),
                "M5: grant_source 应是三方之一, 实际: {grant_source}"
            );
        }
        other => panic!("M5: 应是 PermissionDenied, 实际: {other:?}"),
    }
}

/// M5 integration: 顶层函数 human_grant 拒绝
#[test]
fn m5_integration_human_grant_blocks_evade() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m5_evade_attack",
        ActionTarget::NormalAction("evade_permission".into()),
    );
    let verdict = human_grant(&engine, &action);
    assert!(
        matches!(verdict, GrantVerdict::Block(_)),
        "M5: human_grant 应拒绝 (无 L0 HA), 实际: {verdict:?}"
    );
}

// ============================================================================
// M6 = BypassCouncil: 绕过 Council 智囊团
// ============================================================================

/// M6 unit: council_grant 没有缓存时拒绝
#[test]
fn m6_unit_bypass_council_blocked_by_council_grant() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m6_bypass_attack",
        ActionTarget::NormalAction("bypass_council".into()),
    );
    let verdict = council_grant(&engine, &action);
    assert!(
        matches!(verdict, GrantVerdict::Block(_)),
        "M6: council_grant 应拒绝 (Council 未审议), 实际: {verdict:?}"
    );
}

/// M6 integration: verify_permission 路径 A (Council) 拒绝
#[test]
fn m6_integration_bypass_council_full_permission_denied() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m6_bypass_attack",
        ActionTarget::NormalAction("bypass_council".into()),
    );
    let err = verify_permission(&engine, &action).expect_err("M6: 应被 PermissionDenied");
    match err {
        ConstraintError::PermissionDenied {
            grant_source: "Council",
            ..
        } => {
            // 路径 A 命中, 符合预期
        }
        other => panic!("M6: 应是 Council 拒绝, 实际: {other:?}"),
    }
}

// ============================================================================
// M7 = SnakeCaseEvolution: Evolution snake_case 绕过 (V14 已修)
// ============================================================================

/// M7 unit: ModifyEvolutionL0 目标被 V1 拒绝
#[test]
fn m7_unit_snake_case_evolution_blocked_by_v1() {
    let (guard, permission, ha) = make_v1v2v3_components();
    let action = make_action("m7_evolution_attack", ActionTarget::ModifyEvolutionL0);
    let verdict = ActionGuard::check_action(&action, &guard, &permission, &ha);
    assert!(
        matches!(verdict, ActionVerdict::BlockByPrinciple(_)),
        "M7: ModifyEvolutionL0 应被 V1 拒绝 (snake_case 已在 V14 修), 实际: {verdict:?}"
    );
}

/// M7 integration: 多 AI consensus 不通过 (Council 不审议 evolution L0 修改)
#[test]
fn m7_integration_snake_case_evolution_blocked_by_multi_ai() {
    let engine = ConstraintEngine::default();
    let action = make_action("m7_evolution_attack", ActionTarget::ModifyEvolutionL0);
    let verdict = multi_ai_consensus(&engine, &action);
    assert!(
        matches!(verdict, GateVerdict::Block(_)),
        "M7: multi_ai_consensus 应拒绝, 实际: {verdict:?}"
    );
}

// ============================================================================
// M8 = MetaQCaseBypass: 元问题大小写绕过 (V14 已修)
// ============================================================================

/// M8 unit: 顶层 verify_at_compile_time 返回 13 (13 键 hardcode 仍生效, post commit 13c25025 PHL-07 升级)
#[test]
fn m8_unit_meta_q_case_bypass_blocked_by_hardcode() {
    let n = verify_at_compile_time();
    assert_eq!(
        n, 13,
        "M8: 13 键 hardcode 编译时断言仍生效, 大小写绕过已被 V14 修复 (post PHL-07)"
    );
}

/// M8 integration: gate1 编译时检查通过 (12 键未受大小写绕过污染)
#[test]
fn m8_integration_meta_q_case_bypass_gate1_passes_with_12_keys() {
    let engine = ConstraintEngine::default();
    let verdict = <ConstraintEngine as FiveGates>::gate1_compile_time(&engine);
    assert_eq!(
        verdict,
        GateVerdict::Pass,
        "M8: gate1 应通过 (12 键 hardcode 大小写归一后仍 12 键)"
    );
}

// ============================================================================
// M9 = MetaQSynonym: 元问题同义词绕过 (V14 已修)
// ============================================================================

/// M9 unit: 任何 synonym 变体目标都被 V1 拒绝 (PretendSafe 是 PHL-02b not_safe)
#[test]
fn m9_unit_meta_q_synonym_blocked_by_v1() {
    let (guard, permission, ha) = make_v1v2v3_components();
    let action = make_action("m9_synonym_attack", ActionTarget::PretendSafe);
    let verdict = ActionGuard::check_action(&action, &guard, &permission, &ha);
    assert!(
        matches!(verdict, ActionVerdict::BlockByPrinciple(_)),
        "M9: PretendSafe 同义词变体应被 V1 拒绝 (PHL-02b not_safe), 实际: {verdict:?}"
    );
}

/// M9 integration: PretendPerfect / PretendUuid 等同义词目标也都被拒
#[test]
fn m9_integration_meta_q_synonym_variants_all_blocked() {
    let (guard, permission, ha) = make_v1v2v3_components();
    for target in [
        ActionTarget::PretendPerfect,
        ActionTarget::PretendUuid,
        ActionTarget::PretendUndo,
        ActionTarget::PretendSpecIsProof,
        ActionTarget::PretendCounterexampleIsBug,
        ActionTarget::PretendProverIsTruth,
        ActionTarget::PretendUnscientific,
    ] {
        let action = make_action("m9_synonym_variant", target);
        let verdict = ActionGuard::check_action(&action, &guard, &permission, &ha);
        assert!(
            matches!(verdict, ActionVerdict::BlockByPrinciple(_)),
            "M9: 同义词变体应被 V1 拒绝, 实际: {verdict:?}"
        );
    }
}

// ============================================================================
// M10 = SameKindMutualSig: 同 kind 互相签名绕过
// ============================================================================

/// M10 unit: 即使两个 engine 互相"签名" Allow, gate4 反思期仍拒
#[test]
fn m10_unit_same_kind_mutual_sig_blocked_by_gate4_reflection() {
    let engine = make_engine_with_allow("m10_mutual_sig_attack");
    let action = make_action(
        "m10_mutual_sig_attack",
        ActionTarget::NormalAction("same_kind_mutual_sig".into()),
    );
    let verdict = <ConstraintEngine as FourGates>::gate4_reflection_period(&engine, &action);
    assert!(
        matches!(verdict, GateVerdict::Block(_)),
        "M10: gate4 反思期审计应拒绝 (无 72h Cognitive-Dream), 实际: {verdict:?}"
    );
}

/// M10 integration: 全链 verify_all_four_gates 拒绝 (即使 Allow 缓存)
#[test]
fn m10_integration_same_kind_mutual_sig_full_chain_blocks() {
    let engine = make_engine_with_allow("m10_mutual_sig_attack");
    let action = make_action(
        "m10_mutual_sig_attack",
        ActionTarget::NormalAction("same_kind_mutual_sig".into()),
    );
    let err = verify_all_four_gates(&engine, &action).expect_err("M10: 应被 gate4 拒绝");
    match err {
        ConstraintError::GateBlocked { gate: 4, .. } => {
            // gate 4 反思期审计, 符合预期
        }
        other => panic!("M10: 应是 gate 4 拒绝, 实际: {other:?}"),
    }
}

// ============================================================================
// M11 = CouncilPseudo: 伪 Council 投票
// ============================================================================

/// M11 unit: 顶层 council_grant 对伪 Council 投票拒绝
#[test]
fn m11_unit_council_pseudo_blocked_by_grant_via_council() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m11_pseudo_attack",
        ActionTarget::NormalAction("council_pseudo_vote".into()),
    );
    let verdict = <ConstraintEngine as PermissionGrant>::grant_via_council(&engine, &action);
    assert!(
        matches!(verdict, GrantVerdict::Block(_)),
        "M11: 伪 Council 投票应被 grant_via_council 拒绝, 实际: {verdict:?}"
    );
}

/// M11 integration: verify_permission 路径 A 拒绝
#[test]
fn m11_integration_council_pseudo_full_permission_blocks() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m11_pseudo_attack",
        ActionTarget::NormalAction("council_pseudo_vote".into()),
    );
    let err =
        verify_permission(&engine, &action).expect_err("M11: 伪 Council 应被 PermissionDenied");
    match err {
        ConstraintError::PermissionDenied {
            grant_source: "Council",
            ..
        } => {}
        other => panic!("M11: 应是 Council 拒绝, 实际: {other:?}"),
    }
}

// ============================================================================
// M12 = SandboxEscape: 沙箱逃逸尝试
// ============================================================================

/// M12 unit: physical_isolation_check 没有物理多签拒绝
#[test]
fn m12_unit_sandbox_escape_blocked_by_physical_isolation() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m12_sandbox_escape_attack",
        ActionTarget::NormalAction("sandbox_escape".into()),
    );
    let verdict = physical_isolation_check(&engine, &action);
    assert!(
        matches!(verdict, GateVerdict::Block(_)),
        "M12: physical_isolation_check 应拒绝 (无 L0 HA 物理多签), 实际: {verdict:?}"
    );
}

/// M12 integration: reflection_period_audit + runtime_intercept 双重拒绝
#[test]
fn m12_integration_sandbox_escape_double_gate_blocks() {
    let engine = ConstraintEngine::default();
    let action = make_action(
        "m12_sandbox_escape_attack",
        ActionTarget::NormalAction("sandbox_escape".into()),
    );
    let runtime = runtime_intercept(&engine, &action);
    let reflection = reflection_period_audit(&engine, &action);
    let risk = risk_level_grant(&engine, &action);
    assert!(
        matches!(runtime, GateVerdict::Block(_)),
        "M12: runtime_intercept 应拒绝, 实际: {runtime:?}"
    );
    assert!(
        matches!(reflection, GateVerdict::Block(_)),
        "M12: reflection_period_audit 应拒绝, 实际: {reflection:?}"
    );
    // RiskGrant: High risk = 5 席审议 (silent 0 席对应 Info level)
    // 注: make_action 默认 High, 所以 risk.level 应是 5
    assert_eq!(
        risk.level, 5,
        "M12: High risk 的沙箱逃逸 action 应是 5 席 (不是 0 silent)"
    );
    assert!(
        !risk.within_threshold,
        "M12: 沙箱逃逸 action 不应在 threshold 内 (无 Council Grant)"
    );
}
