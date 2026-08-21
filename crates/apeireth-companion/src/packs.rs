//! `apeireth-companion::packs` — 权限包: 一次强确认 → 授权一段时间.
//!
//! 对齐 stage2-decisions-permission-packs.md (2026-07-30) + 主人 2026-08-15 拍板:
//! - 日常包默认永久 (用户可自定义); 90 天强制提醒续签;
//! - 不可逆/对外类可进包 (责任自负 = sudo 模式, 监督机制兜底);
//! - 一次强确认 (Windows Hello) 签包后, 包有效期内不限次 (sudo -v 的包级扩展).
//!
//! 诚实: 生物识别确认 (Windows Hello/YubiKey) 是「签包前的动作」, 本模块只做
//! 确认后的**登记与执行期管理**; 真实 Windows Hello 绑定是平台 SDK 的下一步,
//! sovereignty 的 MockBiometric/CoercionBehavior 已备好口子.

use std::sync::Mutex;

/// 有效期.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackExpiry {
    /// 永久 (默认日常包; 90 天提醒续签)
    Permanent,
    /// 限时 (小时)
    Hours(u64),
    /// 单次
    SingleUse,
}

/// 权限包: 用户预先签发的意图集 (capability).
#[derive(Debug, Clone)]
pub struct PermissionPack {
    pub id: String,
    pub name: String,
    /// 覆盖的工具名 (如 "FileOperator", "ShellExec")
    pub tools: Vec<String>,
    /// 路径前缀约束 (文件类工具适用; 空 = 不约束)
    pub paths: Vec<String>,
    pub expiry: PackExpiry,
    /// 操作预算 (总次数上限; None = 不限)
    pub op_budget: Option<u32>,
    pub used_ops: u32,
    /// 花费预算 (对齐 hydra vault access: spend 上限; None = 不限)
    pub spend_budget: Option<u64>,
    pub spend_used: u64,
    /// B3 沙盒参数口: 本包覆盖的工具执行时使用的资源限额 (None = 用桥级默认).
    pub sandbox: Option<crate::sandbox::SandboxConfig>,
    pub activated_at_ms: i64,
    pub created_at_ms: i64,
}

impl PermissionPack {
    pub fn permanent(name: &str, tools: Vec<String>) -> Self {
        Self {
            id: format!("pack-{}", uuid::Uuid::new_v4()),
            name: name.to_string(),
            tools,
            paths: Vec::new(),
            expiry: PackExpiry::Permanent,
            op_budget: None,
            used_ops: 0,
            spend_budget: None,
            spend_used: 0,
            sandbox: None,
            activated_at_ms: now_ms(),
            created_at_ms: now_ms(),
        }
    }

    pub fn timed(name: &str, tools: Vec<String>, hours: u64, budget: Option<u32>) -> Self {
        Self {
            id: format!("pack-{}", uuid::Uuid::new_v4()),
            name: name.to_string(),
            tools,
            paths: Vec::new(),
            expiry: PackExpiry::Hours(hours),
            sandbox: None,
            op_budget: budget,
            used_ops: 0,
            spend_budget: None,
            spend_used: 0,
            activated_at_ms: now_ms(),
            created_at_ms: now_ms(),
        }
    }

    /// 设花费上限 (花钱类工具: 模型调用/付费 API 等).
    /// B3 沙盒参数口: 给包附沙盒限额配置 (覆盖该工具时用此配置, 否则桥级默认).
    pub fn with_sandbox(mut self, cfg: crate::sandbox::SandboxConfig) -> Self {
        self.sandbox = Some(cfg);
        self
    }

    pub fn with_spend_budget(mut self, budget: u64) -> Self {
        self.spend_budget = Some(budget);
        self
    }

    /// 花费一笔: 预算内 → 记账返回 true; 超限 → false (不记账).
    pub fn try_spend(&mut self, amount: u64) -> bool {
        match self.spend_budget {
            Some(b) if self.spend_used + amount > b => false,
            _ => {
                self.spend_used += amount;
                true
            }
        }
    }

    pub fn with_paths(mut self, paths: Vec<String>) -> Self {
        self.paths = paths;
        self
    }

    /// 是否过期.
    pub fn is_expired(&self, now_ms: i64) -> bool {
        match self.expiry {
            PackExpiry::Permanent => false,
            PackExpiry::Hours(h) => now_ms >= self.activated_at_ms + (h as i64) * 3600_000,
            PackExpiry::SingleUse => self.used_ops >= 1,
        }
    }

