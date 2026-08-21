//! `apeireth-companion::tool_bridge` — 把主动循环焊到基地工具栈.
//!
//! 「基地对他强大而友好」的最后一根线:
//! - **全量工具**: apeireth-tools 4 真工具 (web_search/file_ops/git_ops/code_exec) + recall_memory.
//! - **安全机制守护** (不吝啬授权, 靠安全机制守):
//!   1. 洋葱门 (V1 哲学 × V2 权限 × V3 HA) 先于一切;
//!   2. 审批规则: 黑名单(最严) → 白名单(recall_memory 放行) → 风险(code/shell/exec → 需主人批准);
//!   3. 出站隐私脱敏在送达层 (daemon.rs).
//!
//! 诚实: 审批的「需主人批准」在主动循环里 = 「不自主执行, 如实告诉住客 AI 需要主人」.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use apeireth_core::{ActionTarget, ActionVerdict, RiskLevel};
use apeireth_memory::{CoreEpisode, EpisodeQuery, EpisodeStore, SqliteMemoryStore};
use apeireth_tool_approval::{
    ApprovalDecision, ApprovalManager, BlacklistRule, RiskRule, WhitelistRule,
};
use apeireth_tool_registry::{Tool, ToolAxes, ToolKind, ToolRegistry};
use apeireth_tool_runtime::executor::{ExecutionResult, ToolExecutor};
use apeireth_tool_runtime::parser::ParsedToolCall;
use apeireth_tool_runtime::record::RecordStore;
use serde_json::{json, Value};

use crate::capability::{CapabilityKind, CapabilityRegistry};
use crate::constitution_gate::ConstitutionGate;
use crate::daemon::{requires_llm_review, Judicator};
use crate::oracle::{Entity, Forecast, ForecastRegistry, ScenarioEngine, WorldState};
use crate::packs::PackRegistry;
use crate::security::{SecurityGate, SovereigntyGate};
use crate::spill::{SpillStore, SPILL_THRESHOLD_CHARS};

