//! Apeireth repository tooling facade.
//!
//! Scanning and quality analysis remain separate modules because their public
//! contracts intentionally contain similarly named types such as `CacheEntry`
//! and `ReportGenerator`.

#![warn(missing_docs)]

/// Filesystem, git-state, sensitive-content, and report scanning.
pub mod scan;
// R177: organ invariants (5 tests + 2 Kani)
/// Technical-debt, complexity, dependency, and security analysis.
pub mod analyzer;
mod organ_kani_proofs;
/// N17/TP2: 装配统一注册件 (§10 铁边界: Tool + ToolRegistry.register)
pub mod register;

// K-1 强校验 + m3 防御公开 API (per 蓝图 §2.3.1 + 测试 fixture 缺失补回).
// 集成 test (tests/repo_scan_integration.rs) 用这些常量, 0 触碰 scan mod 私有.
pub use scan::{
    m3_defense_sanity_check, validate_external_whitelist, validate_tool_call, RepoScanError,
    KEY_FILE_PATTERNS, SUPPORTED_LANGUAGES, TOOL_WHITELIST, TOOL_WHITELIST_COUNT,
};
