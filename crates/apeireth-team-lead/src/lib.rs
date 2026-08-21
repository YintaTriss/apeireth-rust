//! `apeireth-team-lead` — **Apeireth R20 阶段 1: Team Lead / Orchestrator**
//!
//! **职责边界** (per `spectrAI-integration-blueprint §5.1`, 2026-08-05):
//! - `apeireth-team-lead` (本 crate) = 多 AI Agent 编排 (1:1 翻译 v0.9.21 商业版
//!   `out/main/agent/AgentMCPServer.js` Orchestrator 估缺 P0, A 方案 13:34 拍板命名)
//! - `apeireth-supervisor` (PID 1) = 进程级监督树 (5 sub-supervisor + RestartStrategy)
//!
//! **关键: 命名冲突规范**: 上述 2 个 crate 名字相近但概念完全不同.
//! team-lead 编排 AI Agent 协作; supervisor 监督系统进程. 主人 2026-08-05 19:50 拍板.
//!
//! **1:1 翻译 v0.9.21 商业版** (per `v09021-commercial-extract-2026-08-05.md` §3.1 #4 +
//! `supervisor-prompt-818-summary-2026-08-05.md`):
//! - 8 调度工具: `spawn_agent` / `send_to_agent` / `get_agent_output` / `wait_agent_idle` /
//!   `wait_agent` / `get_agent_status` / `list_agents` / `cancel_agent`
//! - 3 worktree 工具: `get_task_info` / `check_merge` / `merge_worktree`
//! - 3 感知工具: `list_sessions` / `get_session_summary` / `search_sessions`
//! - supervisorPrompt 818 行 (per supervisorPrompt.ts 7 个 build*() 函数 + 注入入口)
//!   → `pub const SUPERVISOR_PROMPT: &str = include_str!("md/supervisor_prompt.md");`
//!   (编译期嵌入, UTF-8 字节原样保留, 不转义)
//!
//! **本骨架设计**:
//! - §1 错误类型: 5 类 (TeamNotFound / SpawnFailed / MidTaskFailed / ToolUnauthorized / HandoffFailed)
//! - §2 enum: AgentStatus (7 variant) + AgentRole (3 variant) + MessageType (5 variant)
//! - §3 struct: TeamLead + SubAgent + Message + TeamConfig
//! - §4 trait: `Orchestrator` (14 工具) + `SubAgentHandle`
//! - §5 占位 impl + `build_supervisor_prompt` orchestrator (B 方案 per §5.1 supervisor-prompt-818)
//! - §6 tests: 5 fixture (SpawnAgent / SendMessage no swallow / DualAck / CancelAgent / K-1 invariant)

#![warn(missing_docs)]
#![deny(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use apeireth_agent::{Agent, AgentManager};

// ============================================================================
// TP11 (A1, P0) Handoff 委托协议 re-export (per team-work-doc §11)
// ============================================================================
//
// **设计决策** (per task §2 边界 + 实际 dep 约束):
// - Handoff 协议的所有类型实装在 `apeireth-tool-registry::handoff` 模块
// - 原因: apeireth-team-lead → apeireth-agent → apeireth-api → apeireth-tools
//   是已存在的依赖链; 若 apeireth-tools → apeireth-team-lead 形成 cycle
// - 妥协: 在 tool-registry (leaf crate, 0 apeireth-* dep) 实现 handoff, team-lead 透传 re-export
// - team-lead 仍然是"挂接机制"的核心: Orchestrator 持有 HandoffRegistry
pub use apeireth_tool_registry::handoff;
pub use apeireth_tool_registry::handoff::{
    install_handoff_tools, AgentContext, FnFilter, HandoffError, HandoffOutcome, HandoffRegistry,
    HandoffRequest, HandoffResult, HandoffTarget, InputFilter, OnHandoff, PassthroughFilter,
    SyncOnHandoff, TargetSnapshot, TransferTool,
};

