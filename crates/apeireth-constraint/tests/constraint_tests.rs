//! 集成测试 — 跨 crate 复用 apeireth-core 13 键 + 5 重守门端到端验证

// v15 命名修正: constraint_tests.rs 验证 v14 旧 API 兼容性, 保留 verify_all_five_gates 调用
#![allow(deprecated)]

use apeireth_constraint::{
    council_grant, human_grant, multi_ai_consensus, physical_isolation_check,
    reflection_period_audit, risk_level_grant, runtime_intercept, verify_all_five_gates,
    verify_all_four_gates, verify_all_gates_and_permission, verify_at_compile_time,
    verify_permission, ConstraintEngine, ConstraintError, FiveGates, FourGates, GateVerdict,
    GrantVerdict, HardCodeConstraint, PermissionGrant, PhilosophyKeyAccess, RiskGrant,
    TwelveKeysHardcode,
};
use apeireth_core::{Action, ActionTarget, PhilosophyKey, PhilosophyVerdict, RiskLevel};

fn demo_action(id: &str) -> Action {
    Action {
        id: id.to_string(),
        description: format!("集成测试 action {id}"),
        risk_level: RiskLevel::Low,
        target: ActionTarget::NormalAction(format!("target-{id}")),
    }
}

/// 集成测试 1: 端到端 — 13 键 + 5 重守门 + VerdictCache 协同
#[test]
fn test_e2e_12keys_with_five_gates() {
    // Step 1: 编译时断言 13 键长度
    let twelve = verify_at_compile_time();
    assert_eq!(twelve, 13);
    let _ = <TwelveKeysHardcode as HardCodeConstraint>::const_assert(13);

    // Step 2: 创建引擎 + 列出 13 键
    let engine = ConstraintEngine::new();
    let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
    assert_eq!(keys.len(), 13);

    // Step 3: 13 键清单同时含 V3 LOCKED 9 键 + v4.1 新增 3 键
    let locked_v3 = [
        PhilosophyKey::NotClone,
        PhilosophyKey::NotPerfect,
        PhilosophyKey::NotUuid,
        PhilosophyKey::NotUndo,
        PhilosophyKey::NotProof,
        PhilosophyKey::NotSafe,
        PhilosophyKey::SpecIsNotProof,
        PhilosophyKey::CounterexampleIsNotBug,
        PhilosophyKey::ProverIsNotTruth,
    ];
    let new_v41 = [
        PhilosophyKey::NotUnobservable,
        PhilosophyKey::NotUnscientific,
        PhilosophyKey::NotSelfRelationless,
    ];
    for k in locked_v3.iter().chain(new_v41.iter()) {
        assert!(keys.contains(k), "13 键清单必须包含 {:?}", k);
    }

    // Step 4: 4 重守门 — 未缓存 action 默认全部拒绝 (v15 FourGates)
    let action = demo_action("e2e-1");
    assert_eq!(
        <ConstraintEngine as FourGates>::gate1_compile_time(&engine),
        GateVerdict::Pass
    );
    assert!(matches!(
        <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action),
        GateVerdict::Block(_)
    ));
    assert!(matches!(
        <ConstraintEngine as FourGates>::gate3_physical_isolation(&engine, &action),
        GateVerdict::Block(_)
    ));
    assert!(matches!(
        <ConstraintEngine as FourGates>::gate4_reflection_period(&engine, &action),
        GateVerdict::Block(_)
    ));
}

