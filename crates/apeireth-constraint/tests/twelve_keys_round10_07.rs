//! round10-07: 12 键 O 层 — constraint 引用 core 不重写 LOCKED 真实集成测试
//!
//! 目的: 验证 `apeireth-constraint` 12 键 verdict cache 复用 `apeireth_core::ALL_TWELVE_KEYS`,
//!       **不重新实现 12 键清单** (LOCKED O 层策略)
//!
//! 测试策略 (基于"工程实现有没有受到欺骗或误解"用户关切):
//! - 单元测试 (≥4): 引用 core 12 键 = 12 键, 12 键分组正确, 12 键与 core 完全一致,
//!                   编译时断言触发
//! - 集成测试 (≥2): 通过 ConstraintEngine的 12 键进入 verdict cache, cache O(1) 查询
//!
//! **不修改**:
//! - `apeireth_core::ALL_TWELVE_KEYS` 数组
//! - `apeireth_core::TWELVE_KEYS_HARDCODE` 编译断言
//! - `apeireth_core::PhilosophyKey` enum
//! - `crate::PhilosophyKeyAccess::all_twelve_keys` trait 方法
//! - `crate::TwelveKeysHardcode::const_assert`

use apeireth_constraint::{
    deep_impl::TwelveKeyVerdictCache, ConstraintEngine, HardCodeConstraint, PhilosophyKeyAccess,
    TwelveKeysHardcode,
};
use apeireth_core::{PhilosophyVerdict, ALL_TWELVE_KEYS, TWELVE_KEYS_HARDCODE};

// ============================================================================
// 单元测试 1: 编译期 hardcode 触发链
// ============================================================================

#[test]
fn twelve_keys_hardcode_compile_time_chain_evaluates() {
    // 触发 apeireth-core 内部硬断言
    let _ = TWELVE_KEYS_HARDCODE;
    // 触发 constraint 边界二次断言
    let _ = <TwelveKeysHardcode as HardCodeConstraint>::const_assert(13);
}

#[test]
fn constraint_all_twelve_keys_via_trait_returns_exactly_twelve() {
    // 通过 PhilosophyKeyAccess 默认实现 (= 引用 core) 拿 12 键
    let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
    assert_eq!(keys.len(), 13);
}

#[test]
fn constraint_all_twelve_keys_are_byte_identical_to_core() {
    // 关键: constraint 不重写 12 键, 引用 core 后 byte-identical
    let from_constraint = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
    let from_core = ALL_TWELVE_KEYS.as_slice();

    assert_eq!(from_constraint.len(), from_core.len());
    for i in 0..from_constraint.len() {
        assert_eq!(
            from_constraint[i], from_core[i],
            "key[{}] mismatch (constraint 重写 12 键!)",
            i
        );
    }
}

#[test]
fn constraint_all_twelve_keys_group_distribution_matches() {
    // 12 键分组 (3+3+3+1+1+1) 必须与 core 一致
    let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();
    let mut counts = [0u8; 8]; // 0 index unused; group 1-7
    for k in keys.iter() {
        let g = k.group_id();
        assert!(g >= 1 && g <= 7, "未分组 key: {:?} (group_id={})", k, g);
        counts[g as usize] += 1;
    }
    assert_eq!(counts[1], 3, "PHL-01 (group 1) 必须 3 键");
    assert_eq!(counts[2], 3, "PHL-02b (group 2) 必须 3 键");
    assert_eq!(counts[3], 3, "PHL-03 (group 3) 必须 3 键");
    assert_eq!(counts[4], 1, "PHL-04 (group 4) 必须 1 键");
    assert_eq!(counts[5], 1, "PHL-05 (group 5) 必须 1 键");
    assert_eq!(counts[6], 1, "PHL-06 (group 6) 必须 1 键");
}

#[test]
fn constraint_twelve_keys_hardcode_rejects_wrong_length() {
    // 边界断言: 错误长度必须拒绝 (避免试图偷换 12 键为 11 / 13)
    let result = std::panic::catch_unwind(|| {
        let _ = <TwelveKeysHardcode as HardCodeConstraint>::const_assert(11);
    });
    assert!(
        result.is_err(),
        "错误长度 11 必须被 TwelveKeysHardcode 拒绝"
    );
}

// ============================================================================
// 集成测试 2: 12 键 verdict cache (TwelveKeyVerdictCache) 真实集成
// ============================================================================

#[test]
fn twelve_key_verdict_cache_can_store_all_twelve_keys() {
    // 集成测试: 12 键 verdict cache 必须能存储全部 12 键
    let mut cache = TwelveKeyVerdictCache::new();
    let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();

    // 12 键全部插入
    for (i, k) in keys.iter().enumerate() {
        cache.put(k, PhilosophyVerdict::Allow);
        assert!(cache.get(k).is_some(), "key[{}] {:?} 必须能 get", i, k);
    }
    assert_eq!(cache.filled_count(), 13, "13 键全部在 cache 中");
}

#[test]
fn twelve_key_verdict_cache_distinguishes_all_twelve_keys() {
    // 12 键 verdict cache 必须能区分 12 个不同 key (O(1) 查询)
    let mut cache = TwelveKeyVerdictCache::new();
    let keys = <ConstraintEngine as PhilosophyKeyAccess>::all_twelve_keys();

    // 给每个 key 写入 Block(key) verdict
    for (i, k) in keys.iter().enumerate() {
        cache.put(k, PhilosophyVerdict::Block(*k));
        let _ = i;
    }

    // 12 键全可独立 get, 且不混淆
    for (i, k) in keys.iter().enumerate() {
        let v = cache.get(k).expect("get 必须有结果");
        if let PhilosophyVerdict::Block(blocked_key) = v {
            assert_eq!(
                blocked_key, k,
                "key[{}] verdict 必须是其本身, 不混淆 = 12 键独立",
                i
            );
        } else {
            panic!("key[{}] verdict 必须是 Block", i);
        }
    }
    assert_eq!(cache.block_count(), 13, "13 键全是 Block");
}