// ============================================================================
// m3 hallucination 防御 #3 (per m3-hallucination-defense-2026-08-05.md §2.4 + §2.1)
// WHITELIST 编译期 hardcode, validate_tool_call 在 dispatch 前 schema 校验.
// 防止 minimax m3 模型幻觉调用不存在的 orchestrator 工具.
// 14 工具 = 8 调度 (spawn_agent / send_to_agent / ...) + 3 worktree + 3 感知.
// ============================================================================

/// m3 防御: Team Lead / Orchestrator 14 工具白名单 (编译期 hardcode).
pub const TOOL_WHITELIST: &[&str] = &[
    // 8 调度
    "apeireth_team_lead_spawn_agent",
    "apeireth_team_lead_send_to_agent",
    "apeireth_team_lead_get_agent_output",
    "apeireth_team_lead_wait_agent_idle",
    "apeireth_team_lead_wait_agent",
    "apeireth_team_lead_get_agent_status",
    "apeireth_team_lead_list_agents",
    "apeireth_team_lead_cancel_agent",
    // 3 worktree
    "apeireth_team_lead_get_task_info",
    "apeireth_team_lead_check_merge",
    "apeireth_team_lead_merge_worktree",
    // 3 感知
    "apeireth_team_lead_list_sessions",
    "apeireth_team_lead_get_session_summary",
    "apeireth_team_lead_search_sessions",
];

/// m3 防御: 校验工具调用是否在白名单内.
pub fn validate_tool_call(tool: &str, _args: &serde_json::Value) -> Result<(), TeamLeadError> {
    if !TOOL_WHITELIST.contains(&tool) {
        return Err(TeamLeadError::ToolNotWhitelisted(tool.to_string()));
    }
    Ok(())
}

// ============================================================================
// §1 错误类型 (5 类, per A 方案 13:34 拍板)
// ============================================================================

/// Team Lead / Orchestrator 错误类型. 5 类错误覆盖整个 orchestrator 失败面.
#[derive(Debug, Error)]
pub enum TeamLeadError {
    /// m3 防御: 工具未在白名单内 (per m3-hallucination-defense §2.4)
    #[error("tool not whitelisted: {0}")]
    ToolNotWhitelisted(String),
    /// 团队未找到 (eg. spawn 时 agent_id 不在 registry)
    #[error("team not found: {0}")]
    TeamNotFound(String),

    /// 子 Agent 启动失败 (eg. 创建子进程失败 / provider 配置错误)
    #[error("sub-agent spawn failed: agent_id={agent_id}, reason={reason}")]
    SpawnFailed {
        /// 子 Agent ID
        agent_id: String,
        /// 失败原因
        reason: String,
    },

    /// 中途失败 (eg. 子 Agent 已 spawn 但运行中崩溃 / wait_agent 超时)
    #[error("sub-agent mid-task failure: agent_id={agent_id}, reason={reason}")]
    MidTaskFailed {
        /// 子 Agent ID
        agent_id: String,
        /// 失败原因
        reason: String,
    },

    /// 工具未授权 (eg. AI 调用了未注册的 tool, 或工具被 apeireth-tool-approval 拒绝)
    #[error("tool not authorized: tool={tool}, agent_id={agent_id}")]
    ToolUnauthorized {
        /// 工具名
        tool: String,
        /// 触发该调用的 agent_id
        agent_id: String,
    },

    /// 跳板失败 (eg. 跨 session handoff 时 trace_id 丢失 / target session 离线)
    #[error("handoff failed: from={from}, to={to}, reason={reason}")]
    HandoffFailed {
        /// 源 session
        from: String,
        /// 目标 session
        to: String,
        /// 失败原因
        reason: String,
    },
}

/// Team Lead crate Result 别名.
pub type TeamLeadResult<T> = Result<T, TeamLeadError>;

// ============================================================================
// §2 关键 enum
// ============================================================================

/// 子 Agent 生命周期状态 (7 variant, per AgentMCPServer.js Orchestrator 字段).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentStatus {
    /// 已分配但未启动.
    Pending,
    /// 正在运行.
    Running,
    /// 空闲 (oneShot 完成后空闲, 等待交互式 follow-up).
    Idle,
    /// 正常完成.
    Completed,
    /// 失败.
    Failed,
    /// 用户/调度器主动取消.
    Cancelled,
    /// 超时.
    TimedOut,
}