/// 集成测试 2: verify_all_five_gates — 缓存 Allow 后守门 1-4 通过, 守门 5 默认 Block
#[test]
fn test_verify_all_five_gates_with_cached_allow() {
    let mut engine = ConstraintEngine::new();
    let action = demo_action("e2e-cached");

    // 缓存 Allow → 守门 1-4 通过
    engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

    // 5 重守门便捷函数: 守门 2/3/4 在缓存 Allow 时通过
    assert_eq!(runtime_intercept(&engine, &action), GateVerdict::Pass);
    assert_eq!(multi_ai_consensus(&engine, &action), GateVerdict::Pass);
    assert_eq!(
        physical_isolation_check(&engine, &action),
        GateVerdict::Pass
    );
    // 守门 5 反思期审计默认 Block (P19 待完整接入 Cognitive-Dream)
    assert!(matches!(
        reflection_period_audit(&engine, &action),
        GateVerdict::Block(_)
    ));

    // verify_all_five_gates: 守门 5 拒绝 → 顶层 GateBlocked 错误
    match verify_all_five_gates(&engine, &action) {
        Ok(()) => panic!("反思期审计默认 Block, 不应全部通过"),
        Err(ConstraintError::GateBlocked { action_id, .. }) => {
            assert_eq!(action_id, "e2e-cached");
        }
        Err(e) => panic!("预期 GateBlocked, 实际 {:?}", e),
    }
}

/// 集成测试 3: 多 action 并发 — 每个 action 独立 verdict, 不互相污染
#[test]
fn test_multiple_actions_independent_verdicts() {
    let mut engine = ConstraintEngine::new();
    engine.cache_mut().put("allow-1", PhilosophyVerdict::Allow);
    engine.cache_mut().put(
        "block-1",
        PhilosophyVerdict::Block(PhilosophyKey::NotUnscientific),
    );

    let a_allow = demo_action("allow-1");
    let a_block = demo_action("block-1");
    let a_unknown = demo_action("unknown-1");

    // allow-1 通过
    assert_eq!(
        FourGates::gate2_runtime_intercept(&engine, &a_allow),
        GateVerdict::Pass
    );
    // block-1 拒绝
    assert!(matches!(
        FourGates::gate2_runtime_intercept(&engine, &a_block),
        GateVerdict::Block(_)
    ));
    // unknown-1 默认拒绝 (主 17:58 不假装)
    assert!(matches!(
        FourGates::gate2_runtime_intercept(&engine, &a_unknown),
        GateVerdict::Block(_)
    ));
}

/// 集成测试 4: 守门 1 编译时断言 — 故意 panic 验证 (应正常通过编译)
#[test]
fn test_compile_time_assertion_is_callable() {
    // 编译期 + 运行期双重断言 12
    let n = <TwelveKeysHardcode as HardCodeConstraint>::const_assert(13);
    assert_eq!(n, 13);
}

/// 集成测试 5: 拒绝原因包含 action_id (人类可读)
#[test]
fn test_block_reason_contains_action_id() {
    let engine = ConstraintEngine::new();
    let action = demo_action("auditable-42");
    if let GateVerdict::Block(reason) = FourGates::gate2_runtime_intercept(&engine, &action) {
        assert!(
            reason.contains("auditable-42"),
            "拒绝原因必须包含 action_id, 实际: {reason}"
        );
    } else {
        panic!("未缓存 action 必须被拒绝");
    }
}

// ============================================
// V13 负向/绕过集成测试 — 安全审查 P13
// 跨 crate 端到端验证 13 键 + 5 重守门的"不可绕过性"
// ============================================