    /// 是否覆盖某工具.
    pub fn covers(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }

    /// 是否还有预算.
    pub fn has_budget(&self) -> bool {
        self.op_budget.map_or(true, |b| self.used_ops < b)
    }

    /// 是否需 90 天续签提醒 (仅永久包).
    pub fn needs_renewal_reminder(&self, now_ms: i64) -> bool {
        self.expiry == PackExpiry::Permanent && now_ms >= self.created_at_ms + 90 * 24 * 3600_000
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 权限包注册表: 签包登记 + 执行期检查 + 记账.
pub struct PackRegistry {
    inner: Mutex<Vec<PermissionPack>>,
}

impl Default for PackRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PackRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    /// 默认日常包 (永久, 只读工具集 + 记忆写; 主人可自定义扩权).
    pub fn default_daily_pack() -> PermissionPack {
        PermissionPack::permanent(
            "日常包",
            vec![
                "recall_memory".to_string(),
                "save_memory".to_string(),
                "propose_capability".to_string(),
                "simulate".to_string(),
                "forecast".to_string(),
                "WebSearch".to_string(),
                "Grep".to_string(),
                "WebFetch".to_string(),
                "Git".to_string(),
            ],
        )
    }

    /// 用户签一个包 (强确认后登记).
    pub fn grant(&self, pack: PermissionPack) {
        self.inner.lock().unwrap().push(pack);
    }

    /// 撤销.
    pub fn revoke(&self, id: &str) {
        self.inner.lock().unwrap().retain(|p| p.id != id);
    }

    /// 按名撤销 (插件/套件卸载用: 名字是稳定的, UUID 每次生成不同).
    pub fn revoke_by_name(&self, name: &str) {
        self.inner.lock().unwrap().retain(|p| p.name != name);
    }

    /// 检查动作是否被某活跃包覆盖; 覆盖则记账并返回 true.
    pub fn check_and_consume(&self, tool: &str, now_ms: i64) -> bool {
        let mut packs = self.inner.lock().unwrap();
        for p in packs.iter_mut() {
            if !p.is_expired(now_ms) && p.covers(tool) && p.has_budget() {
                p.used_ops += 1;
                return true;
            }
        }
        false
    }

    /// 需续签提醒的包名列表 (90 天).
    pub fn renewal_reminders(&self, now_ms: i64) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.needs_renewal_reminder(now_ms))
            .map(|p| p.name.clone())
            .collect()
    }

    /// 覆盖该工具且带路径约束 (paths 非空) 的活跃包路径列表.
    /// 供执行器做**执行级路径校验** (权限包 paths 不只是元数据).
    pub fn paths_for(&self, tool: &str, now_ms: i64) -> Option<Vec<String>> {
        let packs = self.inner.lock().unwrap();
        packs
            .iter()
            .find(|p| !p.is_expired(now_ms) && p.covers(tool) && !p.paths.is_empty())
            .map(|p| p.paths.clone())
    }

    /// B3 沙盒参数口: 覆盖该工具且带沙盒限额的活跃包配置 (无 = 用桥级默认).
    /// 语义对齐 `paths_for`: 权限包不只授权, 还可携带执行期资源参数.
    pub fn sandbox_for(&self, tool: &str, now_ms: i64) -> Option<crate::sandbox::SandboxConfig> {
        let packs = self.inner.lock().unwrap();
        packs
            .iter()
            .find(|p| !p.is_expired(now_ms) && p.covers(tool) && p.sandbox.is_some())
            .and_then(|p| p.sandbox.clone())
    }

    pub fn active_count(&self, now_ms: i64) -> usize {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .filter(|p| !p.is_expired(now_ms))
            .count()
    }

    // ============================================================
    // Core Capability Expansion Phase 4 — grant 可见性 / 撤销 / 过期
    // ============================================================

    /// 列出全部 grants 的视图 (含 active/expired 状态, 供 Tools 页展示 + revoke).
    pub fn list_grants(&self, now_ms: i64) -> Vec<GrantView> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .map(|p| GrantView::from_pack(p, now_ms))
            .collect()
    }

    /// 撤销指定 grant (按 id). 返回是否撤销成功 (id 不存在 → false).
    /// 撤销后下一次 check_and_consume 立即不再覆盖该工具 (revoke 即时生效).
    pub fn revoke_grant(&self, id: &str) -> bool {
        let mut packs = self.inner.lock().unwrap();
        let before = packs.len();
        packs.retain(|p| p.id != id);
        packs.len() != before
    }

    /// 查找某工具的当前授权决策 (deterministic evaluation).
    /// 返回 (允许, 匹配的 pack id). 用于权限评估 (不记账; 记账走 check_and_consume).
    pub fn evaluate(&self, tool: &str, now_ms: i64) -> GrantDecision {
        let packs = self.inner.lock().unwrap();
        for p in packs.iter() {
            if !p.is_expired(now_ms) && p.covers(tool) && p.has_budget() {
                return GrantDecision::Allow {
                    pack_id: p.id.clone(),
                    pack_name: p.name.clone(),
                    expiry: format!("{:?}", p.expiry),
                };
            }
        }
        // 有覆盖但过期/无预算 → deny (不是 require-approval; approval 走另一套 ApprovalManager).
        let covered_expired = packs
            .iter()
            .any(|p| p.covers(tool) && (p.is_expired(now_ms) || !p.has_budget()));
        if covered_expired {
            GrantDecision::Deny {
                reason: "grant expired or budget exhausted".into(),
            }
        } else {
            GrantDecision::RequireApproval
        }
    }
}