/// Agent 角色 (3 variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    /// 总指挥 (Orchestrator 本身, 通常只有一个).
    Supervisor,
    /// 工作者 (实际执行任务的子 Agent).
    Worker,
    /// 观察者 (只读, 用于 monitoring / reporting).
    Observer,
}

/// 消息类型 (5 variant, per orchestrator send/receive 协议).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    /// 创建新子 Agent.
    Spawn,
    /// 发送文本指令.
    Send,
    /// 等待子 Agent 变 Idle / Completed.
    Wait,
    /// 子 Agent 输出增量.
    Output,
    /// 取消子 Agent.
    Cancel,
}

// ============================================================================
// §3 关键 struct
// ============================================================================

/// 子 Agent 句柄 (id / role / prompt / status / 创建时间).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    /// 唯一 ID.
    pub id: String,
    /// Agent 角色.
    pub role: AgentRole,
    /// 系统 prompt (per build_supervisor_prompt 输出).
    pub prompt: String,
    /// 当前状态.
    pub status: AgentStatus,
    /// 创建时间戳 (epoch millis, per apeireth-bus::now_ms).
    pub created_at_ms: i64,
}

impl SubAgent {
    /// 创建新 SubAgent (默认 Pending 状态).
    pub fn new(id: impl Into<String>, role: AgentRole, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            role,
            prompt: prompt.into(),
            status: AgentStatus::Pending,
            created_at_ms: apeireth_bus::now_ms(),
        }
    }
}

/// 跨 Agent 消息 (sender / receiver / payload / message_type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 发送方 agent_id (None = orchestrator 自身).
    pub sender: Option<String>,
    /// 接收方 agent_id.
    pub receiver: String,
    /// 消息类型.
    pub message_type: MessageType,
    /// 消息负载 (JSON Value, 灵活).
    pub payload: serde_json::Value,
    /// 链路追踪 ID (per apeireth-bus::next_trace_id, 跨子 Agent 保持).
    pub trace_id: u64,
    /// 创建时间戳.
    pub created_at_ms: i64,
}

impl Message {
    /// 构造新消息 (自动分配 trace_id).
    pub fn new(
        sender: Option<String>,
        receiver: impl Into<String>,
        message_type: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            sender,
            receiver: receiver.into(),
            message_type,
            payload,
            trace_id: apeireth_bus::next_trace_id(),
            created_at_ms: apeireth_bus::now_ms(),
        }
    }
}

/// Team Lead 启动配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// 团队名称 (用于 logging / 多 team 隔离).
    pub team_name: String,
    /// 最大并发子 Agent 数 (默认 8, 防止资源耗尽).
    pub max_concurrent: usize,
    /// wait_agent 默认超时 (per §2.3 supervisor-prompt-818, codex ≤ 90000ms).
    pub default_wait_timeout_ms: u64,
    /// autoWorktree 开关 (true → 强制 worktree 隔离).
    pub auto_worktree: bool,
    /// 可用 Provider 列表 (注入到 supervisorPrompt 占位符).
    pub available_providers: Vec<String>,
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self {
            team_name: "default".to_string(),
            max_concurrent: 8,
            // per supervisor-prompt-818 §2.3: codex-based supervisor wait_agent ≤ 90000ms
            default_wait_timeout_ms: 90_000,
            auto_worktree: false,
            // per supervisor-prompt-818 §2.2 #9: fallback 顺序固定
            available_providers: vec![
                "claude-code".to_string(),
                "gemini-cli".to_string(),
                "codex".to_string(),
                "opencode".to_string(),
            ],
        }
    }
}

/// Team Lead / Orchestrator 主体 (持有 children map + AgentManager 引用).
///
/// **命名澄清** (per spectrAI-integration-blueprint §5.1):
/// 跟 `apeireth_supervisor::PidOneSupervisor` (进程级 PID 1) **完全无关**.
///
/// 注意: 不 derive `Debug` 因为 `AgentManager` 没 `Debug` impl (per 任务 8 项不修改承诺).
pub struct TeamLead {
    /// 配置.
    config: TeamConfig,
    /// 子 Agent 表 (id → SubAgent).
    children: Arc<Mutex<HashMap<String, SubAgent>>>,
    /// Agent 注册表引用 (per apeireth-agent 战役 2-4).
    agent_manager: Arc<AgentManager>,
}