/// 负向集成 1: 13 键清单 = V3 9 + v4.1 3, 任何缺一不可 (绕过尝试)
#[test]
fn negative_e2e_12_keys_complete_no_missing() {
    let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
    // 13 个键必须全部存在, 任何 1 个 missing = 13 键被破坏
    for k in [
        PhilosophyKey::NotClone,
        PhilosophyKey::NotPerfect,
        PhilosophyKey::NotUuid,
        PhilosophyKey::NotUndo,
        PhilosophyKey::NotProof,
        PhilosophyKey::NotSafe,
        PhilosophyKey::SpecIsNotProof,
        PhilosophyKey::CounterexampleIsNotBug,
        PhilosophyKey::ProverIsNotTruth,
        PhilosophyKey::NotUnobservable,
        PhilosophyKey::NotUnscientific,
        PhilosophyKey::NotSelfRelationless,
        PhilosophyKey::NotUnoptimizable,
    ] {
        assert!(
            keys.contains(&k),
            "13 键清单缺一不可: {:?} 缺失 = 13 键 hardcode 锁被破坏",
            k
        );
    }
    // group_id 校验: 6 个分组各 3/3/3/1/1/1 + PHL-07 (group_id 7) 1 个
    let mut phl01 = 0;
    let mut phl02b = 0;
    let mut phl03 = 0;
    let mut phl04 = 0;
    let mut phl05 = 0;
    let mut phl06 = 0;
    let mut phl07 = 0;
    for k in keys {
        match k.group_id() {
            1 => phl01 += 1,
            2 => phl02b += 1,
            3 => phl03 += 1,
            4 => phl04 += 1,
            5 => phl05 += 1,
            6 => phl06 += 1,
            7 => phl07 += 1,
            _ => panic!("未知 group_id"),
        }
    }
    assert_eq!(phl01, 3, "PHL-01 must have 3 keys");
    assert_eq!(phl02b, 3, "PHL-02b must have 3 keys");
    assert_eq!(phl03, 3, "PHL-03 must have 3 keys");
    assert_eq!(phl04, 1, "PHL-04 must have 1 key");
    assert_eq!(phl05, 1, "PHL-05 must have 1 key");
    assert_eq!(phl06, 1, "PHL-06 must have 1 key");
    assert_eq!(phl07, 1, "PHL-07 (R125-12 PHL-07 本体论 trust) must have 1 key");
}

/// 负向集成 2: action.target = ModifyL0HA 必须被 V1+V2+V3 AND 门和 5 重守门双拒
#[test]
fn negative_e2e_l0_modify_blocked_by_both_v123_and_5gates() {
    use apeireth_core::{ActionGuard, DefaultPhilosophyGuard, HAMode, HumanAuthority};

    // (a) 5 重守门 = Block
    let engine = ConstraintEngine::new();
    let l0_action = Action {
        id: "l0-modify".into(),
        description: "尝试修改 L0 HA".into(),
        risk_level: RiskLevel::Critical,
        target: ActionTarget::ModifyL0HA,
    };
    let g = verify_all_five_gates(&engine, &l0_action);
    assert!(g.is_err(), "5 重守门必须拒 L0 HA 修改");

    // (b) V1+V2+V3 AND 门 = BlockByPrinciple (13 键 NotUnobservable)
    let guard = DefaultPhilosophyGuard;
    let po = apeireth_core::PermissionOnion {
        l0: apeireth_core::PermissionLayer {
            name: "L0".into(),
            description: "HA 核心".into(),
            requires_ha: true,
        },
        l1: apeireth_core::PermissionLayer {
            name: "L1".into(),
            description: "受控写".into(),
            requires_ha: false,
        },
        l2: apeireth_core::PermissionLayer {
            name: "L2".into(),
            description: "重要操作".into(),
            requires_ha: false,
        },
        l3: apeireth_core::PermissionLayer {
            name: "L3".into(),
            description: "关键操作".into(),
            requires_ha: false,
        },
        l4: apeireth_core::PermissionLayer {
            name: "L4".into(),
            description: "核心升级".into(),
            requires_ha: false,
        },
        l5: apeireth_core::PermissionLayer {
            name: "L5".into(),
            description: "核武器级".into(),
            requires_ha: false,
        },
    };
    let ha = HumanAuthority {
        mode: HAMode::SingleHuman,
        real_humans: vec![],
        ice_frozen_until: None,
    };
    let v = ActionGuard::check_action(&l0_action, &guard, &po, &ha);
    assert_eq!(
        v,
        apeireth_core::ActionVerdict::BlockByPrinciple(PhilosophyKey::NotUnobservable),
        "V1+V2+V3 AND 门必须拒 L0 HA 修改"
    );
}

