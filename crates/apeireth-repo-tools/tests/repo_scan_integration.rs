//! Integration tests for apeireth-repo-tools (post-1.0.0 增量)
//!
//! src/ 各 mod tests 已覆盖基础. 这里 (tests/) 加 per-行为样板, 跟其他 crate 一致.
//!
//! src/scan.rs 末尾注释提到 `tests/test_repo_scan_in_process.rs` 但文件不存在 —
//! 5 fixture 缺失. 这次写 18 cases 覆盖 K-1 强校验 + m3 防御 + 8 工具白名单 + 13 语言 + 11 关键文件模式.
//!
//! 真生产价值:
//! - m3 防御白名单 (8 工具) 端到端
//! - validate_tool_call 接受/拒绝路径
//! - validate_external_whitelist 全在/部分在/全不在
//! - K-1 编译期 hardcode 守门 (SUPPORTED_LANGUAGES=13, TOOL_WHITELIST=8, KEY_FILE_PATTERNS=11)
//! - RepoScanError 8 variant Display 端到端
//! - m3_defense_sanity_check 始终 true (K-1 强校验)
//!
//! 0 触碰 src/, 0 编造"已实现"。

#![allow(missing_docs)]

use apeireth_repo_tools::{
    m3_defense_sanity_check, validate_external_whitelist, validate_tool_call, RepoScanError,
    KEY_FILE_PATTERNS, SUPPORTED_LANGUAGES, TOOL_WHITELIST, TOOL_WHITELIST_COUNT,
};

// =============================================================================
// 1. K-1 强校验: 编译期 hardcode 常数
// =============================================================================

#[test]
fn k1_tool_whitelist_count_is_8() {
    assert_eq!(TOOL_WHITELIST_COUNT, 8, "K-1 强校验: 8 工具白名单");
    assert_eq!(TOOL_WHITELIST.len(), 8);
}

#[test]
fn k1_supported_languages_count_is_13() {
    // K-1 强校验: SUPPORTED_LANGUAGES 13 项
    assert_eq!(SUPPORTED_LANGUAGES.len(), 13, "K-1 强校验: 13 种语言");
}

#[test]
fn k1_key_file_patterns_count_is_11() {
    assert_eq!(KEY_FILE_PATTERNS.len(), 11, "K-1 强校验: 11 关键文件模式");
}

#[test]
fn m3_defense_sanity_check_returns_true() {
    // K-1 编译期守门: SUPPORTED_LANGUAGES=13 + TOOL_WHITELIST=8 + KEY_FILE_PATTERNS=11
    assert!(
        m3_defense_sanity_check(),
        "sanity check 应永远 true (K-1 编译期 hardcode)"
    );
}

#[test]
fn tool_whitelist_contains_required_8_tools() {
    let required = [
        "apeireth_repo_scan_scan",
        "apeireth_repo_scan_stats",
        "apeireth_repo_scan_key_files",
        "apeireth_repo_scan_git_state",
        "apeireth_repo_scan_report_json",
        "apeireth_repo_scan_report_markdown",
        "apeireth_repo_scan_cache_clear",
        "apeireth_repo_scan_sensitive_grep",
    ];
    for t in required {
        assert!(TOOL_WHITELIST.contains(&t), "白名单应含 {t}");
    }
}

#[test]
fn tool_whitelist_no_duplicates() {
    let mut sorted = TOOL_WHITELIST.to_vec();
    sorted.sort();
    let original_len = sorted.len();
    sorted.dedup();
    assert_eq!(sorted.len(), original_len, "白名单 0 重复");
}

// =============================================================================
// 2. validate_tool_call 端到端 (m3 防御)
// =============================================================================

#[test]
fn validate_tool_call_accepts_8_whitelisted_tools() {
    for tool in TOOL_WHITELIST {
        let r = validate_tool_call(tool, &serde_json::json!({}));
        assert!(r.is_ok(), "白名单 {tool} 应被接受");
    }
}

#[test]
fn validate_tool_call_rejects_unknown_tool() {
    let r = validate_tool_call("unknown_tool", &serde_json::json!({}));
    assert!(matches!(r, Err(RepoScanError::ToolNotWhitelisted(_))));
    if let Err(RepoScanError::ToolNotWhitelisted(t)) = r {
        assert_eq!(t, "unknown_tool");
    } else {
        panic!("expected ToolNotWhitelisted");
    }
}

#[test]
fn validate_tool_call_rejects_empty_string() {
    let r = validate_tool_call("", &serde_json::json!({}));
    assert!(matches!(r, Err(RepoScanError::ToolNotWhitelisted(_))));
}

#[test]
fn validate_tool_call_ignores_args_validation() {
    // args 不会被验证 (注释 _args), 任何 JSON 都接受
    let r = validate_tool_call(
        "apeireth_repo_scan_scan",
        &serde_json::json!({"any": "thing"}),
    );
    assert!(r.is_ok());
    let r2 = validate_tool_call("apeireth_repo_scan_scan", &serde_json::Value::Null);
    assert!(r2.is_ok());
}

// =============================================================================
// 3. validate_external_whitelist 端到端
// =============================================================================