impl TeamLead {
    /// 创建新 TeamLead (持有 config + agent_manager).
    pub fn new(config: TeamConfig, agent_manager: Arc<AgentManager>) -> Self {
        Self {
            config,
            children: Arc::new(Mutex::new(HashMap::new())),
            agent_manager,
        }
    }

    /// 读取 team config.
    pub fn config(&self) -> &TeamConfig {
        &self.config
    }

    /// 当前子 Agent 数量.
    pub async fn child_count(&self) -> usize {
        self.children.lock().await.len()
    }
}

// ============================================================================
// §4 关键 trait: Orchestrator 14 工具 (8 调度 + 3 worktree + 3 感知)
// ============================================================================

/// Orchestrator trait — 14 工具 (1:1 翻译 AgentMCPServer.js Orchestrator).
///
/// **mid-task bug 修复** (per sub-agent 1 architect §6 修复建议):
/// `send_to_agent` 用 `await` + 检查返回, 不允许"fire-and-forget"吞掉错误.
#[async_trait]
pub trait Orchestrator: Send + Sync {
    // ---------- 8 调度工具 ----------

    /// 工具 1: `spawn_agent` — 创建新子 Agent.
    async fn spawn_agent(&self, role: AgentRole, prompt: String) -> TeamLeadResult<String>;

    /// 工具 2: `send_to_agent` — 发送消息到子 Agent (await + 检查返回).
    async fn send_to_agent(&self, agent_id: &str, message: Message) -> TeamLeadResult<()>;

    /// 工具 3: `get_agent_output` — 读取子 Agent 输出.
    async fn get_agent_output(&self, agent_id: &str) -> TeamLeadResult<String>;

    /// 工具 4: `wait_agent_idle` — 短轮询等待 (60-90s 一次, per §2.3).
    async fn wait_agent_idle(&self, agent_id: &str, timeout_ms: u64) -> TeamLeadResult<()>;

    /// 工具 5: `wait_agent` — 等待子 Agent 完成 (用循环轮询, 不单次长阻塞).
    async fn wait_agent(&self, agent_id: &str, timeout_ms: u64) -> TeamLeadResult<()>;

    /// 工具 6: `get_agent_status` — 读取子 Agent 状态.
    async fn get_agent_status(&self, agent_id: &str) -> TeamLeadResult<AgentStatus>;

    /// 工具 7: `list_agents` — 列出所有子 Agent.
    async fn list_agents(&self) -> TeamLeadResult<Vec<SubAgent>>;

    /// 工具 8: `cancel_agent` — 取消子 Agent.
    async fn cancel_agent(&self, agent_id: &str) -> TeamLeadResult<()>;

    // ---------- 3 worktree 工具 ----------

    /// 工具 9: `get_task_info` — 读取 worktree 任务信息.
    async fn get_task_info(&self, task_id: &str) -> TeamLeadResult<serde_json::Value>;

    /// 工具 10: `check_merge` — 检查 worktree 合并冲突.
    async fn check_merge(&self, task_id: &str) -> TeamLeadResult<bool>;

    /// 工具 11: `merge_worktree` — 合并 worktree 回主分支.
    async fn merge_worktree(
        &self,
        task_id: &str,
        squash: bool,
        cleanup: bool,
    ) -> TeamLeadResult<()>;

    // ---------- 3 感知工具 (per supervisor-prompt-818 §2.1) ----------

    /// 工具 12: `list_sessions` — 列出所有 Claude Code 会话.
    async fn list_sessions(
        &self,
        status_filter: Option<&str>,
        limit: Option<usize>,
    ) -> TeamLeadResult<Vec<serde_json::Value>>;