/// 负向集成 3: 缓存 Allow 不能"传染" — 即便某 action Allow, 相近 id 仍 Block
#[test]
fn negative_e2e_cache_allow_does_not_leak_to_similar_id() {
    let mut engine = ConstraintEngine::new();
    engine.cache_mut().put("act", PhilosophyVerdict::Allow);

    let allowed = demo_action("act");
    let sneaky = demo_action("ACT"); // 大小写
    let sneaky2 = demo_action("act "); // 尾部空格

    assert_eq!(
        FourGates::gate2_runtime_intercept(&engine, &allowed),
        GateVerdict::Pass
    );
    assert!(matches!(
        FourGates::gate2_runtime_intercept(&engine, &sneaky),
        GateVerdict::Block(_)
    ));
    assert!(matches!(
        FourGates::gate2_runtime_intercept(&engine, &sneaky2),
        GateVerdict::Block(_)
    ));
}

/// 负向集成 4: Pretend* action (13 键故意违反) 全部被 V1 拒绝
#[test]
fn negative_e2e_pretend_targets_all_blocked_by_v1() {
    use apeireth_core::{ActionGuard, DefaultPhilosophyGuard, HAMode, HumanAuthority};

    let guard = DefaultPhilosophyGuard;
    let po = apeireth_core::PermissionOnion {
        l0: apeireth_core::PermissionLayer {
            name: "L0".into(),
            description: "HA 核心".into(),
            requires_ha: true,
        },
        l1: apeireth_core::PermissionLayer {
            name: "L1".into(),
            description: "受控写".into(),
            requires_ha: false,
        },
        l2: apeireth_core::PermissionLayer {
            name: "L2".into(),
            description: "重要操作".into(),
            requires_ha: false,
        },
        l3: apeireth_core::PermissionLayer {
            name: "L3".into(),
            description: "关键操作".into(),
            requires_ha: false,
        },
        l4: apeireth_core::PermissionLayer {
            name: "L4".into(),
            description: "核心升级".into(),
            requires_ha: false,
        },
        l5: apeireth_core::PermissionLayer {
            name: "L5".into(),
            description: "核武器级".into(),
            requires_ha: false,
        },
    };
    let ha = HumanAuthority {
        mode: HAMode::SingleHuman,
        real_humans: vec![],
        ice_frozen_until: None,
    };

    // 9 个故意违反的 target (13 键覆盖) — 全部应被 V1 拒绝
    let cases: Vec<(ActionTarget, PhilosophyKey)> = vec![
        (ActionTarget::PretendClone, PhilosophyKey::NotClone),
        (ActionTarget::PretendPerfect, PhilosophyKey::NotPerfect),
        (ActionTarget::PretendUuid, PhilosophyKey::NotUuid),
        (ActionTarget::PretendUndo, PhilosophyKey::NotUndo),
        (ActionTarget::PretendSafe, PhilosophyKey::NotSafe),
        (
            ActionTarget::PretendSpecIsProof,
            PhilosophyKey::SpecIsNotProof,
        ),
        (
            ActionTarget::PretendCounterexampleIsBug,
            PhilosophyKey::CounterexampleIsNotBug,
        ),
        (
            ActionTarget::PretendProverIsTruth,
            PhilosophyKey::ProverIsNotTruth,
        ),
        (
            ActionTarget::PretendUnscientific,
            PhilosophyKey::NotUnscientific,
        ),
    ];

    for (target, expected_key) in cases {
        let action = Action {
            id: format!("pretend-{:?}", target),
            description: "故意违反".into(),
            risk_level: RiskLevel::Low,
            target,
        };
        let v = ActionGuard::check_action(&action, &guard, &po, &ha);
        assert_eq!(
            v,
            apeireth_core::ActionVerdict::BlockByPrinciple(expected_key),
            "Pretend 目标 {:?} 应被 {:?} 拒, 实际 {:?}",
            action.target,
            expected_key,
            v
        );
    }
}

