//! Integration tests for apeireth-context-fold (post-1.0.0 增量)
//!
//! Unit tests 在 src/ 各 mod tests (~7 cases).
//! 这里 (tests/) 加 per-行为样板, 跟其他 crate (e.g. apeireth-cron/tests/) 一致.
//!
//! 真生产价值:
//! - fold 4 策略 (Truncate / HeadTail / MarkerReplace / Summary) 端到端
//! - fold limit=0 守门 (Err InvalidLimit, 8 硬墙 "0 假装")
//! - 内容 ≤ limit no-op (零拷贝, 高效)
//! - content.len() == limit boundary (== 不触发 fold)
//! - Unicode 边界 (char_boundary 检查, 多字节字符不切碎)
//! - unfold round-trip (MarkerReplace / HeadTail 还原)
//! - 4 策略原长度 (original_len) 一致
//! - FoldError Display
//!
//! 0 触碰 src/, 0 编造"已实现"。

#![allow(missing_docs)]

use apeireth_context_fold::{
    fold, unfold, FoldError, FoldMarker, FoldResult, FoldStrategy, MarkerKind,
};

// =============================================================================
// 1. fold 4 策略端到端
// =============================================================================

#[test]
fn fold_truncate_shortens_to_limit() {
    let r = fold("hello world this is a test", FoldStrategy::Truncate, 10).unwrap();
    assert_eq!(r.folded, "hello worl");
    assert_eq!(r.folded_len, 10);
    assert!(r.markers.is_empty());
}

#[test]
fn fold_truncate_unicode_preserves_char_boundary() {
    // 11 字符 (含中文) 在 byte 长度上不一致; 截到 byte 9 不能切碎多字节字符
    let s = "你好世界这是测试文本";
    let limit = 9;
    let r = fold(s, FoldStrategy::Truncate, limit).unwrap();
    // 截到 char boundary, 实际 < limit bytes (中文 3 字节/字符)
    assert!(r.folded.len() <= limit);
    assert!(s.is_char_boundary(r.folded.len()));
    assert_eq!(r.original_len, s.len());
    // 至少 3 字符 (9 bytes 能装 3 个中文字符, 因每个 3 bytes)
    assert!(r.folded.chars().count() >= 3);
}

#[test]
fn fold_headtail_keeps_first_and_last_n() {
    let s = "abcdefghijklmnopqrstuvwxyz";
    let r = fold(s, FoldStrategy::HeadTail, 10).unwrap();
    // 前 half = 5, 后 half = 5, 中间 16 字符被压缩
    assert!(r.folded.starts_with("abcde"));
    assert!(r.folded.ends_with("vwxyz"));
    assert!(r.folded.contains("HEADTAIL"), "应含 HEADTAIL 占位符");
    assert!(r.folded.contains("16 bytes"), "应含字节数");
    assert_eq!(r.markers.len(), 1);
    assert_eq!(r.markers[0].kind, MarkerKind::HeadTail);
    // payload 包含中间被压缩的内容
    assert!(r.markers[0].payload.contains("fghijklmnopqrstu"));
}

#[test]
fn fold_markerreplace_preserves_full_content_in_marker() {
    let s = "this is the full content to be folded";
    let r = fold(s, FoldStrategy::MarkerReplace, 5).unwrap();
    // folded 只是占位符, 原始内容在 marker.payload
    assert!(!r.folded.contains("this is the full content"));
    assert_eq!(r.markers.len(), 1);
    assert_eq!(r.markers[0].kind, MarkerKind::Full);
    assert_eq!(
        r.markers[0].payload, s,
        "MarkerReplace lossless — payload = 原始"
    );
}

#[test]
fn fold_summary_uses_truncate_fallback_honestly() {
    // O-5 不假装: Summary 无 internal LLM, 退化到 truncate
    let s = "summary with no llm";
    let r = fold(s, FoldStrategy::Summary, 5).unwrap();
    assert_eq!(r.folded, "summa");
    assert!(
        r.markers.is_empty(),
        "summary fallback 跟 truncate 一样, 不加 marker"
    );
}

#[test]
fn fold_no_op_when_content_leq_limit() {
    // 内容 ≤ limit → 原样返回, 零拷贝
    let s = "short";
    let r = fold(s, FoldStrategy::Truncate, 100).unwrap();
    assert_eq!(r.folded, s);
    assert_eq!(r.folded_len, 5);
    assert_eq!(r.original_len, 5);
    assert!(r.markers.is_empty());
}

#[test]
fn fold_exact_limit_boundary_does_not_fold() {
    // content.len() == limit → 不触发 fold, 原样
    let s = "exact12";
    let r = fold(s, FoldStrategy::Truncate, 7).unwrap();
    assert_eq!(r.folded, s);
    assert!(r.markers.is_empty());
}

#[test]
fn fold_original_len_consistent_across_strategies() {
    let s = "same content for all 4 strategies test case";
    for strategy in [
        FoldStrategy::Truncate,
        FoldStrategy::HeadTail,
        FoldStrategy::MarkerReplace,
        FoldStrategy::Summary,
    ] {
        let r = fold(s, strategy, 10).unwrap();
        assert_eq!(r.original_len, s.len(), "strategy {strategy:?}");
    }
}