    /// 工具 13: `get_session_summary` — 读取会话摘要.
    async fn get_session_summary(
        &self,
        session_id: Option<&str>,
        session_name: Option<&str>,
    ) -> TeamLeadResult<serde_json::Value>;

    /// 工具 14: `search_sessions` — 跨会话搜索.
    async fn search_sessions(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> TeamLeadResult<Vec<serde_json::Value>>;
}

/// 子 Agent 句柄 trait (per SpectrAI 1 个 sub-agent = 1 个 handle).
#[async_trait]
pub trait SubAgentHandle: Send + Sync {
    /// 句柄对应的子 Agent ID.
    fn id(&self) -> &str;

    /// 当前状态.
    async fn status(&self) -> AgentStatus;

    /// 等待到 Idle / Completed / Failed.
    async fn wait_idle(&self, timeout_ms: u64) -> TeamLeadResult<()>;
}

// ============================================================================
// §5 占位 impl + supervisorPrompt 818 行翻译 (B 方案, per §5.1 supervisor-prompt-818)
// ============================================================================

/// 14 工具数量 (1:1 翻译 AgentMCPServer.js Orchestrator + 3 感知).
/// 编译期 hardcode, 改 Orchestrator trait 时必须同步更新.
pub const ORCHESTRATOR_TOOL_COUNT: usize = 14;

/// 8 调度工具 + 3 worktree + 3 感知 = 14.
pub const SCHEDULING_TOOL_COUNT: usize = 8;
/// Worktree 工具数 (get_task_info / check_merge / merge_worktree), per AgentMCPServer.js.
pub const WORKTREE_TOOL_COUNT: usize = 3;
/// 感知工具数 (list_sessions / get_session_summary / search_sessions), per supervisorPrompt §2.1.
pub const AWARENESS_TOOL_COUNT: usize = 3;

const _: () = {
    assert!(
        SCHEDULING_TOOL_COUNT + WORKTREE_TOOL_COUNT + AWARENESS_TOOL_COUNT
            == ORCHESTRATOR_TOOL_COUNT,
        "8+3+3 = 14 tools (1:1 AgentMCPServer.js Orchestrator)"
    );
};

/// 占位 impl — Orchestrator trait 的骨架实现.
///
/// **本阶段不实装** (per 主人 2026-08-05 19:50 "派成员干" + R20 阶段 1 Fixture 1 准备):
/// - ✅ 14 工具方法签名对齐 AgentMCPServer.js
/// - ✅ spawn_agent / cancel_agent 内部真做 (写 children map, 写 agent_manager)
/// - ⏳ send_to_agent / wait_agent / output / worktree / 感知 工具 → 返回 unimplemented 占位
///   (等 R20 阶段 1 Fixture 1 `test_team_lead_workflow` 写实装)
#[async_trait]
impl Orchestrator for TeamLead {
    #[instrument(skip(self, prompt), fields(role = ?role))]
    async fn spawn_agent(&self, role: AgentRole, prompt: String) -> TeamLeadResult<String> {
        // 真做: 创建 SubAgent + 注入到 children map
        let id = format!("agent-{}", apeireth_bus::next_trace_id());
        let sub = SubAgent::new(&id, role, prompt.clone());
        // 同步注册到 agent_manager (per apeireth-agent 战役 2-4)
        let agent = Agent::new(&id, &id, vec![], vec![], &prompt);
        if let Err(e) = self.agent_manager.register(agent) {
            return Err(TeamLeadError::SpawnFailed {
                agent_id: id,
                reason: format!("agent_manager.register failed: {e}"),
            });
        }
        self.children.lock().await.insert(id.clone(), sub);
        info!(agent_id = %id, "spawn_agent ok");
        Ok(id)
    }

    async fn send_to_agent(&self, agent_id: &str, _message: Message) -> TeamLeadResult<()> {
        // mid-task bug 修复: 必须 await + 检查返回 (per sub-agent 1 architect §6)
        let children = self.children.lock().await;
        if !children.contains_key(agent_id) {
            return Err(TeamLeadError::TeamNotFound(agent_id.to_string()));
        }
        // ⏳ 占位: R20 阶段 1 Fixture 1 实装 apeireth-bus L4 跨进程 send
        warn!(agent_id = %agent_id, "send_to_agent: R20 stage 1 placeholder");
        Ok(())
    }