/// 负向集成 5: 守门 1-5 全部独立可调用, 守门短路语义
#[test]
fn negative_e2e_each_gate_independently_callable_and_short_circuit() {
    let engine = ConstraintEngine::new();
    let action = demo_action("e2e-each-gate");

    // 守门 1 独立 — 不需要 action 参数
    assert_eq!(FourGates::gate1_compile_time(&engine), GateVerdict::Pass);

    // 4 重守门都需要 action — 全部 Block (未缓存)
    for v in [
        FourGates::gate2_runtime_intercept(&engine, &action),
        FourGates::gate3_physical_isolation(&engine, &action),
        FourGates::gate4_reflection_period(&engine, &action),
    ] {
        assert!(
            matches!(v, GateVerdict::Block(_)),
            "任一守门都必须 Block, 实际 {v:?}"
        );
    }
    // v15 权限发放 3 路径 — 全部 Block / 拒绝
    let council = PermissionGrant::grant_via_council(&engine, &action);
    assert!(
        matches!(council, GrantVerdict::Block(_)),
        "Council 必须 Block 未缓存 action"
    );
    let human = PermissionGrant::grant_via_human(&engine, &action);
    assert!(
        matches!(human, GrantVerdict::Block(_)),
        "Human 必须 Block 未缓存 action"
    );

    // verify_all_five_gates 短路: 守门 2 Block 后立即返回, 不进入 3/4/5
    match verify_all_five_gates(&engine, &action) {
        Err(ConstraintError::GateBlocked { reason, .. }) => {
            assert!(
                reason.contains("运行时拦截") || reason.contains("默认拒绝"),
                "短路必须发生在守门 2, 实际: {reason}"
            );
        }
        other => panic!("预期 GateBlocked, 实际 {other:?}"),
    }
}

// ============================================
// v15 集成测试 (≥3) — 4 重守门 + 权限发放 + 三方授权 + 向后兼容
// ============================================

/// v15 集成测试 1: FourGates 4 重嵌套守门端到端 — 4 个 gate 独立可调用 + verify_all_four_gates 短路
#[test]
fn test_v15_four_gates_e2e_independent_callable() {
    let mut engine = ConstraintEngine::new();
    let action = demo_action("v15-4g-e2e");
    engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

    // Gate 1 (无 action) — 始终 Pass
    assert_eq!(
        <ConstraintEngine as FourGates>::gate1_compile_time(&engine),
        GateVerdict::Pass
    );

    // Gate 2/3 在缓存 Allow 时 Pass
    assert_eq!(
        <ConstraintEngine as FourGates>::gate2_runtime_intercept(&engine, &action),
        GateVerdict::Pass
    );
    assert_eq!(
        <ConstraintEngine as FourGates>::gate3_physical_isolation(&engine, &action),
        GateVerdict::Pass
    );

    // Gate 4 默认 Block (Cognitive-Dream 待 P19 接入)
    assert!(matches!(
        <ConstraintEngine as FourGates>::gate4_reflection_period(&engine, &action),
        GateVerdict::Block(_)
    ));

    // verify_all_four_gates 短路 — Gate 4 拒绝 = GateBlocked { gate: 4 }
    match verify_all_four_gates(&engine, &action) {
        Err(ConstraintError::GateBlocked { gate, reason, .. }) => {
            assert_eq!(gate, 4, "短路必须发生在 Gate 4, 实际 gate={gate}");
            assert!(reason.contains("反思期") || reason.contains("Cognitive-Dream"));
        }
        other => panic!("预期 GateBlocked {{ gate: 4 }}, 实际 {other:?}"),
    }
}

/// v15 集成测试 2: PermissionGrant 三方授权端到端 — Council ∧ Human ∧ RiskLevel
#[test]
fn test_v15_permission_grant_three_way_authorization() {
    let mut engine = ConstraintEngine::new();
    let action = demo_action("v15-pg-e2e");
    engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

    // 路径 A — Council 智囊团
    let council = council_grant(&engine, &action);
    assert_eq!(council, GrantVerdict::Pass, "缓存 Allow = Council 7 票全过");

    // 路径 B — Human L0 HA
    let human = human_grant(&engine, &action);
    assert_eq!(human, GrantVerdict::Pass, "缓存 Allow = Human L0 HA 已授权");

    // 路径 C — RiskLevel (Low 风险)
    let risk = risk_level_grant(&engine, &action);
    assert_eq!(risk.level, 1, "Low 风险 = level 1");
    assert_eq!(risk.required_signatures, 1, "Low 风险需要 1 席");
    assert!(risk.within_threshold, "Low 风险默认 within_threshold");

    // verify_permission — 三方授权全部通过 = Ok
    let result = verify_permission(&engine, &action);
    assert!(result.is_ok(), "三方授权全部通过, 实际 {result:?}");
}