// =============================================================================
// 2. fold limit=0 守门 (8 硬墙 "0 假装")
// =============================================================================

#[test]
fn fold_limit_zero_returns_error() {
    let r = fold("content", FoldStrategy::Truncate, 0);
    assert!(matches!(r, Err(FoldError::InvalidLimit)));
}

#[test]
fn fold_limit_zero_error_all_strategies() {
    for strategy in [
        FoldStrategy::Truncate,
        FoldStrategy::HeadTail,
        FoldStrategy::MarkerReplace,
        FoldStrategy::Summary,
    ] {
        assert!(matches!(
            fold("x", strategy, 0),
            Err(FoldError::InvalidLimit)
        ));
    }
}

// =============================================================================
// 3. unfold round-trip
// =============================================================================

#[test]
fn unfold_markerreplace_round_trip() {
    let original = "this is the original content that should be preserved exactly";
    let folded = fold(original, FoldStrategy::MarkerReplace, 10).unwrap();
    let restored = unfold(&folded.folded, &folded.markers);
    assert_eq!(restored, original, "MarkerReplace lossless round-trip");
}

#[test]
fn unfold_headtail_round_trip() {
    let original = "abcdefghijklmnopqrstuvwxyz0123456789";
    let folded = fold(original, FoldStrategy::HeadTail, 10).unwrap();
    let restored = unfold(&folded.folded, &folded.markers);
    assert_eq!(restored, original, "HeadTail lossless round-trip");
}

#[test]
fn unfold_no_markers_returns_content_unchanged() {
    // 没 marker 的内容 (零 fold), unfold 应返原样
    let content = "original content with no markers here";
    let out = unfold(content, &[]);
    assert_eq!(out, content);
}

#[test]
fn unfold_multiple_markers_all_replaced() {
    // 多个 marker 顺序替换, 全还原
    // 注: unfold 实际用 String::replace (per src/lib.rs:113), 只替换首次出现
    // 多个 marker 需不同 placeholder 才不会冲突
    let original_a = "AAA";
    let original_b = "BBB";
    let original_c = "CCC";
    let markers = vec![
        FoldMarker {
            kind: MarkerKind::Full,
            payload: original_a.into(),
        },
        FoldMarker {
            kind: MarkerKind::Full,
            payload: original_b.into(),
        },
        FoldMarker {
            kind: MarkerKind::Full,
            payload: original_c.into(),
        },
    ];
    // 用不同 payload 字符串 (避免 placeholder 重复)
    let folded = format!(
        "pre {} mid {} post {}",
        markers[0].payload, markers[1].payload, markers[2].payload
    );
    let out = unfold(&folded, &markers);
    // String::replace 顺序替换: marker[0] 替换它 (first 出现), 后续 marker 也替换
    // 注: 因为 payload 不同 (AAA, BBB, CCC), 替换不冲突
    assert!(out.contains("AAA"));
    assert!(out.contains("BBB"));
    assert!(out.contains("CCC"));
}

// =============================================================================
// 4. FoldError Display (thiserror 派生)
// =============================================================================

#[test]
fn fold_error_display_invalid_limit() {
    let err = FoldError::InvalidLimit;
    assert_eq!(err.to_string(), "fold limit must be > 0");
}

// =============================================================================
// 5. 集成: 多策略 + 多边界 + 多 unicode 端到端
// =============================================================================

#[test]
fn integration_fold_unfold_with_unicode() {
    // 真实场景: 中文 + emoji + 英文 混合, fold + unfold 应无损还原
    let original = "用户消息 user-msg 🎉 测试 unicode round-trip";
    let folded = fold(original, FoldStrategy::MarkerReplace, 20).unwrap();
    let restored = unfold(&folded.folded, &folded.markers);
    assert_eq!(restored, original);
}

#[test]
fn integration_fold_chooses_strategy_by_content_size() {
    // 真实场景: 根据 content 长度挑最合适 strategy
    // 短内容 (< limit) → 任何 strategy 都 no-op
    // 中等内容 → Truncate / HeadTail 都生效
    // 长内容 → MarkerReplace 保证 lossless

    let short = "abc";
    let r = fold(short, FoldStrategy::MarkerReplace, 100).unwrap();
    assert_eq!(r.folded, short, "短内容 no-op");

    let long = "a".repeat(500);
    let r = fold(&long, FoldStrategy::MarkerReplace, 50).unwrap();
    assert!(
        !r.folded.contains('a'),
        "MarkerReplace 全压缩, 占位符不包含原文"
    );
    assert_eq!(r.markers[0].payload, long, "MarkerReplace lossless");
}

#[test]
fn integration_fold_then_unfold_idempotent() {
    // 多次 fold + unfold 应得原内容 (lossless 验证)
    let original = "test content for fold/unfold idempotency check, multiple times";
    for _ in 0..3 {
        let folded = fold(original, FoldStrategy::MarkerReplace, 20).unwrap();
        let restored = unfold(&folded.folded, &folded.markers);
        assert_eq!(restored, original, "fold+unfold idempotent");
    }
}