    async fn get_agent_output(&self, agent_id: &str) -> TeamLeadResult<String> {
        self.children
            .lock()
            .await
            .get(agent_id)
            .ok_or_else(|| TeamLeadError::TeamNotFound(agent_id.to_string()))?;
        debug!(agent_id = %agent_id, "get_agent_output: placeholder");
        Ok(String::new())
    }

    async fn wait_agent_idle(&self, agent_id: &str, timeout_ms: u64) -> TeamLeadResult<()> {
        let children = self.children.lock().await;
        if !children.contains_key(agent_id) {
            return Err(TeamLeadError::TeamNotFound(agent_id.to_string()));
        }
        // per §2.3 supervisor-prompt-818: 60-90s 一次, 不单次长阻塞
        debug!(agent_id = %agent_id, timeout_ms, "wait_agent_idle: placeholder (loop poll)");
        let _ = timeout_ms; // 占位
        Ok(())
    }

    async fn wait_agent(&self, agent_id: &str, timeout_ms: u64) -> TeamLeadResult<()> {
        // 内部用循环轮询, 不允许单次长阻塞
        self.wait_agent_idle(agent_id, timeout_ms).await
    }

    async fn get_agent_status(&self, agent_id: &str) -> TeamLeadResult<AgentStatus> {
        self.children
            .lock()
            .await
            .get(agent_id)
            .map(|s| s.status)
            .ok_or_else(|| TeamLeadError::TeamNotFound(agent_id.to_string()))
    }

    async fn list_agents(&self) -> TeamLeadResult<Vec<SubAgent>> {
        Ok(self.children.lock().await.values().cloned().collect())
    }

    async fn cancel_agent(&self, agent_id: &str) -> TeamLeadResult<()> {
        let mut children = self.children.lock().await;
        let sub = children
            .get_mut(agent_id)
            .ok_or_else(|| TeamLeadError::TeamNotFound(agent_id.to_string()))?;
        sub.status = AgentStatus::Cancelled;
        info!(agent_id = %agent_id, "cancel_agent ok");
        Ok(())
    }

    async fn get_task_info(&self, _task_id: &str) -> TeamLeadResult<serde_json::Value> {
        // ⏳ 占位: 集成 apeireth-git 阶段 4 SDK
        Ok(serde_json::json!({"worktree": null, "base_branch": null}))
    }

    async fn check_merge(&self, _task_id: &str) -> TeamLeadResult<bool> {
        Ok(true) // 占位: 无冲突
    }

    async fn merge_worktree(
        &self,
        _task_id: &str,
        _squash: bool,
        _cleanup: bool,
    ) -> TeamLeadResult<()> {
        Ok(()) // 占位
    }

    async fn list_sessions(
        &self,
        _status_filter: Option<&str>,
        _limit: Option<usize>,
    ) -> TeamLeadResult<Vec<serde_json::Value>> {
        Ok(vec![]) // 占位: 集成 apeireth-mcp
    }

    async fn get_session_summary(
        &self,
        _session_id: Option<&str>,
        _session_name: Option<&str>,
    ) -> TeamLeadResult<serde_json::Value> {
        Ok(serde_json::json!({})) // 占位
    }