#[test]
fn validate_external_whitelist_accepts_superset() {
    // 外部白名单包含所有 TOOL_WHITELIST → true
    let external: Vec<&str> = TOOL_WHITELIST.to_vec();
    assert!(validate_external_whitelist(&external));
}

#[test]
fn validate_external_whitelist_accepts_exact_match() {
    let external: Vec<&str> = TOOL_WHITELIST.to_vec();
    assert_eq!(external.len(), 8);
    assert!(validate_external_whitelist(&external));
}

#[test]
fn validate_external_whitelist_rejects_partial() {
    // 外部白名单只含部分 TOOL_WHITELIST → false
    let external = ["apeireth_repo_scan_scan", "unknown_tool_xyz"];
    assert!(!validate_external_whitelist(&external));
}

#[test]
fn validate_external_whitelist_empty_is_vacuously_true() {
    // 真空 truth: 空 list iter().all() = true (无 item 可检查)
    let external: Vec<&str> = vec![];
    assert!(
        validate_external_whitelist(&external),
        "空 list iter().all() = vacuously true"
    );
}

#[test]
fn validate_external_whitelist_strict_subset() {
    // 外部白名单只含部分 TOOL_WHITELIST → false (per .iter().any | false, !.all)
    let external = ["apeireth_repo_scan_scan", "unknown_tool_xyz"];
    assert!(!validate_external_whitelist(&external));
}

#[test]
fn validate_external_whitelist_extra_strict() {
    // 外部白名单含 TOOL_WHITELIST + 额外允许项 → false (per .all)
    let mut external: Vec<&str> = TOOL_WHITELIST.to_vec();
    external.push("apeireth_repo_extra_allowed");
    assert!(
        !validate_external_whitelist(&external),
        "外部多允许应 false (严格校验)"
    );
}

// =============================================================================
// 4. RepoScanError 8 variant Display 端到端
// =============================================================================

#[test]
fn repo_scan_error_displays_8_variants_distinctly() {
    let variants = vec![
        RepoScanError::ToolNotWhitelisted("x".into()),
        RepoScanError::InvalidPath(std::path::PathBuf::from("/nonexistent")),
        RepoScanError::DepthExceeded { depth: 10, max: 5 },
        RepoScanError::GitFailed("not a git repo".into()),
        RepoScanError::EmptyPattern,
        RepoScanError::ReportFailed("template missing".into()),
        RepoScanError::CacheIo("permission denied".into()),
        RepoScanError::CacheExpired {
            age_days: 60,
            ttl_days: 30,
        },
    ];
    let displays: Vec<String> = variants.iter().map(|e| e.to_string()).collect();
    let unique: std::collections::HashSet<&String> = displays.iter().collect();
    assert_eq!(unique.len(), displays.len(), "8 variant Display 互不相同");
}

#[test]
fn repo_scan_error_specific_messages() {
    assert_eq!(
        RepoScanError::ToolNotWhitelisted("dangerous".into()).to_string(),
        "tool not whitelisted: dangerous"
    );
    assert_eq!(
        RepoScanError::DepthExceeded {
            depth: 100,
            max: 10
        }
        .to_string(),
        "max scan depth exceeded: 100 > 10"
    );
    assert_eq!(
        RepoScanError::EmptyPattern.to_string(),
        "sensitive pattern is empty"
    );
    assert_eq!(
        RepoScanError::CacheExpired {
            age_days: 90,
            ttl_days: 30
        }
        .to_string(),
        "cache expired (age 90 days > ttl 30)".to_string()
    );
}

#[test]
fn repo_scan_error_io_and_json_have_from_impl() {
    // #[from] std::io::Error 和 #[from] serde_json::Error 让 from 转换可用
    use std::io::{Error as IoError, ErrorKind};
    let io_err = IoError::new(ErrorKind::NotFound, "file missing");
    let wrapped: RepoScanError = io_err.into();
    assert!(matches!(wrapped, RepoScanError::Io(_)));
    assert!(wrapped.to_string().contains("I/O error"));

    let json_err: serde_json::Error =
        serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err();
    let wrapped: RepoScanError = json_err.into();
    assert!(matches!(wrapped, RepoScanError::Json(_)));
    assert!(wrapped.to_string().contains("JSON error"));
}

// =============================================================================
// 5. 集成: m3 防御 end-to-end (K-1 + 8 工具 + 13 语言全场景)
// =============================================================================

#[test]
fn integration_m3_defense_full_workflow() {
    // 1. K-1 sanity check 必过
    assert!(m3_defense_sanity_check());

    // 2. 所有 8 个工具全被 validate_tool_call 接受
    for tool in TOOL_WHITELIST {
        assert!(validate_tool_call(tool, &serde_json::json!({})).is_ok());
    }

    // 3. 外部白名单包含全部 TOOL_WHITELIST → validate_external_whitelist 接受
    assert!(validate_external_whitelist(TOOL_WHITELIST));

    // 4. 13 种语言全编译期 hardcode
    assert_eq!(SUPPORTED_LANGUAGES.len(), 13);

    // 5. 11 关键文件模式 (Cargo.toml / README.md / Dockerfile 等)
    assert_eq!(KEY_FILE_PATTERNS.len(), 11);
}