/// Grant 视图 (只读, 供 HTTP/UI 展示). 不含 secret.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrantView {
    pub id: String,
    pub name: String,
    pub tools: Vec<String>,
    pub paths: Vec<String>,
    pub expiry: String,
    pub op_budget: Option<u32>,
    pub used_ops: u32,
    pub spend_budget: Option<u64>,
    pub spend_used: u64,
    pub activated_at_ms: i64,
    pub created_at_ms: i64,
    /// 当前是否活跃 (未过期 + 有预算).
    pub active: bool,
    /// 是否已过期.
    pub expired: bool,
}

impl GrantView {
    pub fn from_pack(p: &PermissionPack, now_ms: i64) -> Self {
        let expired = p.is_expired(now_ms);
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            tools: p.tools.clone(),
            paths: p.paths.clone(),
            expiry: format!("{:?}", p.expiry),
            op_budget: p.op_budget,
            used_ops: p.used_ops,
            spend_budget: p.spend_budget,
            spend_used: p.spend_used,
            activated_at_ms: p.activated_at_ms,
            created_at_ms: p.created_at_ms,
            active: !expired && p.has_budget(),
            expired,
        }
    }
}

/// 权限评估决策 (deterministic).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum GrantDecision {
    /// 允许 (匹配到活跃包).
    Allow {
        pack_id: String,
        pack_name: String,
        expiry: String,
    },
    /// 拒绝 (覆盖但过期/无预算).
    Deny { reason: String },
    /// 需批准 (无覆盖 — 走 ApprovalManager).
    RequireApproval,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_pack_never_expires() {
        let p = PermissionPack::permanent("日常", vec!["WebSearch".into()]);
        assert!(!p.is_expired(now_ms() + 10 * 365 * 24 * 3600_000));
    }

    #[test]
    fn timed_pack_expires() {
        let p = PermissionPack::timed("学习", vec!["ShellExec".into()], 24, None);
        assert!(!p.is_expired(now_ms()));
        assert!(p.is_expired(now_ms() + 25 * 3600_000));
    }

    #[test]
    fn single_use_consumed() {
        let mut p = PermissionPack::timed("单次", vec!["ShellExec".into()], 1, Some(1));
        p.expiry = PackExpiry::SingleUse;
        p.used_ops += 1;
        assert!(p.is_expired(now_ms()));
    }

    #[test]
    fn budget_exhausted() {
        let mut p = PermissionPack::timed("限次", vec!["FileOperator".into()], 1, Some(2));
        p.used_ops = 2;
        assert!(!p.has_budget());
    }

    #[test]
    fn registry_consumes_covered_tool() {
        let r = PackRegistry::new();
        r.grant(PackRegistry::default_daily_pack());
        assert!(r.check_and_consume("WebSearch", now_ms()));
        // ShellExec 不在日常包 → false
        assert!(!r.check_and_consume("ShellExec", now_ms()));
    }

    #[test]
    fn ninety_day_reminder_fires() {
        let mut p = PermissionPack::permanent("日常", vec![]);
        p.created_at_ms = 0;
        assert!(p.needs_renewal_reminder(91 * 24 * 3600_000));
        assert!(!p.needs_renewal_reminder(89 * 24 * 3600_000));
    }

    #[test]
    fn sandbox_config_lookup_by_covered_tool() {
        // B3 参数口: 包级沙盒配置按覆盖工具可查 (无配置包 → None → 桥级默认)
        let r = PackRegistry::new();
        r.grant(
            PermissionPack::permanent("沙盒包", vec!["ShellExec".into()]).with_sandbox(
                crate::sandbox::SandboxConfig {
                    memory_limit_mb: Some(256),
                    timeout_secs: 60,
                    ..crate::sandbox::SandboxConfig::default()
                },
            ),
        );
        r.grant(PackRegistry::default_daily_pack());
        let cfg = r
            .sandbox_for("ShellExec", now_ms())
            .expect("应有包级沙盒配置");
        assert_eq!(cfg.memory_limit_mb, Some(256));
        assert_eq!(cfg.timeout_secs, 60);
        // WebSearch 只被无沙盒配置的日常包覆盖 → None
        assert!(r.sandbox_for("WebSearch", now_ms()).is_none());
    }

    // ===== Core Capability Expansion Phase 4: grant 可见性 / 撤销 / 评估 =====

    #[test]
    fn phase4_list_grants_shows_active_and_expired() {
        let r = PackRegistry::new();
        let active = PermissionPack::timed("活跃包", vec!["ShellExec".into()], 24, None);
        let active_id = active.id.clone();
        r.grant(active);
        // 一个已过期的包
        let mut expired = PermissionPack::timed("过期包", vec!["FileOperator".into()], 1, None);
        expired.activated_at_ms = now_ms() - 2 * 3600_000; // 2 小时前激活, 1 小时过期
        r.grant(expired);
        let grants = r.list_grants(now_ms());
        assert_eq!(grants.len(), 2);
        let active_view = grants.iter().find(|g| g.id == active_id).unwrap();
        assert!(active_view.active);
        assert!(!active_view.expired);
        let expired_view = grants.iter().find(|g| !g.active).unwrap();
        assert!(expired_view.expired);
    }

    #[test]
    fn phase4_revoke_grant_immediate_effect() {
        let r = PackRegistry::new();
        let pack = PermissionPack::timed("可撤销", vec!["ShellExec".into()], 24, None);
        let id = pack.id.clone();
        r.grant(pack);
        assert!(r.check_and_consume("ShellExec", now_ms()));
        // 撤销
        assert!(r.revoke_grant(&id));
        // 撤销后立即不再覆盖 (revoke 即时生效)
        assert!(!r.check_and_consume("ShellExec", now_ms()));
        // 二次撤销同一 id → false (已不存在)
        assert!(!r.revoke_grant(&id));
    }

    #[test]
    fn phase4_evaluate_allow_deny_require_approval() {
        let r = PackRegistry::new();
        // 无覆盖 → RequireApproval
        match r.evaluate("ShellExec", now_ms()) {
            GrantDecision::RequireApproval => {}
            other => panic!("expected RequireApproval, got {other:?}"),
        }
        // 授予 → Allow
        r.grant(PermissionPack::timed(
            "授权",
            vec!["ShellExec".into()],
            24,
            None,
        ));
        match r.evaluate("ShellExec", now_ms()) {
            GrantDecision::Allow { pack_name, .. } => assert_eq!(pack_name, "授权"),
            other => panic!("expected Allow, got {other:?}"),
        }
        // 覆盖但过期 → Deny
        let mut expired = PermissionPack::timed("过期", vec!["FileOperator".into()], 1, None);
        expired.activated_at_ms = now_ms() - 2 * 3600_000;
        r.grant(expired);
        match r.evaluate("FileOperator", now_ms()) {
            GrantDecision::Deny { .. } => {}
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn phase4_grant_view_no_secret() {
        let r = PackRegistry::new();
        r.grant(PermissionPack::timed(
            "包",
            vec!["ShellExec".into()],
            1,
            None,
        ));
        let json = serde_json::to_string(&r.list_grants(now_ms())).unwrap();
        // grant view 不含 secret
        assert!(!json.contains("api_key"));
        assert!(!json.contains("master_token"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn phase4_expiry_boundary() {
        let r = PackRegistry::new();
        // 1 小时包, 激活于 now
        let pack = PermissionPack::timed("边界", vec!["ShellExec".into()], 1, None);
        let activated = pack.activated_at_ms;
        r.grant(pack);
        // 正好 3600s = 边界: now_ms >= activated + 3600000 → 过期
        assert!(r.check_and_consume("ShellExec", activated + 3_599_999));
        assert!(!r.check_and_consume("ShellExec", activated + 3_600_000));
    }
}