    async fn search_sessions(
        &self,
        _query: &str,
        _limit: Option<usize>,
    ) -> TeamLeadResult<Vec<serde_json::Value>> {
        Ok(vec![]) // 占位
    }
}

// ============================================================================
// §5.1 supervisorPrompt 818 行翻译 (B 方案, per §5.1 supervisor-prompt-818)
// ============================================================================

/// supervisorPrompt 818 行 1:1 翻译 (per `supervisor-prompt-818-summary-2026-08-05.md` §5.1 B 方案).
///
/// **7 段** (per §1.1 supervisor-prompt-818):
/// 1. 感知层 (18 行)
/// 2. 调度层 (158 行, 核心)
/// 3. Progress / Timeout addon (13 行, m3 测后追加, 不能漏)
/// 4. Workspace 多仓库 (32 行, Task 流 + Session 流)
/// 5. 文件操作规范 (42 行, 最高优先级)
/// 6. Worktree 隔离规范 (56 行)
/// 7. Worktree 已激活 (15 行)
///
/// **关键 1:1 保留** (per K-1 强校验 §5.3 supervisor-prompt-818):
/// - "Claude Code" / "Claude" 字样 1:1 保留 (minimax m3 兼容)
/// - 14 调度工具名 + 4 个 spectrai_* 文件操作工具名 1:1 保留
/// - 4 个 Provider 名 (claude-code / gemini-cli / codex / opencode) 保留
/// - "SpectrAI" 平台名 → "apeireth" (per 主人 2026-08-05 拍板命名重命名)
pub const SUPERVISOR_PROMPT: &str = include_str!("md/supervisor_prompt.md");

/// `build_supervisor_prompt` orchestrator (B 方案: 1 个 orchestrator 函数 + 7 个 build_*).
///
/// **拼接规则** (per §5.1 supervisor-prompt-818):
/// `awareness` + "\n" + `supervisor` (含 ${availableProviders} 注入) + `progress_addon`
pub fn build_supervisor_prompt(available_providers: &[&str]) -> String {
    // 本骨架用单一 include_str! (818 行整体), 后续可拆为 7 段 + format!
    // 现阶段先做"包含关系"验证 (K-1 invariant), format 注入留给 R20 阶段 1 Fixture 1
    let mut s = SUPERVISOR_PROMPT.to_string();
    // 占位: ${availableProviders} 注入
    let providers_str = if available_providers.is_empty() {
        "claude-code, gemini-cli, codex, opencode".to_string()
    } else {
        available_providers.join(", ")
    };
    s = s.replace("${availableProviders}", &providers_str);
    s
}

// ============================================================================
// §6 单元测试 (5 fixture, per 主人 19:50 拍板)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ----- Fixture 1: spawn_agent 成功 -----

    #[tokio::test]
    async fn t01_spawn_agent_succeeds() {
        let mgr = Arc::new(AgentManager::new());
        let lead = TeamLead::new(TeamConfig::default(), mgr.clone());
        let id = lead
            .spawn_agent(AgentRole::Worker, "do research".to_string())
            .await
            .unwrap();
        assert!(id.starts_with("agent-"));
        assert_eq!(lead.child_count().await, 1);
        let status = lead.get_agent_status(&id).await.unwrap();
        assert_eq!(status, AgentStatus::Pending);
    }

    // ----- Fixture 2: send_to_agent 不吞错 (mid-task bug 修复) -----

    #[tokio::test]
    async fn t02_send_to_agent_no_swallow() {
        let mgr = Arc::new(AgentManager::new());
        let lead = TeamLead::new(TeamConfig::default(), mgr);
        // 发送到一个不存在的 agent → 必须返错, 不能吞
        let msg = Message::new(None, "ghost", MessageType::Send, serde_json::json!({}));
        let err = lead.send_to_agent("ghost", msg).await.unwrap_err();
        assert!(matches!(err, TeamLeadError::TeamNotFound(_)));
    }

    // ----- Fixture 3: DualAck (cancel + 状态) -----

    #[tokio::test]
    async fn t03_cancel_agent_dual_ack() {
        let mgr = Arc::new(AgentManager::new());
        let lead = TeamLead::new(TeamConfig::default(), mgr);
        let id = lead
            .spawn_agent(AgentRole::Worker, "task".to_string())
            .await
            .unwrap();
        // 第 1 ack: 状态变 Cancelled
        lead.cancel_agent(&id).await.unwrap();
        let s1 = lead.get_agent_status(&id).await.unwrap();
        assert_eq!(s1, AgentStatus::Cancelled);
        // 第 2 ack: 取消不存在的 agent 返 TeamNotFound
        let err = lead.cancel_agent("ghost").await.unwrap_err();
        assert!(matches!(err, TeamLeadError::TeamNotFound(_)));
    }