/// 路径前缀白名单校验 (执行级, 防越权写盘 + `..` 穿越).
///
/// 规则: 规范化 (Windows 分隔符/大小写统一) 后, `path` 必须等于 `base` 或位于
/// `base/` 之下. 目标文件可能不存在 → canonicalize 父目录 + 文件名再比 (`..` 被解析).
fn path_within(path: &str, base: &str) -> bool {
    use std::path::Path;
    let norm = |p: &std::path::PathBuf| -> String {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    let base_p = Path::new(base);
    let base_c = std::fs::canonicalize(base_p).unwrap_or_else(|_| base_p.to_path_buf());
    let path_p = Path::new(path);
    let path_c = match std::fs::canonicalize(path_p) {
        Ok(c) => c,
        Err(_) => match path_p
            .parent()
            .and_then(|pa| std::fs::canonicalize(pa).ok())
        {
            Some(cp) => cp.join(path_p.file_name().unwrap_or_default()),
            None => path_p.to_path_buf(),
        },
    };
    let (b, p) = (norm(&base_c), norm(&path_c));
    p == b || p.starts_with(&format!("{b}/"))
}

/// 「回忆记忆」工具 — 基地给住客 AI 的第一个自研工具 (只读, 最安全).
pub struct RecallMemoryTool {
    store: Arc<SqliteMemoryStore>,
}

impl RecallMemoryTool {
    pub fn new(store: Arc<SqliteMemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl Tool for RecallMemoryTool {
    fn name(&self) -> &str {
        "recall_memory"
    }
    fn kind(&self) -> ToolKind {
        ToolKind::Sync
    }
    fn axes(&self) -> ToolAxes {
        ToolAxes::default()
    }
    async fn call(&self, args: Value) -> Result<Value, String> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        if query.trim().is_empty() {
            return Err("query 不能为空".to_string());
        }
        let eps = self
            .store
            .query(&EpisodeQuery::new().limit(200))
            .map_err(|e| e.to_string())?;
        let terms: Vec<String> = query
            .split(|c: char| {
                c.is_whitespace() || matches!(c, '，' | ',' | '、' | '。' | '.' | '?' | '？')
            })
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        let mut scored: Vec<(usize, String)> = eps
            .into_iter()
            .filter_map(|ep| {
                let n = terms
                    .iter()
                    .filter(|t| ep.content.contains(t.as_str()))
                    .count();
                if n > 0 {
                    Some((n, ep.content))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let hits: Vec<String> = scored.into_iter().take(3).map(|(_, c)| c).collect();
        Ok(json!({
            "query": query,
            "found": hits.len(),
            "top": hits,
        }))
    }
}

/// 「沉淀记忆」工具 — 基地给住客 AI 的记忆写入口 (append-only, 低危).
///
/// 用途: AI 自己总结对话/经历后, 主动把值得长期记住的事实写回真 SQLite.
/// 约束: 单条 <= 500 字; 只能追加 (SQLite append-only, 无覆盖/删除).
pub struct SaveMemoryTool {
    store: Arc<SqliteMemoryStore>,
}

impl SaveMemoryTool {
    pub fn new(store: Arc<SqliteMemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl Tool for SaveMemoryTool {
    fn name(&self) -> &str {
        "save_memory"
    }
    fn kind(&self) -> ToolKind {
        ToolKind::Sync
    }
    fn axes(&self) -> ToolAxes {
        ToolAxes::default()
    }
    async fn call(&self, args: Value) -> Result<Value, String> {
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "content 不能为空".to_string())?;
        if content.chars().count() > 500 {
            return Err("记忆内容过长 (单条 <= 500 字)".to_string());
        }
        let session_id = args
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("me");
        let ep = CoreEpisode {
            id: format!("mem-{}", uuid::Uuid::new_v4()),
            timestamp: chrono::Utc::now().timestamp(),
            role: "assistant".into(),
            content: content.to_string(),
            session_id: session_id.to_string(),
        };
        self.store.put_episode(&ep).map_err(|e| e.to_string())?;
        let preview: String = content.chars().take(40).collect();
        Ok(json!({
            "ok": true,
            "id": ep.id,
            "saved": format!("{preview}…"),
        }))
    }
}

/// post-execute 钩子: 工具结果产出后、审计前执行 (可替换/拦截结果).
/// 三段瀑布 (吸收 DSH #2): pre(洋葱门→宪法评审→权限→路径) → execute(宿主/worker) → post(钩子链→spill→审计).
pub trait PostExecuteHook: Send + Sync {
    fn apply(&self, call: &ParsedToolCall, result: &ExecutionResult) -> ExecutionResult;
}

/// 「提案能力」工具 — AI 自己长能力的第一条通道 (涌现哲学).
/// 只登记提案 (pending), 不执行能力 — 激活需宪法评审/主人批准.
pub struct ProposeCapabilityTool {
    registry: Arc<CapabilityRegistry>,
}

impl ProposeCapabilityTool {
    pub fn new(registry: Arc<CapabilityRegistry>) -> Self {
        Self { registry }
    }
}
#[async_trait::async_trait]
impl Tool for ProposeCapabilityTool {
    fn name(&self) -> &str {
        "propose_capability"
    }
    fn kind(&self) -> ToolKind {
        ToolKind::Sync
    }
    fn axes(&self) -> ToolAxes {
        ToolAxes::default()
    }
    async fn call(&self, args: Value) -> Result<Value, String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "name 不能为空".to_string())?;
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let kind = match args.get("kind").and_then(|v| v.as_str()) {
            Some("action") => CapabilityKind::Action,
            _ => CapabilityKind::Skill,
        };
        let p = self.registry.propose(name, description, kind, "apeireth")?;
        Ok(json!({
            "id": p.id,
            "status": p.status.label(),
            "note": "已提案待宪法评审/主人批准",
        }))
    }
}

/// 「沙盘推演」工具 — oracle 套件: 世界状态 + 事件序列 → 规则推演各步状态.
/// 纯内存推演, 无副作用. 事件语法 (宽容, 实测模型书写习惯):
/// - 增减: "id.key+delta" / "id.key-delta" (如 "主人.复习进度+0.3", "主人.焦虑-0.1")
/// - 赋值: "id.key=delta" (delta 可带符号, 如 "主人.信心=0.5", "错题本A.收录数=8")
/// - 编号前缀: "e1.id.key±delta" 自动剥离 "e<数字>." (模型常写 "e1.主人.信心=0.5")
pub struct SimulateTool;

impl SimulateTool {
    /// 内置规则 apply — 宽容解析 (0 假装: 宽容是工程, 不是纵容 — 坏格式仍报错带示例).
    fn apply(state: &mut WorldState, event: &str) -> Result<(), String> {
        let sep_idx = event
            .find(|c| c == '+' || c == '-' || c == '=')
            .ok_or_else(|| {
                format!("事件格式应为 实体.属性±增量 (如 \"主人.复习进度+0.3\" / \"主人.信心=-0.1\"): {event}")
            })?;
        let sep = event.as_bytes()[sep_idx];
        let (path, delta_str) = event.split_at(sep_idx);
        let delta: f64 = delta_str[1..]
            .trim()
            .parse()
            .map_err(|_| format!("delta 非法: {}", &delta_str[1..]))?;
        // 语义: "+"/"-" = 增减, "=" = 赋值 (模型自然语义).
        let is_assign = sep == b'=';
        let sign = if sep == b'-' { -1.0 } else { 1.0 };
        // 实体名容忍: "e1.主人.剩余时间h" → 剥离 "e<数字>." 前缀 (模型实测写法).
        let mut path = path;
        if let Some(rest) = path.strip_prefix('e') {
            if let Some(dot) = rest.find('.') {
                if rest[..dot].chars().all(|c| c.is_ascii_digit()) {
                    path = &rest[dot + 1..];
                }
            }
        }
        let (id, key) = path
            .split_once('.')
            .ok_or_else(|| format!("路径应为 实体.属性: {path}"))?;
        let e = state
            .entities
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| {
                format!("实体不存在: {id} (entities 的键就是实体名, 直接写名字如 主人)")
            })?;
        let slot = e.props.entry(key.to_string()).or_insert(0.0);
        if is_assign {
            *slot = delta;
        } else {
            *slot += sign * delta;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Tool for SimulateTool {
    fn name(&self) -> &str {
        "simulate"
    }
    fn kind(&self) -> ToolKind {
        ToolKind::Sync
    }
    fn axes(&self) -> ToolAxes {
        ToolAxes::default()
    }
    async fn call(&self, args: Value) -> Result<Value, String> {
        let entities = args
            .get("entities")
            .and_then(|v| v.as_object())
            .ok_or_else(|| "entities 应为对象 {id: {属性: 值}}".to_string())?;
        let mut state = WorldState {
            entities: Vec::new(),
            tick: 0,
        };
        for (id, props) in entities {
            let mut e = Entity {
                id: id.clone(),
                name: id.clone(),
                props: std::collections::HashMap::new(),
            };
            if let Some(p) = props.as_object() {
                for (k, v) in p {
                    e.props.insert(k.clone(), v.as_f64().unwrap_or(0.0));
                }
            }
            state.entities.push(e);
        }
        let events: Vec<String> = args
            .get("events")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut eng = ScenarioEngine::new(state);
        let apply: crate::oracle::ApplyFn = Box::new(Self::apply);
        let snaps = eng.simulate(&events, &apply)?;
        let steps: Vec<Value> = snaps
            .iter()
            .map(|s| {
                let ents: serde_json::Map<String, Value> = s
                    .entities
                    .iter()
                    .map(|e| {
                        (
                            e.id.clone(),
                            serde_json::to_value(&e.props).unwrap_or(json!({})),
                        )
                    })
                    .collect();
                json!({"tick": s.tick, "entities": ents})
            })
            .collect();
        Ok(json!({"steps": steps, "final": steps.last().cloned().unwrap_or(json!(null))}))
    }
}

/// 「预测断言」工具 — oracle 套件: 登记可证伪预测 (待对照 resolve).
pub struct ForecastTool {
    registry: Arc<ForecastRegistry>,
}

impl ForecastTool {
    pub fn new(registry: Arc<ForecastRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl Tool for ForecastTool {
    fn name(&self) -> &str {
        "forecast"
    }
    fn kind(&self) -> ToolKind {
        ToolKind::Sync
    }
    fn axes(&self) -> ToolAxes {
        ToolAxes::default()
    }
    async fn call(&self, args: Value) -> Result<Value, String> {
        let statement = args
            .get("statement")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "statement 不能为空".to_string())?;
        let probability = args
            .get("probability")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let deadline_hours = args
            .get("deadline_hours")
            .and_then(|v| v.as_f64())
            .unwrap_or(24.0);
        let deadline_ms =
            chrono::Utc::now().timestamp_millis() + (deadline_hours * 3600_000.0) as i64;
        let f = Forecast::new(statement, probability, deadline_ms);
        self.registry.register(&f)?;
        Ok(json!({
            "id": f.id,
            "statement": f.statement,
            "probability": f.probability,
            "deadline_ms": f.deadline_ms,
            "note": "已登记可证伪预测, 到期后对照 resolve"
        }))
    }
}

/// 工具桥: 注册中心 + 洋葱门 + 审批 (黑/白/风险规则) + 执行器.
pub struct ToolBridge {
    pub registry: Arc<ToolRegistry>,
    executor: ToolExecutor,
    approval: ApprovalManager,
    pub gate: SecurityGate,
    pub sovereignty: SovereigntyGate,
    pub records: RecordStore,
    pub packs: PackRegistry,
    /// 宪法评审者 (真 LLM, 可选): 配置后 Medium+ 风险自动按原则判案.
    judicator: Option<Arc<dyn Judicator>>,
    /// 执行体隔离: worker 可执行文件路径 (None = 不隔离, 宿主内执行).
    worker: Option<PathBuf>,
    /// B3 沙盒参数 (内存/CPU/超时): 默认不限+30s; 套件清单/权限包可在运行时覆盖.
    sandbox: std::sync::Arc<std::sync::Mutex<crate::sandbox::SandboxConfig>>,
    /// 2026-08-20: Stage3 HardenedSandbox (NetIsolation + VMSandbox 双 Noop 默认).
    /// 真接入点: 高危工具执行前 arm_for_high_risk → net.apply + vm.start (0 装期双双 Err,
    /// 返 receipt boolean 表示加固结果, 不阻断主链路).
    /// 参考: crates/apeireth-companion/src/sandbox_integration.rs (commit 1288d617).
    hardened: Option<Arc<crate::sandbox_integration::HardenedSandbox>>,
    /// 结果溢出存储 (可选): 超大工具输出 spill 到会话私有文件, messages 只留定位.
    spill: Option<SpillStore>,
    /// post-execute 钩子链 (顺序执行, 审计前).
    post_hooks: Vec<Arc<dyn PostExecuteHook>>,
    /// 目标服务 (模块 6: with_goals 注入; None = 目标工具不注册).
    goals: Option<std::sync::Arc<std::sync::Mutex<crate::goal::GoalService>>>,
}

impl ToolBridge {
    /// 全量注册 (不吝啬授权, 安全机制守护) + 三层审批规则.
    pub fn new(store: Arc<SqliteMemoryStore>) -> Self {
        let records = RecordStore::new(store.clone());
        let registry = Arc::new(ToolRegistry::new());
        // 基地 4 真工具 (R17 战役: web_search / file_ops / git_ops / code_exec)
        if let Err(e) = apeireth_tools::register_all(&registry) {
            eprintln!("[bridge] register_all 部分失败: {e}");
        }
        // N17/TP2: 9 工具子 crate 统一装配 (§10 铁边界 ② Tool+ToolRegistry.register+ToolBridge 三件套)
        // 每个 register() 失败如实 eprintln 打点, 不阻断其余装配 (集成而非分立).
        for (label, res) in [
            (
                "EnhancedShell",
                apeireth_tool_shell::register::register(&registry),
            ),
            (
                "FetchEngine",
                apeireth_tool_fetch::register::register(&registry),
            ),
            (
                "EnhancedBrowser",
                apeireth_tool_browser::register::register(&registry),
            ),
            (
                "CodeIntelligence",
                apeireth_tool_codesearch::register::register(&registry),
            ),
            (
                "ImageGenEnhanced",
                apeireth_tool_image_gen::register::register(&registry),
            ),
            (
                "ImageProcess",
                apeireth_tool_image_process::register::register(&registry),
            ),
            (
                "VSearch",
                apeireth_tool_search::register::register(&registry),
            ),
            (
                "EnhancedFileOps",
                apeireth_tool_filesystem::register::register(&registry),
            ),
            (
                "RepoQualityAnalyzer",
                apeireth_repo_tools::register::register(&registry),
            ),
        ] {
            if let Err(e) = res {
                eprintln!("[bridge] N17 register `{label}` 部分失败: {e}");
            }
        }
        registry.register(
            "recall_memory".to_string(),
            Arc::new(RecallMemoryTool::new(Arc::clone(&store))),
        );
        registry.register(
            "save_memory".to_string(),
            Arc::new(SaveMemoryTool::new(Arc::clone(&store))),
        );
        registry.register(
            "propose_capability".to_string(),
            Arc::new(ProposeCapabilityTool::new(Arc::new(
                CapabilityRegistry::new(Arc::clone(&store), "me"),
            ))),
        );
        registry.register("simulate".to_string(), Arc::new(SimulateTool));
        registry.register(
            "forecast".to_string(),
            Arc::new(ForecastTool::new(Arc::new(ForecastRegistry::new(
                Arc::clone(&store),
                "me",
            )))),
        );
        registry.register(
            "audit_log".to_string(),
            Arc::new(crate::audit::AuditLogTool::new(Arc::clone(&store))),
        );
        // 自成长管道 (Level 0/1 经验库 + Level 2/3 原则晋级): 2026-08-16
        registry.register(
            "save_experience".to_string(),
            Arc::new(crate::experience::SaveExperienceTool::new(Arc::clone(
                &store,
            ))),
        );
        registry.register(
            "list_experience".to_string(),
            Arc::new(crate::experience::ListExperienceTool::new(Arc::clone(
                &store,
            ))),
        );
        registry.register(
            "verify_experience".to_string(),
            Arc::new(crate::experience::VerifyExperienceTool::new(Arc::clone(
                &store,
            ))),
        );
        registry.register(
            "propose_principle".to_string(),
            Arc::new(crate::principles::ProposePrincipleTool::new(Arc::clone(
                &store,
            ))),
        );
        registry.register(
            "approve_principle".to_string(),
            Arc::new(crate::principles::ApprovePrincipleTool::new(Arc::clone(
                &store,
            ))),
        );
        let executor = ToolExecutor::new(registry.clone());
        // 权限包: 默认日常包 (永久, 只读工具 + 记忆写; 主人可 grant 自定义包扩权)
        let packs = PackRegistry::new();
        packs.grant(PackRegistry::default_daily_pack());
        let approval = ApprovalManager::with_rules(vec![
            Box::new(BlacklistRule::with_blacklist(Vec::<String>::new(), false)),
            Box::new(WhitelistRule::with_whitelist([
                "recall_memory".to_string(),
                "save_memory".to_string(),
                "propose_capability".to_string(),
                "simulate".to_string(),
                "forecast".to_string(),
                "audit_log".to_string(),
                "save_experience".to_string(),
                "list_experience".to_string(),
                "verify_experience".to_string(),
                "propose_principle".to_string(),
                "approve_principle".to_string(),
                "goal_create".to_string(),
                "goal_status".to_string(),
                "goal_complete".to_string(),
                "goal_pause".to_string(),
                "goal_block".to_string(),
                // N17/TP2: 4 只读 N17 工具扩入日常包白名单 (网络/写盘/执行 5 件仍走 RiskRule 高危分级)
                "VSearch".to_string(),
                "CodeIntelligence".to_string(),
                "RepoQualityAnalyzer".to_string(),
                "ImageProcess".to_string(),
            ])),
            Box::new(RiskRule::with_categories(
                5 * 60 * 1000,
                [
                    "system".to_string(),
                    "network".to_string(),
                    "file".to_string(),
                    "shell".to_string(),
                    "exec".to_string(),
                    "patch".to_string(),
                    "task".to_string(),
                ],
            )),
        ]);
        Self {
            registry,
            executor,
            approval,
            gate: SecurityGate::default(),
            sovereignty: SovereigntyGate::default(),
            records,
            packs,
            judicator: None,
            worker: None,
            sandbox: std::sync::Arc::new(std::sync::Mutex::new(
                crate::sandbox::SandboxConfig::default(),
            )),
            hardened: None, // 2026-08-20: Stage3 HardenedSandbox (默认 None, 0 装 PASS: 不加固)
            spill: None,
            post_hooks: Vec::new(),
            goals: None,
        }
    }

    /// 接目标服务 (模块 6: 注册 goal_create/status/complete/pause/block 工具).
    /// 与注入侧共享同一实例 (serve: AppState.goal = 同一 Arc).
    pub fn with_goals(
        mut self,
        goals: std::sync::Arc<std::sync::Mutex<crate::goal::GoalService>>,
    ) -> Self {
        self.goals = Some(std::sync::Arc::clone(&goals));
        self.registry.register(
            "goal_create".to_string(),
            Arc::new(crate::goal_tools::GoalCreateTool::new(
                std::sync::Arc::clone(&goals),
            )),
        );
        self.registry.register(
            "goal_status".to_string(),
            Arc::new(crate::goal_tools::GoalStatusTool::new(
                std::sync::Arc::clone(&goals),
            )),
        );
        self.registry.register(
            "goal_complete".to_string(),
            Arc::new(crate::goal_tools::GoalCompleteTool::new(
                std::sync::Arc::clone(&goals),
            )),
        );
        self.registry.register(
            "goal_pause".to_string(),
            Arc::new(crate::goal_tools::GoalPauseTool::new(
                std::sync::Arc::clone(&goals),
            )),
        );
        self.registry.register(
            "goal_block".to_string(),
            Arc::new(crate::goal_tools::GoalBlockTool::new(
                std::sync::Arc::clone(&goals),
            )),
        );
        self
    }

    /// 接宪法评审者 (真 LLM): Medium+ 风险动作执行前自动按原则判案.
    /// BLOCK → sovereignty 记录 + 拒绝; 评审失败 → 保守拒绝 (0 装 PASS, 不放过).
    pub fn with_judicator(mut self, judge: Arc<dyn Judicator>) -> Self {
        self.judicator = Some(judge);
        self
    }

    /// 开启执行体隔离: MOVE 类工具 (文件/进程/代码等有副作用) 剥离到 per-call 子进程执行.
    /// `worker_bin` = `exec_worker` 可执行文件路径 (测试用 `env!("CARGO_BIN_EXE_exec_worker")`).
    /// 安全判断 (洋葱门/宪法评审/权限包/路径约束) 仍在宿主完成, 子进程只执行已批准操作.
    pub fn with_isolation(mut self, worker_bin: impl Into<PathBuf>) -> Self {
        self.worker = Some(worker_bin.into());
        self
    }

    /// B3 沙盒参数 (构造期): 隔离 worker 的内存/CPU/超时上限.
    /// 非法值请用 [`crate::sandbox::SandboxConfig::from_json`] 解析 (自动回退默认).
    pub fn with_sandbox_config(mut self, cfg: crate::sandbox::SandboxConfig) -> Self {
        self.set_sandbox_config(cfg);
        self
    }

    /// B3 沙盒参数 (运行时覆盖): 套件装配 (suites.rs) / 权限包路径调用.
    pub fn set_sandbox_config(&self, cfg: crate::sandbox::SandboxConfig) {
        if let Ok(mut g) = self.sandbox.lock() {
            *g = cfg;
        }
    }

    /// 2026-08-20: 挂 Stage3 HardenedSandbox (NetIsolation + VMSandbox).
    /// **0 装 PASS 严守**: 默认 `None` = 不加固 (backward compat 1:1).
    /// 显式挂载后, 高危工具 (shell / filesystem-write / code-search-replace) 执行前
    /// 自动走 `arm_for_high_risk()` → net.apply + vm.start (0 装期双双 Err).
    /// 加固失败不阻断 (per JobGuard 同款 "增强不是门" 语义).
    ///
    /// 示例:
    /// ```ignore
    /// use apeireth_companion::sandbox_integration::HardenedSandbox;
    /// let sandbox = HardenedSandbox::default(); // 双 Noop
    /// bridge.with_hardened_sandbox(Arc::new(sandbox));
    /// ```
    pub fn with_hardened_sandbox(
        mut self,
        sandbox: Arc<crate::sandbox_integration::HardenedSandbox>,
    ) -> Self {
        self.hardened = Some(sandbox);
        self
    }

    /// 当前桥级沙盒参数 (克隆读取).
    pub fn sandbox_config(&self) -> crate::sandbox::SandboxConfig {
        self.sandbox.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// 生效的沙盒参数: 权限包级覆盖优先, 否则桥级默认 (B3 参数口语义).
    fn effective_sandbox(&self, tool: &str) -> crate::sandbox::SandboxConfig {
        self.packs
            .sandbox_for(tool, chrono::Utc::now().timestamp_millis())
            .unwrap_or_else(|| self.sandbox_config())
    }

    /// 开启结果溢出: 工具输出超过阈值 → spill 到会话私有文件, messages 只留定位+提示.
    pub fn with_spill(mut self, spill: SpillStore) -> Self {
        self.spill = Some(spill);
        self
    }

    /// **TP29 (生态批)**: 从 YAML spec 文件注册工具占位 (Composio 借鉴).
    ///
    /// **纪律**:
    /// - 失败不破坏现有 API — 失败时返 `Err(String)`, 调用方 eprintln 处理.
    /// - 冲突不覆盖 — 同名已注册 → 返 `Err(NameConflict)`, 现有工具链不断.
    /// - 真实密码不入 yml — 沿用 TP33 纪律: 仅 `${VAR:?msg}` 形式, 由 `CredentialSpec::validate` 兜底.
    ///
    /// 真实实现挂接 (`implementation:` 字段) 后续任务做; 当前仅产"声明解析 + 占位 shim".
    pub fn register_yaml_spec<P: AsRef<std::path::Path>>(&self, path: P) -> Result<String, String> {
        let path_ref = path.as_ref();
        match apeireth_tools::register_yaml_spec(&self.registry, path_ref) {
            Ok(name) => {
                eprintln!(
                    "[bridge] TP29 yaml_spec registered: {name} ← {}",
                    path_ref.display()
                );
                Ok(name)
            }
            Err(e) => {
                eprintln!(
                    "[bridge] TP29 yaml_spec skipped ({}): {}",
                    path_ref.display(),
                    e
                );
                Err(e.to_string())
            }
        }
    }

    /// **TP29 (生态批)**: 批量注册目录下所有 `.yaml` / `.yml` 文件.
    ///
    /// 行为:
    /// - 每个文件独立尝试加载 + 注册; 任一失败 eprintln 但不阻断后续 (granular, 与
    ///   `load_yaml_spec_dir` 的 transactional 语义不同 — 桥接层优先保证部分可用).
    /// - 返成功注册的 spec 名称列表 (按文件名字典序).
    /// - 同名冲突 → 跳过, 不覆盖现有工具.
    pub fn register_yaml_spec_dir<P: AsRef<std::path::Path>>(&self, dir: P) -> Vec<String> {
        let dir_ref = dir.as_ref();
        let mut names = Vec::new();
        let read_dir = match std::fs::read_dir(dir_ref) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "[bridge] TP29 yaml_spec_dir read_dir 失败: {}: {e}",
                    dir_ref.display()
                );
                return names;
            }
        };
        let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            match self.register_yaml_spec(&path) {
                Ok(name) => names.push(name),
                Err(_) => {
                    // register_yaml_spec 内部已 eprintln 详细原因; 这里保持静默即可.
                }
            }
        }
        names
    }

    /// 注册 post-execute 钩子 (结果产出后、审计前执行; 可替换/拦截).
    pub fn with_post_hook(mut self, hook: Arc<dyn PostExecuteHook>) -> Self {
        self.post_hooks.push(hook);
        self
    }

    /// **TP22 (E1+W5, 核心)**: 注册 Observer 捕获钩子 — 工具执行结果即时沉淀候选.
    ///
    /// 与 `with_post_hook` 等价, 但接受 `Arc<ExperienceQueue>` 而非裸 hook:
    /// 内部实例化 `ObserverCaptureHook`, 复用同一条 post_hook 链.
    /// 顺序: observer hook **插在链尾** (最后执行, 在所有用户 hook 之后, 确保
    /// 拿到的是「最终态」ExecutionResult, 不是被中间 hook 替换前的中间值).
    pub fn with_observer_capture(
        mut self,
        queue: Arc<crate::observer_capture::ExperienceQueue>,
    ) -> Self {
        self.post_hooks
            .push(Arc::new(crate::observer_capture::ObserverCaptureHook::new(
                queue,
            )));
        self
    }

    /// 当前注册的 post-hook 数 (测试/调试用).
    pub fn post_hooks_len(&self) -> usize {
        self.post_hooks.len()
    }

    /// 工具风险映射 (对齐基地 8 工具真名): ShellExec → High;
    /// FileOperator/ApplyPatch/LongTask → Medium; WebSearch/Grep/Git/WebFetch/recall_memory → Low.
    pub fn tool_risk(tool: &str) -> RiskLevel {
        let t = tool.to_lowercase();
        if t.contains("exec") || t.contains("shell") {
            RiskLevel::High
        } else if t.contains("file") || t.contains("patch") || t.contains("task") {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }

    /// 主权总闸 → 洋葱门 → 审批 → 执行.
    pub async fn execute_if_allowed(&self, call: &ParsedToolCall) -> ExecutionResult {
        if self.sovereignty.is_frozen() {
            return ExecutionResult {
                tool_name: call.tool_name.clone(),
                success: false,
                output: json!(null),
                error: Some("主权熔断: 循环已冻结".to_string()),
                duration_ms: 0,

                ..Default::default()
            };
        }
        let verdict = self.gate.check(
            "tool_call",
            &format!("调用工具 {}", call.tool_name),
            Self::tool_risk(&call.tool_name),
            ActionTarget::NormalAction(format!("tool:{}", call.tool_name)),
        );
        if !matches!(verdict, ActionVerdict::Allow) {
            let err = format!("洋葱门拦下: {:?}", verdict);
            return ExecutionResult {
                tool_name: call.tool_name.clone(),
                success: false,
                output: json!(null),
                error: Some(err),
                duration_ms: 0,

                ..Default::default()
            };
        }
        // 结构化宪法门 (零成本硬门, 全部风险级别; 描述由系统侧生成, 调用方不可伪造):
        // 命中编译期规则 (E-4/E-6/PHL 等) → 直接拒绝 + sovereignty 记录.
        let desc = format!("调用工具 {} 参数 {}", call.tool_name, call.args);
        if let Some((key, why)) = ConstitutionGate::check(&desc) {
            self.sovereignty.report_violation(key, &call.tool_name);
            return ExecutionResult {
                tool_name: call.tool_name.clone(),
                success: false,
                output: json!(null),
                error: Some(format!("宪法硬门拦截 ({key}): {why}")),
                duration_ms: 0,

                ..Default::default()
            };
        }
        // 动态原则层 (自成长 Level 2, 洋葱外层运行时规则): 命中 active 原则 → 拦截 + 记违反.
        // 原则由 AI 提案 + 主人 master token 批准; 语义 = 前缀匹配 (对齐 ConstitutionGate).
        let dynamic_rules =
            crate::principles::PrincipleStore::new(Arc::clone(self.records.store()));
        let rules = dynamic_rules.active_rules();
        if let Some((pid, stmt)) = crate::principles::PrincipleStore::check_dynamic(&desc, &rules) {
            dynamic_rules.record_violation(&pid);
            self.sovereignty
                .report_violation("动态原则拦截", &call.tool_name);
            return ExecutionResult {
                tool_name: call.tool_name.clone(),
                success: false,
                output: json!(null),
                error: Some(format!("动态原则拦截 ({pid}): {stmt}")),
                duration_ms: 0,

                ..Default::default()
            };
        }
        // 宪法评审 (真 LLM, 按原则判案): Medium+ 风险且配置了评审者 → 自动评审.
        // 只审动作摘要 (action + tool + args), 不审对话/记忆自由文本.
        if requires_llm_review(Self::tool_risk(&call.tool_name)) {
            if let Some(judge) = &self.judicator {
                match judge.judge(&desc).await {
                    Ok(true) => {}
                    Ok(false) => {
                        self.sovereignty
                            .report_violation("宪法评审拦截", &call.tool_name);
                        return ExecutionResult {
                            tool_name: call.tool_name.clone(),
                            success: false,
                            output: json!(null),
                            error: Some("BLOCK: 宪法评审拒绝 (按原则判案, 非关键词)".to_string()),
                            duration_ms: 0,

                            ..Default::default()
                        };
                    }
                    Err(e) => {
                        // 评审失败 → 保守拒绝 (不放过未审动作)
                        return ExecutionResult {
                            tool_name: call.tool_name.clone(),
                            success: false,
                            output: json!(null),
                            error: Some(format!("宪法评审失败, 保守拒绝: {e}")),
                            duration_ms: 0,

                            ..Default::default()
                        };
                    }
                }
            }
        }
        // 权限包检查: 被活跃包覆盖 → 免现场审批直接执行 (责任自负 + 监督兜底)
        let pack_authorized = self
            .packs
            .check_and_consume(&call.tool_name, chrono::Utc::now().timestamp_millis());
        // 执行级路径校验: 权限包 paths 约束 (FileOperator 等文件类工具, 防越权写盘 / `..` 穿越)
        if pack_authorized {
            if let Some(paths) = self
                .packs
                .paths_for(&call.tool_name, chrono::Utc::now().timestamp_millis())
            {
                if let Some(p) = call
                    .args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    if !paths.iter().any(|base| path_within(p, base)) {
                        return ExecutionResult {
                            tool_name: call.tool_name.clone(),
                            success: false,
                            output: json!(null),
                            error: Some(format!(
                                "权限包路径约束拒绝: {p} 不在获准路径 [{}] 内",
                                paths.join(", ")
                            )),
                            duration_ms: 0,

                            ..Default::default()
                        };
                    }
                }
            }
        }
        let r = if pack_authorized {
            self.run_executor(call).await
        } else {
            match self.approval.check(call) {
                ApprovalDecision::Allow => self.run_executor(call).await,
                ApprovalDecision::RequireApproval { .. } => {
                    // 授权请求机制 (2026-08-16): 被拒时产生一条「待主人批准」请求,
                    // 前端轮询展示, 主人一键批准 (权限洋葱的真实载体, 防 AI 虚构交互流程).
                    crate::approval_requests::record_request(
                        self.records.store(),
                        &call.tool_name,
                        &call.args,
                        "需要主人批准 (权限洋葱)",
                        None, // TP20-N20: ToolBridge 暂不持 bridge, 后续 wire 可注入
                    );
                    return ExecutionResult {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: json!(null),
                        error: Some("该工具是高风险操作且未被权限包覆盖, 需要主人批准 (已向主人发出授权请求)".to_string()),
                        duration_ms: 0,
                        ..Default::default()
                    };
                }
                _ => {
                    return ExecutionResult {
                        tool_name: call.tool_name.clone(),
                        success: false,
                        output: json!(null),
                        error: Some("审批拒绝".to_string()),
                        duration_ms: 0,

                        ..Default::default()
                    }
                }
            }
        };
        // 结果溢出: 超大输出 spill 到会话私有文件, messages 只留定位 (防撑爆上下文)
        let r = if let Some(spill) = &self.spill {
            if r.success {
                let ser = serde_json::to_string(&r.output).unwrap_or_default();
                if ser.chars().count() > SPILL_THRESHOLD_CHARS {
                    match spill.spill("me", "tool_result.txt", &ser) {
                        Ok(path) => ExecutionResult {
                            tool_name: r.tool_name.clone(),
                            success: true,
                            output: json!({
                                "spilled": true,
                                "path": path,
                                "bytes": ser.len(),
                                "hint": "结果过大已溢出到会话私有文件; 需要时用 FileOperator(op=read) 读取"
                            }),
                            error: None,
                            duration_ms: r.duration_ms,

                            ..Default::default()
                        },
                        Err(e) => {
                            eprintln!("[spill] 溢出失败: {e}");
                            r
                        }
                    }
                } else {
                    r
                }
            } else {
                r
            }
        } else {
            r
        };
        // post-execute 钩子链 (结果产出后、审计前; 可替换/拦截)
        let mut r = r;
        for h in &self.post_hooks {
            r = h.apply(call, &r);
        }
        // **TP12 (A2, P0) 结构化回灌**: 把 guardrail_error / validation_error / tripwire 包装进
        // r.output 的 `_tp12_report` 子字段. 这样:
        // 1. 模型在 tool message 里看到结构化 hint (path/expected/hint), 可自修正后重试
        // 2. 审计 record 也能拿到完整结构 (与 record_execution 的 tp12_report 字段一致)
        r.output = inject_tp12_into_output(&r);
        // 监督机制: 每次工具调用 append-only 记录 (含结果, 出站隐私脱敏后存)
        let serialized = serde_json::to_string(&r.output).unwrap_or_default();
        let pii = apeireth_guard::detect_pii(&serialized);
        let masked_output = if pii.is_empty() {
            r.output.clone()
        } else {
            serde_json::Value::String(apeireth_guard::redact_text(
                &serialized,
                &pii,
                apeireth_guard::RedactionStrategy::Mask,
            ))
        };
        // **TP12**: 改用 record_execution 而非 record(), 让 audit payload 自动带上
        // _tp12_report 结构 (guardrail/validation/tripwire). 行为向后兼容:
        // 干净调用时 record_execution 也不会在 payload 里塞 tp12_report 字段.
        let mut r_for_record = r.clone();
        r_for_record.output = masked_output;
        let _ = self
            .records
            .record_execution(call, &r_for_record, !pii.is_empty())
            .await;
        r
    }

    /// 执行器入口: 隔离模式 + MOVE 工具 → per-call 子进程; 否则宿主执行器.
    async fn run_executor(&self, call: &ParsedToolCall) -> ExecutionResult {
        if let Some(worker) = &self.worker {
            if crate::exec_worker::should_isolate(&call.tool_name) {
                let cfg = self.effective_sandbox(&call.tool_name);
                return self.execute_isolated(worker, call, &cfg).await;
            }
        }
        self.executor.execute(call).await
    }

    /// per-call 子进程执行: 一行 JSON 请求 → 一行响应, 超时 kill (B3: 可配).
    ///
    /// 沙盒语义 (B3): Job Object 按 [`crate::sandbox::SandboxConfig`] 设内存/CPU
    /// 限额; 超限 → 系统终止 worker, guard 留痕并翻译成明确错误 (不静默);
    /// 加固失败不阻断执行 (如实 eprintln 记录).
    async fn execute_isolated(
        &self,
        worker: &PathBuf,
        call: &ParsedToolCall,
        cfg: &crate::sandbox::SandboxConfig,
    ) -> ExecutionResult {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let start = std::time::Instant::now();
        let err_res = |msg: String, start: std::time::Instant| ExecutionResult {
            tool_name: call.tool_name.clone(),
            success: false,
            output: json!(null),
            error: Some(msg),
            duration_ms: start.elapsed().as_millis() as u64,

            ..Default::default()
        };
        // 2026-08-20: Stage3 HardenedSandbox arm_for_high_risk 真接入点.
        // NetIsolation + VMSandbox 0 装期双双 Err → receipt 双 false → 不阻断 (per JobGuard 同款).
        // 真接 NetIsolation/VMSandbox 后, 高危工具自动获网络/VM 加固.
        if let Some(hardened) = &self.hardened {
            let tool_static: &'static str = Box::leak(call.tool_name.clone().into_boxed_str());
            let net_cfg = crate::sandbox_net::NetworkIsolationConfig {
                level: crate::sandbox_net::NetworkIsolationLevel::LoopbackOnly,
                outbound_whitelist: Vec::new(),
                allow_inbound: false,
                allow_dns: false,
            };
            let vm_cfg = crate::vm_sandbox::VMSandboxConfig {
                vcpus: 1,
                memory_mb: 256,
                rootfs: None,
                kernel: None,
                initrd: None,
                network: None,
                boot_timeout_secs: 60,
            };
            let receipt = hardened.arm_for_high_risk(tool_static, &net_cfg, &vm_cfg);
            eprintln!(
                "[hardened-sandbox] arm_for_high_risk(\"{}\"): net={} vm={}",
                call.tool_name, receipt.net, receipt.vm
            );
        }
        let mut child = match tokio::process::Command::new(worker)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return err_res(format!("worker spawn 失败: {e}"), start),
        };
        // P3#16 microsandbox 加固 + B3 限额: Windows Job Object (KILL_ON_JOB_CLOSE —
        // 宿主退出/崩溃 → 进程树终止, 防孤儿; 内存/CPU 限额超限 → 系统终止 + 留痕).
        // ⚠️ guard 必须持有到 worker 结束: 句柄提前关闭会触发 KILL_ON_JOB_CLOSE.
        let guard = match crate::job_object::JobGuard::with_config(cfg) {
            Ok(g) => Some(g),
            Err(e) => {
                eprintln!("[sandbox] Job Object 加固失败 (不阻断): {e}");
                None
            }
        };
        if let Some(pid) = child.id() {
            if let Some(g) = &guard {
                if let Err(e) = g.assign(pid) {
                    eprintln!("[sandbox] Job Object assign({pid}) 失败 (不阻断): {e}");
                }
            }
        }
        // 超限留痕 → 明确错误 (把"worker 提前退出"翻译成具体限额原因, 不静默)
        let violation_msg = |g: &Option<crate::job_object::JobGuard>, base: &str| match g
            .as_ref()
            .and_then(|x| x.violation())
        {
            Some(v) => format!("{base}: 沙盒资源限额终止 — {v}"),
            None => base.to_string(),
        };
        let Some(mut stdin) = child.stdin.take() else {
            return err_res("worker stdin 不可用".into(), start);
        };
        let req = format!("{}\n", json!({"tool": call.tool_name, "args": call.args}));
        if let Err(e) = stdin.write_all(req.as_bytes()).await {
            let _ = child.kill().await;
            return err_res(format!("写 worker 请求失败: {e}"), start);
        }
        drop(stdin);
        let Some(stdout) = child.stdout.take() else {
            return err_res("worker stdout 不可用".into(), start);
        };
        let line = match tokio::time::timeout(Duration::from_secs(cfg.timeout_secs), async {
            let mut r = tokio::io::BufReader::new(stdout);
            r.lines().next_line().await
        })
        .await
        {
            Ok(Ok(Some(l))) => l,
            Ok(Ok(None)) => {
                let _ = child.wait().await;
                return err_res(violation_msg(&guard, "worker 无响应 (提前退出)"), start);
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                return err_res(format!("读 worker 响应失败: {e}"), start);
            }
            Err(_) => {
                let _ = child.kill().await;
                return err_res(
                    violation_msg(
                        &guard,
                        &format!("worker 超时 ({}s), 已 kill", cfg.timeout_secs),
                    ),
                    start,
                );
            }
        };
        let _ = child.wait().await;
        let resp: serde_json::Value = serde_json::from_str(&line)
            .unwrap_or(json!({"ok": false, "error": format!("worker 响应非法: {line}")}));
        let dur = start.elapsed().as_millis() as u64;
        if resp["ok"] == json!(true) {
            ExecutionResult {
                tool_name: call.tool_name.clone(),
                success: true,
                output: resp["output"].clone(),
                error: None,
                duration_ms: dur,

                ..Default::default()
            }
        } else {
            ExecutionResult {
                tool_name: call.tool_name.clone(),
                success: false,
                output: json!(null),
                error: resp
                    .get("error")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| Some("worker 返回失败".to_string())),
                duration_ms: dur,

                ..Default::default()
            }
        }
    }

    /// 给住客 AI 的工具调用格式说明 (注入 LLM system prompt).
    pub fn tool_format_instruction() -> String {
        "如果你需要调用基地工具 (比如回忆用户的记忆), 在回复中输出:\n<<<[TOOL_REQUEST]>>>\ntool_name:<<<recall_memory>>>\nquery:<<<关键词>>>\n<<<[END_TOOL_REQUEST]>>>\n收到工具结果后, 再继续用自然语言回复。高危工具 (执行代码等) 需要主人批准, 你不能自主执行。"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apeireth_memory::CoreEpisode;

    #[tokio::test]
    async fn recall_tool_searches_seeded_memory() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        store
            .put_episode(&CoreEpisode {
                id: "e1".into(),
                timestamp: 1,
                role: "assistant".into(),
                content: "线性代数: 矩阵的秩的作业".into(),
                session_id: "s1".into(),
            })
            .unwrap();
        let bridge = ToolBridge::new(store);
        let call = ParsedToolCall {
            tool_name: "recall_memory".into(),
            args: json!({"query": "线性代数"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "err = {:?}", r.error);
        assert!(r.output["found"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn all_base_tools_registered() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let names = bridge.registry.list();
        assert!(names.iter().any(|n| n == "recall_memory"));
        assert!(names.iter().any(|n| n == "save_memory"));
        assert!(names.iter().any(|n| n == "propose_capability"));
        assert!(
            names.len() >= 7,
            "应含 4 真工具 + recall/save/propose, 实际 {}: {:?}",
            names.len(),
            names
        );
    }

    #[tokio::test]
    async fn propose_capability_tool_registers_proposal() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(Arc::clone(&store));
        let call = ParsedToolCall {
            tool_name: "propose_capability".into(),
            args: json!({"name": "换元检查", "description": "做换元法时自动提醒检查 dx", "kind": "skill"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "提案应成功: {:?}", r.error);
        assert_eq!(r.output["status"], json!("pending"));
        // 提案已登记 (pending), 未激活
        use crate::capability::CapabilityStatus;
        let reg = crate::capability::CapabilityRegistry::new(store, "me");
        let list = reg.list(Some(CapabilityStatus::Pending)).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "换元检查");
    }

    #[tokio::test]
    async fn save_memory_then_recall_finds_it() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(Arc::clone(&store));
        let call = ParsedToolCall {
            tool_name: "save_memory".into(),
            args: json!({"content": "AI 自己总结: 主人明天要交线代作业, 矩阵的秩那节还没做完", "session_id": "me"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "err = {:?}", r.error);
        assert_eq!(r.output["ok"], json!(true));
        // 写进去的能被 recall 捞到 (append-only 真库)
        let eps = store.recent_episodes("me", 10).unwrap();
        assert_eq!(eps.len(), 1);
        assert!(eps[0].content.contains("线代作业"));
        // 空 content 被拒
        let bad = ParsedToolCall {
            tool_name: "save_memory".into(),
            args: json!({"content": ""}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&bad).await;
        assert!(!r.success);
    }

    #[tokio::test]
    async fn pack_path_constraint_blocks_outside_write() {
        use crate::packs::PermissionPack;
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let workdir =
            std::env::temp_dir().join(format!("apeireth-path-test-{}", std::process::id()));
        std::fs::create_dir_all(&workdir).unwrap();
        bridge.packs.grant(
            PermissionPack::timed("路径测试", vec!["FileOperator".to_string()], 1, Some(10))
                .with_paths(vec![workdir.to_string_lossy().to_string()]),
        );
        let mk = |path: String| ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "write", "path": path, "content": "x"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        // 包内写 → 允许
        let ok = mk(workdir.join("ok.txt").to_string_lossy().to_string());
        let r = bridge.execute_if_allowed(&ok).await;
        assert!(r.success, "包内写应成功: {:?}", r.error);
        // 包外写 → 拦 (执行级路径约束)
        let outside = std::env::temp_dir().join("apeireth-outside-test.txt");
        let bad = mk(outside.to_string_lossy().to_string());
        let r = bridge.execute_if_allowed(&bad).await;
        assert!(!r.success, "包外写应被拦");
        assert!(
            r.error.as_deref().unwrap_or("").contains("路径约束"),
            "err={:?}",
            r.error
        );
        // `..` 穿越 → 拦 (canonicalize 解析后落在包外)
        let escape = workdir.join("..").join("escape.txt");
        let bad2 = mk(escape.to_string_lossy().to_string());
        let r = bridge.execute_if_allowed(&bad2).await;
        assert!(!r.success, "`..` 穿越应被拦: {:?}", r.error);
        let _ = std::fs::remove_dir_all(&workdir);
    }

    #[tokio::test]
    async fn constitution_hard_gate_blocks_before_llm() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        // 无 LLM 评审配置, 纯硬门也应拦截 (零成本层)
        let call = ParsedToolCall {
            tool_name: "ShellExec".into(),
            args: json!({"command": "复制自己到另一台主机"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(!r.success, "硬门应拦截自我复制");
        assert!(
            r.error.as_deref().unwrap_or("").contains("宪法硬门"),
            "err={:?}",
            r.error
        );
    }

    #[tokio::test]
    async fn constitution_judicator_blocks_medium_risk() {
        use crate::daemon::Judicator;
        struct BlockAll;
        #[async_trait::async_trait]
        impl Judicator for BlockAll {
            async fn judge(&self, _a: &str) -> Result<bool, String> {
                Ok(false)
            }
        }
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store).with_judicator(Arc::new(BlockAll));
        // FileOperator (Medium) → 宪法评审 BLOCK → 拒绝
        let call = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "read", "path": "C:/x"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(!r.success, "评审 BLOCK 应拒绝");
        assert!(
            r.error.as_deref().unwrap_or("").contains("宪法评审"),
            "err={:?}",
            r.error
        );
        // sovereignty 已记录 violation (熔断演示: 越界触碰)
        assert!(bridge.sovereignty.is_frozen(), "BLOCK 后应触发主权熔断");
    }

    #[tokio::test]
    async fn constitution_judicator_allows_when_judge_approves() {
        use crate::daemon::Judicator;
        use crate::packs::PermissionPack;
        struct AllowAll;
        #[async_trait::async_trait]
        impl Judicator for AllowAll {
            async fn judge(&self, _a: &str) -> Result<bool, String> {
                Ok(true)
            }
        }
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store).with_judicator(Arc::new(AllowAll));
        bridge.packs.grant(PermissionPack::timed(
            "评审测试包",
            vec!["FileOperator".to_string()],
            1,
            Some(5),
        ));
        let call = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "write", "path": std::env::temp_dir().join("apeireth-judge-allow.txt").to_string_lossy().to_string(), "content": "x"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "评审 ALLOW + 包覆盖应放行: {:?}", r.error);
        let _ = std::fs::remove_file(std::env::temp_dir().join("apeireth-judge-allow.txt"));
    }

    #[tokio::test]
    async fn constitution_judicator_failure_is_conservative() {
        use crate::daemon::Judicator;
        struct ErrJudge;
        #[async_trait::async_trait]
        impl Judicator for ErrJudge {
            async fn judge(&self, _a: &str) -> Result<bool, String> {
                Err("MiniMax suppressed".into())
            }
        }
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store).with_judicator(Arc::new(ErrJudge));
        let call = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "read", "path": "C:/x"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(!r.success, "评审失败应保守拒绝");
        assert!(
            r.error.as_deref().unwrap_or("").contains("保守拒绝"),
            "err={:?}",
            r.error
        );
    }

    #[tokio::test]
    async fn oversized_tool_result_spills_to_private_file() {
        let spill_root =
            std::env::temp_dir().join(format!("apeireth-spill-bridge-{}", std::process::id()));
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store).with_spill(SpillStore::with_root(&spill_root));
        let dir = std::env::temp_dir().join(format!("apeireth-spill-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = "y".repeat(3000);
        std::fs::write(dir.join("big.txt"), &big).unwrap();
        bridge.packs.grant(
            crate::packs::PermissionPack::timed(
                "溢出测试",
                vec!["FileOperator".to_string()],
                1,
                Some(5),
            )
            .with_paths(vec![dir.to_string_lossy().to_string()]),
        );
        let call = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "read", "path": dir.join("big.txt").to_string_lossy().to_string()}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "read 应成功: {:?}", r.error);
        assert_eq!(
            r.output["spilled"],
            json!(true),
            "超大结果应溢出: {}",
            r.output
        );
        let path = r.output["path"].as_str().unwrap().to_string();
        let read_back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            read_back.matches('y').count(),
            3000,
            "溢出文件应含完整 3000 字符内容"
        );
        // 小结果不溢出
        let small_file = dir.join("small.txt");
        std::fs::write(&small_file, "ok").unwrap();
        let call2 = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "read", "path": small_file.to_string_lossy().to_string()}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r2 = bridge.execute_if_allowed(&call2).await;
        assert_eq!(r2.output["spilled"], json!(null), "小结果不应溢出");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&spill_root);
    }

    #[tokio::test]
    async fn post_execute_hook_can_replace_or_block_result() {
        use crate::packs::PermissionPack;
        // 替换钩子: 把成功结果包一层 "via_hook"
        struct WrapHook;
        impl PostExecuteHook for WrapHook {
            fn apply(&self, _call: &ParsedToolCall, r: &ExecutionResult) -> ExecutionResult {
                if r.success {
                    ExecutionResult {
                        tool_name: r.tool_name.clone(),
                        success: true,
                        output: json!({"via_hook": true, "inner": r.output}),
                        error: None,
                        duration_ms: r.duration_ms,

                        ..Default::default()
                    }
                } else {
                    r.clone()
                }
            }
        }
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store).with_post_hook(Arc::new(WrapHook));
        let dir = std::env::temp_dir().join(format!("apeireth-hook-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("ok.txt");
        bridge.packs.grant(
            PermissionPack::timed("钩子测试", vec!["FileOperator".to_string()], 1, Some(5))
                .with_paths(vec![dir.to_string_lossy().to_string()]),
        );
        let call = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "write", "path": target.to_string_lossy().to_string(), "content": "x"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "钩子不应拦截成功: {:?}", r.error);
        assert_eq!(
            r.output["via_hook"],
            json!(true),
            "post 钩子应替换结果: {}",
            r.output
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn post_execute_hook_can_block() {
        use crate::daemon::Judicator;
        use crate::packs::PermissionPack;
        struct AllowAll;
        #[async_trait::async_trait]
        impl Judicator for AllowAll {
            async fn judge(&self, _a: &str) -> Result<bool, String> {
                Ok(true)
            }
        }
        struct BlockHook;
        impl PostExecuteHook for BlockHook {
            fn apply(&self, _call: &ParsedToolCall, r: &ExecutionResult) -> ExecutionResult {
                ExecutionResult {
                    tool_name: r.tool_name.clone(),
                    success: false,
                    output: json!(null),
                    error: Some("post 拦截: 结果不符合出站策略".to_string()),
                    duration_ms: r.duration_ms,

                    ..Default::default()
                }
            }
        }
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store)
            .with_judicator(Arc::new(AllowAll))
            .with_post_hook(Arc::new(BlockHook));
        let dir = std::env::temp_dir().join(format!("apeireth-hook-block-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        bridge.packs.grant(
            PermissionPack::timed("钩子拦截", vec!["FileOperator".to_string()], 1, Some(5))
                .with_paths(vec![dir.to_string_lossy().to_string()]),
        );
        let call = ParsedToolCall {
            tool_name: "FileOperator".into(),
            args: json!({"op": "write", "path": dir.join("x.txt").to_string_lossy().to_string(), "content": "x"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(!r.success, "post 钩子应能拦截");
        assert!(r.error.as_deref().unwrap_or("").contains("post 拦截"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn high_risk_tool_requires_approval() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let call = ParsedToolCall {
            tool_name: "ShellExec".into(),
            args: json!({"command": "echo hi"}),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("主人批准"));
    }

    #[test]
    fn simulate_apply_lenient_grammar_matches_model_writing() {
        use crate::oracle::Entity;
        use std::collections::HashMap;
        let mut s = WorldState {
            entities: vec![
                Entity {
                    id: "主人".into(),
                    name: "主人".into(),
                    props: HashMap::from([
                        ("复习进度".into(), 0.3f64),
                        ("焦虑".into(), 0.6f64),
                        ("剩余时间h".into(), 48f64),
                    ]),
                },
                Entity {
                    id: "错题本A".into(),
                    name: "错题本A".into(),
                    props: HashMap::from([("收录数".into(), 3f64)]),
                },
            ],
            tick: 0,
        };
        // 标准 + / -
        SimulateTool::apply(&mut s, "主人.复习进度+0.2").unwrap();
        assert!((s.prop("主人", "复习进度").unwrap() - 0.5).abs() < 1e-9);
        SimulateTool::apply(&mut s, "主人.焦虑-0.1").unwrap();
        assert!((s.prop("主人", "焦虑").unwrap() - 0.5).abs() < 1e-9);
        SimulateTool::apply(&mut s, "主人.剩余时间h-24").unwrap();
        assert!(
            (s.prop("主人", "剩余时间h").unwrap() - 24.0).abs() < 1e-9,
            "48-24"
        );
        // 等号 = 赋值 (验收实况: "delta 非法: =24" — 模型用 = 表示设定)
        SimulateTool::apply(&mut s, "主人.剩余时间h=48").unwrap();
        assert!(
            (s.prop("主人", "剩余时间h").unwrap() - 48.0).abs() < 1e-9,
            "赋值=48"
        );
        SimulateTool::apply(&mut s, "错题本A.收录数=8").unwrap();
        assert!(
            (s.prop("错题本A", "收录数").unwrap() - 8.0).abs() < 1e-9,
            "赋值=8"
        );
        // 编号前缀 "e1." 剥离 (验收实况: "实体不存在: e1")
        SimulateTool::apply(&mut s, "e1.主人.信心=0.5").unwrap();
        assert!((s.prop("主人", "信心").unwrap() - 0.5).abs() < 1e-9);
        // 坏格式仍报错 (带示例), 未知实体报错带指引
        assert!(SimulateTool::apply(&mut s, "主人 信心 48").is_err());
        assert!(SimulateTool::apply(&mut s, "幽灵.复习进度+0.1").is_err());
    }

    #[tokio::test]
    async fn simulate_tool_bridge_accepts_lenient_events() {
        // 桥全链路: simulate 工具调用接受宽容语法 (验收实况复现)
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let call = ParsedToolCall {
            tool_name: "simulate".into(),
            args: json!({
                "entities": {"主人": {"复习进度": 0.3, "信心": 0.2}},
                "events": ["e1.主人.复习进度+0.3", "主人.信心=0.4", "主人.复习进度-0.1"]
            }),
            raw_marker: String::new(),
            archery: false,
            archery_no_reply: false,
        };
        let r = bridge.execute_if_allowed(&call).await;
        assert!(r.success, "宽容语法应全通过: {:?}", r.error);
        let steps = r.output["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);
        let fin = &r.output["final"]["entities"]["主人"];
        assert!(
            (fin["复习进度"].as_f64().unwrap() - 0.5).abs() < 1e-9,
            "0.3+0.3-0.1"
        );
        assert!(
            (fin["信心"].as_f64().unwrap() - 0.4).abs() < 1e-9,
            "=0.4 赋值"
        );
    }

    // ---- B3 沙盒包参数化 ----

    #[test]
    fn sandbox_config_invalid_falls_back_not_blocking() {
        // 参数非法 → 回退默认 (0 阻断); 桥级可正常持有
        let cfg = crate::sandbox::SandboxConfig::from_json(&serde_json::json!({
            "memory_limit_mb": -1, "cpu_percent": 999, "timeout_secs": 0
        }));
        assert_eq!(cfg, crate::sandbox::SandboxConfig::default());
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store).with_sandbox_config(cfg);
        assert_eq!(
            bridge.sandbox_config(),
            crate::sandbox::SandboxConfig::default()
        );
        // 运行时覆盖同样可用
        bridge.set_sandbox_config(crate::sandbox::SandboxConfig {
            timeout_secs: 90,
            ..crate::sandbox::SandboxConfig::default()
        });
        assert_eq!(bridge.sandbox_config().timeout_secs, 90);
    }

    // ──────────────────────────────────────────────────────────────────
    // 2026-08-20: Stage3 HardenedSandbox 真接入 (NetIsolation + VMSandbox)
    // 0 装 PASS: 默认 hardened = None, builder 链等价 1:1
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn hardened_sandbox_default_none_backward_compatible() {
        // 0 装 PASS: 不挂 HardenedSandbox 时, 桥级 1:1 行为 (旧版完全兼容)
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        // 桥级 hardened 字段不可访问 (pub struct), 但执行器路径不会调 arm_for_high_risk
        // 验证: bridge.sandbox_config() 仍可用, 不挂 hardened 1:1 行为
        assert_eq!(bridge.sandbox_config().timeout_secs, 30); // default
    }

    #[test]
    fn hardened_sandbox_with_builder_accepted() {
        // 0 装 PASS: 挂 HardenedSandbox (默认双 Noop) 时, 桥级链不 panic
        use crate::sandbox_integration::HardenedSandbox;
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge =
            ToolBridge::new(store).with_hardened_sandbox(Arc::new(HardenedSandbox::default()));
        // 验证桥级创建后仍可访问 sandbox_config (与 hardened 平行)
        assert_eq!(bridge.sandbox_config().timeout_secs, 30);
    }

    #[test]
    fn hardened_sandbox_arm_noop_does_not_panic() {
        // 0 装 PASS: Noop NetworkIsolation + NoopVMSandbox 双双 Err → receipt 双 false + 不 panic
        use crate::sandbox_integration::HardenedSandbox;
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let hardened = HardenedSandbox::default();
        let net_cfg = crate::sandbox_net::NetworkIsolationConfig {
            level: crate::sandbox_net::NetworkIsolationLevel::LoopbackOnly,
            outbound_whitelist: Vec::new(),
            allow_inbound: false,
            allow_dns: false,
        };
        let vm_cfg = crate::vm_sandbox::VMSandboxConfig {
            vcpus: 1,
            memory_mb: 256,
            rootfs: None,
            kernel: None,
            initrd: None,
            network: None,
            boot_timeout_secs: 60,
        };
        let receipt = hardened.arm_for_high_risk("shell", &net_cfg, &vm_cfg);
        // 0 装期: receipt 双 false (加固失败, 不假装)
        assert!(!receipt.net, "Noop NetworkIsolation → receipt.net = false");
        assert!(!receipt.vm, "Noop VMSandbox → receipt.vm = false");
        assert_eq!(receipt.tool, "shell");
        // 桥级挂了 hardened 也能 1:1 创建
        let _bridge = ToolBridge::new(store).with_hardened_sandbox(Arc::new(hardened));
    }

    // 带真 worker 的沙盒限额 e2e 在 tests/exec_worker_isolation.rs
    // (CARGO_BIN_EXE_exec_worker 仅集成测试可用).

    // ===== N17/TP2: 9 工具子 crate 装配端到端验收 =====

    #[tokio::test]
    async fn n17_tool_bridge_registers_all_nine_and_catalog_reflects() {
        use apeireth_tool_registry::catalog::CapabilityCatalog;

        let store = std::sync::Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(std::sync::Arc::clone(&store));
        // TP21 fix (master ff3f6d10 pre-existing E0599): `registry` 是 pub 字段 (Arc<ToolRegistry>)
        // 而非方法. ToolRegistry 暴露 list()/get(), CapabilityCatalog 由 from_registry(&ToolRegistry) 构造.
        let names = bridge.registry.list();
        for tool in [
            "EnhancedShell",
            "FetchEngine",
            "EnhancedBrowser",
            "CodeIntelligence",
            "ImageGenEnhanced",
            "ImageProcess",
            "VSearch",
            "EnhancedFileOps",
            "RepoQualityAnalyzer",
        ] {
            assert!(
                bridge.registry.get(tool).is_some(),
                "[N17] 装配后 `{tool}` 不在 registry"
            );
        }
        for tool in [
            "EnhancedShell",
            "FetchEngine",
            "EnhancedBrowser",
            "CodeIntelligence",
            "ImageGenEnhanced",
            "ImageProcess",
            "VSearch",
            "EnhancedFileOps",
            "RepoQualityAnalyzer",
        ] {
            assert!(
                names.contains(&tool.to_string()),
                "[N17] names() 缺 `{tool}`"
            );
        }
        let cat = CapabilityCatalog::from_registry(bridge.registry.as_ref());
        // TP21 fix (master ff3f6d10): ToolBridge::new 同时调用 apeireth_tools::register_all
        // (战役 2-5 的 9 件: WebSearch/FileOperator/Git/ShellExec/Grep/ApplyPatch/LongTask/
        // WebFetch/Crawl) + 9 件 N17 子 crate (EnhancedShell/FetchEngine/...) = 18 件基线 +
        // 其他.  断言改为 "≥ 9 N17" 而不是 "== 9" — 上方 contains 循环已逐件验证 N17 全装,
        // 没必要硬等于总数 (总数随战役推进会涨).
        assert!(
            cat.len() >= 9,
            "[N17] catalog 应至少含 9 件 N17 工具 (实测 {})",
            cat.len()
        );
        let mut sorted = cat.names();
        sorted.sort();
        assert_eq!(cat.names(), sorted, "[N17] catalog 排序应确定性");
        let md = cat.render_markdown();
        assert!(md.contains("| EnhancedShell |"), "[N17] markdown 应含首件");
        assert!(
            md.contains("| RepoQualityAnalyzer |"),
            "[N17] markdown 应含末件"
        );
    }

    #[tokio::test]
    async fn n17_nine_register_unregister_round_trip_zero_residue() {
        let registry = apeireth_tool_registry::ToolRegistry::new();
        let nine: Vec<(
            &str,
            fn(&apeireth_tool_registry::ToolRegistry) -> Result<(), String>,
            fn(&apeireth_tool_registry::ToolRegistry) -> bool,
        )> = vec![
            (
                "EnhancedShell",
                apeireth_tool_shell::register::register,
                apeireth_tool_shell::register::unregister,
            ),
            (
                "FetchEngine",
                apeireth_tool_fetch::register::register,
                apeireth_tool_fetch::register::unregister,
            ),
            (
                "EnhancedBrowser",
                apeireth_tool_browser::register::register,
                apeireth_tool_browser::register::unregister,
            ),
            (
                "CodeIntelligence",
                apeireth_tool_codesearch::register::register,
                apeireth_tool_codesearch::register::unregister,
            ),
            (
                "ImageGenEnhanced",
                apeireth_tool_image_gen::register::register,
                apeireth_tool_image_gen::register::unregister,
            ),
            (
                "ImageProcess",
                apeireth_tool_image_process::register::register,
                apeireth_tool_image_process::register::unregister,
            ),
            (
                "VSearch",
                apeireth_tool_search::register::register,
                apeireth_tool_search::register::unregister,
            ),
            (
                "EnhancedFileOps",
                apeireth_tool_filesystem::register::register,
                apeireth_tool_filesystem::register::unregister,
            ),
            (
                "RepoQualityAnalyzer",
                apeireth_repo_tools::register::register,
                apeireth_repo_tools::register::unregister,
            ),
        ];
        for (name, reg, _) in &nine {
            reg(&registry).unwrap_or_else(|e| panic!("[N17] `{name}` register 失败: {e}"));
            assert!(
                registry.get(name).is_some(),
                "[N17] `{name}` register 后 get 查不到"
            );
        }
        assert_eq!(
            registry.len(),
            9,
            "[N17] 9 件 register 后 registry 应为 9 件"
        );
        for (name, _, unreg) in &nine {
            assert!(unreg(&registry), "[N17] `{name}` 首次 unregister 应返 true");
            assert!(
                registry.get(name).is_none(),
                "[N17] `{name}` unregister 后残留"
            );
            assert!(
                !unreg(&registry),
                "[N17] `{name}` 重复 unregister 应返 false (幂等)"
            );
        }
        assert_eq!(
            registry.len(),
            0,
            "[N17] 9 件全卸后 registry 应为 0 件 (0 残留)"
        );
    }
}

// ============================================================
// TP12 (A2, P0) 集成测试 — 结构化回灌
// ============================================================

#[cfg(test)]
mod tp12_tests {
    use super::*;
    use apeireth_tools::{GuardrailError, GuardrailKind, Tripwire};

    /// 干净 ExecutionResult → inject_tp12_into_output 不动 output
    #[test]
    fn inject_clean_result_passes_through() {
        let r = ExecutionResult {
            tool_name: "X".into(),
            success: true,
            output: json!({"ok": true, "n": 42}),
            error: None,
            duration_ms: 10,
            ..Default::default()
        };
        let out = inject_tp12_into_output(&r);
        assert_eq!(out["ok"], json!(true));
        assert_eq!(out["n"], json!(42));
        assert!(out.get("_tp12_report").is_none(), "干净调用不应注入");
    }

    /// guardrail 命中 → output 加 _tp12_report.guardrail_error
    #[test]
    fn inject_guardrail_error_adds_report() {
        let r = ExecutionResult {
            tool_name: "X".into(),
            success: false,
            output: json!("[GuardrailBlocked] path contains .."),
            error: Some("path_traversal".into()),
            duration_ms: 1,
            guardrail_error: Some(GuardrailError {
                kind: GuardrailKind::PathTraversal,
                tool_name: "X".into(),
                field: "$.path".into(),
                detail: "contains ../".into(),
                hint: "remove .. segments".into(),
            }),
            ..Default::default()
        };
        let out = inject_tp12_into_output(&r);
        let report = out.get("_tp12_report").expect("report missing");
        let ge = report
            .get("guardrail_error")
            .expect("guardrail_error missing");
        assert_eq!(ge["kind"], "path_traversal");
        assert_eq!(ge["field"], "$.path");
        assert_eq!(ge["hint"], "remove .. segments");
    }

    /// tripwire 命中 → output 加 _tp12_report.tripwire
    #[test]
    fn inject_tripwire_adds_report() {
        let r = ExecutionResult {
            tool_name: "S".into(),
            success: false,
            output: json!("[TripwireBlocked] AWS Access Key detected"),
            error: Some("secret_leak".into()),
            duration_ms: 1,
            tripwire: Some(Tripwire {
                kind: apeireth_tools::GuardrailKind::SecretLeak,
                tool_name: "S".into(),
                field: "$.config".into(),
                detail: "AWS Access Key detected (AKIA prefix)".into(),
                hint: "redact before re-injection".into(),
            }),
            ..Default::default()
        };
        let out = inject_tp12_into_output(&r);
        let report = out.get("_tp12_report").expect("report missing");
        let tw = report.get("tripwire").expect("tripwire missing");
        assert_eq!(tw["kind"], "secret_leak");
        assert_eq!(tw["field"], "$.config");
    }

    /// 非 object output (string / null) → 包成 {raw, _tp12_report}
    #[test]
    fn inject_non_object_output_wraps_in_raw() {
        let r = ExecutionResult {
            tool_name: "X".into(),
            success: false,
            output: json!("plain string"),
            error: None,
            duration_ms: 1,
            guardrail_error: Some(GuardrailError {
                kind: GuardrailKind::ShellInjection,
                tool_name: "X".into(),
                field: "$.cmd".into(),
                detail: "contains ;".into(),
                hint: "remove ; and chain".into(),
            }),
            ..Default::default()
        };
        let out = inject_tp12_into_output(&r);
        assert_eq!(out["raw"], "plain string");
        assert!(out.get("_tp12_report").is_some());
    }
}

/// **TP12 — 把 guardrail/validation/tripwire 结构化信息并入 output**
///
/// **目的**: 模型在 tool message 里看到原始 `[GuardrailBlocked] xxx` 字符串时,
/// 只有模糊的 hint; 把结构化字段 (`kind`, `field`, `hint`) 一并塞进 `_tp12_report`,
/// 模型可以解析后自修正 (e.g. 改 args.path 去掉 `../`, 改 cmd 去掉 `;`).
///
/// **0 装 PASS**: 若 ExecutionResult 无任何 TP12 字段, 返回 r.output 原值不变 (向后兼容干净调用).
///
/// **结构示例**:
/// ```json
/// {
///   "spilled": true,             // 或其他原始字段
///   "_tp12_report": {
///     "guardrail_error": {
///       "kind": "path_traversal",
///       "field": "$.path",
///       "hint": "remove `../` segments"
///     }
///   }
/// }
/// ```
fn inject_tp12_into_output(r: &ExecutionResult) -> Value {
    // 检查是否有任何 TP12 字段
    let has_guardrail = r.guardrail_error.is_some();
    let has_validation = r.validation_error.is_some();
    let has_tripwire = r.tripwire.is_some();
    if !(has_guardrail || has_validation || has_tripwire) {
        // 干净调用 → 原值不动 (向后兼容)
        return r.output.clone();
    }

    // 构造 _tp12_report 对象
    let mut report = serde_json::Map::new();
    if let Some(ge) = &r.guardrail_error {
        if let Ok(v) = serde_json::to_value(ge) {
            report.insert("guardrail_error".into(), v);
        }
    }
    if let Some(ve) = &r.validation_error {
        if let Ok(v) = serde_json::to_value(ve) {
            report.insert("validation_error".into(), v);
        }
    }
    if let Some(tw) = &r.tripwire {
        if let Ok(v) = serde_json::to_value(tw) {
            report.insert("tripwire".into(), v);
        }
    }

    // 把 _tp12_report 加进 output (object 形式 → 插入字段; 非 object → 包成 {"raw": <原值>, "_tp12_report": ...})
    match r.output.clone() {
        Value::Object(mut obj) => {
            obj.insert("_tp12_report".into(), Value::Object(report));
            Value::Object(obj)
        }
        other => {
            let mut obj = serde_json::Map::new();
            obj.insert("raw".into(), other);
            obj.insert("_tp12_report".into(), Value::Object(report));
            Value::Object(obj)
        }
    }
}

// ============================================================
// TP29 (生态批) 集成测试 — yaml_spec 与 tool_bridge 衔接
// ============================================================

#[cfg(test)]
mod tp29_tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// 合法 YAML → register_yaml_spec 成功 + registry 可查
    #[test]
    fn bridge_register_yaml_spec_legal() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("eco_tool.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(
            b"name: eco_tool\ndescription: ecological plugin\n\
              parameters:\n  - name: query\n    type: string\n    description: q\n    required: true\n\
              permissions:\n  - network:api.example.com\n\
              credentials:\n  - name: api_key\n    required: false\n    env: ${ECO_API_KEY}\n",
        )
        .unwrap();
        let name = bridge.register_yaml_spec(&path).expect("register ok");
        assert_eq!(name, "eco_tool");
        assert!(bridge.registry.get("eco_tool").is_some());
    }

    /// 非法 YAML (缺 description) → register 失败, registry 数量不变 (fail-safety)
    #[test]
    fn bridge_register_yaml_spec_invalid_does_not_corrupt() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let count_before = bridge.registry.len();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "name: bad\n").unwrap();
        let res = bridge.register_yaml_spec(&path);
        assert!(res.is_err(), "非法 YAML 应失败");
        assert_eq!(
            bridge.registry.len(),
            count_before,
            "失败后 registry 数量应不变"
        );
        assert!(bridge.registry.get("bad").is_none());
    }

    /// 同名冲突 → 不覆盖现有 (返回 Err, 原工具仍在)
    #[test]
    fn bridge_register_yaml_spec_name_conflict() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("dupe.yaml");
        std::fs::write(
            &path,
            "name: recall_memory\ndescription: clash with existing\n",
        )
        .unwrap();
        // recall_memory 已被 ToolBridge::new 预注册 (apeireth-memory 工具)
        assert!(bridge.registry.get("recall_memory").is_some());
        let res = bridge.register_yaml_spec(&path);
        assert!(res.is_err(), "同名应冲突拒绝");
        // recall_memory 仍在 (未被 yaml 占位覆盖)
        assert!(bridge.registry.get("recall_memory").is_some());
    }

    /// register_yaml_spec_dir 批量注册, 跳过多 / 失败文件, 仅成功入册
    #[test]
    fn bridge_register_yaml_spec_dir_mixed() {
        let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
        let bridge = ToolBridge::new(store);
        let dir = TempDir::new().unwrap();
        // 2 个合法 + 1 个非法
        std::fs::write(dir.path().join("a.yaml"), "name: y_a\ndescription: a\n").unwrap();
        std::fs::write(dir.path().join("b.yml"), "name: y_b\ndescription: b\n").unwrap();
        std::fs::write(dir.path().join("c.yaml"), "name: bad\n").unwrap();
        let names = bridge.register_yaml_spec_dir(dir.path());
        // 顺序: a.yaml → b.yml → c.yaml; c 失败被跳过.
        assert_eq!(names, vec!["y_a".to_string(), "y_b".to_string()]);
        assert!(bridge.registry.get("y_a").is_some());
        assert!(bridge.registry.get("y_b").is_some());
        assert!(bridge.registry.get("bad").is_none());
    }
}