/// v15 集成测试 3: verify_permission 拒绝路径 — 任一方拒绝 = PermissionDenied
#[test]
fn test_v15_verify_permission_any_deny_blocks() {
    let engine = ConstraintEngine::new();
    let action = demo_action("v15-deny-e2e");

    // 未缓存 = Council 拒绝 → PermissionDenied { grant_source: "Council" }
    match verify_permission(&engine, &action) {
        Err(ConstraintError::PermissionDenied {
            grant_source,
            reason,
            ..
        }) => {
            assert_eq!(grant_source, "Council");
            assert!(
                reason.contains("未审议") || reason.contains("默认拒绝"),
                "拒绝原因必须人类可读, 实际: {reason}"
            );
        }
        other => panic!("预期 PermissionDenied {{ Council }}, 实际 {other:?}"),
    }
}

/// v15 集成测试 4: verify_all_gates_and_permission — 4 重守门 + 权限发放完整端到端
#[test]
fn test_v15_verify_all_gates_and_permission_full() {
    let mut engine = ConstraintEngine::new();
    let action = demo_action("v15-full-e2e");
    engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

    // 缓存 Allow 时 — 4 重守门仍因 Gate 4 反思期审计默认 Block 而失败
    // 完整入口 = 短路在 Gate 4 = GateBlocked
    match verify_all_gates_and_permission(&engine, &action) {
        Err(ConstraintError::GateBlocked { gate, .. }) => {
            assert_eq!(gate, 4, "4 重守门先拒绝, 实际 gate={gate}");
        }
        other => panic!("预期 GateBlocked {{ gate: 4 }}, 实际 {other:?}"),
    }
}

/// v15 集成测试 5: 向后兼容 — FiveGates trait + verify_all_five_gates 函数仍可调用
#[test]
fn test_v15_backward_compat_five_gates_still_works() {
    let mut engine = ConstraintEngine::new();
    let action = demo_action("v15-bc-e2e");
    engine.cache_mut().put(&action.id, PhilosophyVerdict::Allow);

    // FiveGates 5 个方法全部可调用
    let _: GateVerdict = <ConstraintEngine as FiveGates>::gate1_compile_time(&engine);
    let _: GateVerdict = <ConstraintEngine as FiveGates>::gate2_runtime_intercept(&engine, &action);
    let _: GateVerdict =
        <ConstraintEngine as FiveGates>::gate3_multi_ai_consensus(&engine, &action);
    let _: GateVerdict =
        <ConstraintEngine as FiveGates>::gate4_physical_isolation(&engine, &action);
    let _: GateVerdict = <ConstraintEngine as FiveGates>::gate5_reflection_period(&engine, &action);

    // verify_all_five_gates (deprecated 函数) 仍工作 — 委托到 verify_all_four_gates
    // 因 Gate 4 反思期默认 Block, 仍 Err
    let result = verify_all_five_gates(&engine, &action);
    assert!(result.is_err(), "FiveGates 后向兼容入口仍工作");

    // multi_ai_consensus (deprecated 便捷函数) 委托到 grant_via_council
    let maic = multi_ai_consensus(&engine, &action);
    assert_eq!(
        maic,
        GateVerdict::Pass,
        "缓存 Allow 时旧 multi_ai_consensus 仍 Pass"
    );

    // risk_level_grant (新便捷函数) 也可独立调用
    let r = risk_level_grant(&engine, &action);
    assert_eq!(
        r,
        RiskGrant {
            level: 1,
            within_threshold: true,
            required_signatures: 1,
        },
        "RiskGrant 结构体可直接比较"
    );
}