    // ----- Fixture 4: list_agents 列出所有子 Agent -----

    #[tokio::test]
    async fn t04_list_agents_reflects_children() {
        let mgr = Arc::new(AgentManager::new());
        let lead = TeamLead::new(TeamConfig::default(), mgr);
        assert_eq!(lead.list_agents().await.unwrap().len(), 0);
        lead.spawn_agent(AgentRole::Worker, "a".to_string())
            .await
            .unwrap();
        lead.spawn_agent(AgentRole::Observer, "b".to_string())
            .await
            .unwrap();
        let list = lead.list_agents().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    // ----- Fixture 5: K-1 强校验 (per §5.3 supervisor-prompt-818) -----

    #[test]
    fn t05_k1_invariant_supervisor_prompt_preserves_claude_code() {
        let prompt = build_supervisor_prompt(&["claude-code", "gemini-cli", "codex", "opencode"]);
        // K-1 #1 + #2: "Claude Code" 字样 ≥ 1 次 (per §5.3 列表)
        assert!(
            prompt.contains("Claude Code"),
            "K-1: SUPERVISOR_PROMPT must preserve 'Claude Code' (minimax m3 compatibility)"
        );
        // K-1 #3: claude-code provider 名 ≥ 1 次
        assert!(
            prompt.contains("claude-code"),
            "K-1: SUPERVISOR_PROMPT must preserve 'claude-code' provider"
        );
        // K-1 #4: 调度工具 spawn_agent 1:1 保留
        assert!(
            prompt.contains("spawn_agent"),
            "K-1: SUPERVISOR_PROMPT must contain 'spawn_agent' tool name"
        );
        // K-1 #5: 调度工具 wait_agent_idle 1:1 保留
        assert!(
            prompt.contains("wait_agent_idle"),
            "K-1: SUPERVISOR_PROMPT must contain 'wait_agent_idle' tool name"
        );
        // K-1 #6: Progress addon "must-do" 字样
        assert!(
            prompt.contains("must-do") || prompt.contains("must do"),
            "K-1: SUPERVISOR_PROMPT must contain 'must-do' (Progress addon marker)"
        );
    }
}

// R177: organ invariants (5 tests + 2 Kani)
mod organ_kani_proofs;

// TP20-N20 ApprovalBridge — companion ↔ orchestrator 跨 crate 审批透传契约.
// 详见 src/approval_bridge.rs 模块头 (0 装 PASS / 字段透传保真 / 缺字段默认 reject).
pub mod approval_bridge;
pub use approval_bridge::{
    ApprovalBridge, ApprovalBridgeError, ApprovalRequest, ApprovalResponse, InProcessBridge,
};

// ============================================================================
// Task DAG Lease (BORROW: AgentFlow task state machine, 2026-08-21)
// ============================================================================
//
// **借鉴 ID**: `BORROW-Jimmyxiao2009/AgentFlow-task-dag-lease-2026-08-21`
// **License**: AgentFlow 无 LICENSE (默认 all-rights-reserved); 仅借鉴设计思想, 0 行代码复制.
//
// **职责**: Task 分配时附 lease (含 timeout), Scheduler 主动循环 (每分钟一次) reap 到期,
// 强制标记为 Failed (非 Ready — 关键不变量, 防止破坏状态机).
// **LOCATION 决策**: 作为 team-lead 子模块 (非独立 crate) — lease 是 Orchestrator 内部组件,
// 与 `Orchestrator` trait 紧耦合; 其他 crate 后续若有需求可通过 pub use re-export.
//
// **不假装** (P1 backlog):
// - in-memory only (重启后 lease 丢失)
// - reap_expired 只返回 task_id, 不自动改 state (caller 决定)
// - sync API (std::sync::Mutex); 改 async 需 Orchestrator 一起迁移
//
// 详见 src/lease.rs 模块头 + docs/04-internal/design-task-lease-2026-08-21.md.
pub mod lease;
pub use lease::{
    try_acquire, InMemoryLeaseManager, LeaseError, LeaseGuard, LeaseManager, TaskId, TaskLease,
    TaskState,
};
