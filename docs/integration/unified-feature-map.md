# Apeireth Unified Feature Map (Phase 0)

本矩阵用于指引桌面端能力与 Apeireth 原生后端的精确对接，坚持**现有后端优先、仅补薄表现层、Pattern 仅作设计参考**的原则。

| 产品功能 (Product Feature) | Apeireth 后端实现 (Backend Source) | 当前 UI 状态 (Current UI) | 状态评级 (Gap) | 实施行动 (Action) |
|---|---|---|---|---|
| **Chat & Streaming** | `companion_serve` (`POST /v1/chat/completions` OpenAI SSE) | `App.svelte` 对话流式打字机 | BACKEND READY / UI PARTIAL | 升级流式增量 Markdown/KaTeX 防抖渲染与流畅打字机体验 |
| **Reasoning Display** | `<think>...</think>` 推理流提取 | 基础文本切分后隐藏 `<think>` | PATTERN UX BETTER | 引入折叠/展开式思考卡片，展示思考耗时与过程 |
| **Tool Execution** | `apeireth-tool-runtime`, `ToolBridge`, `ToolRegistry` | 消息内简单文字指示 | BACKEND READY / UI MISSING | 结构化展示 ToolName、参数、执行中 Loading、执行结果摘要 |
| **Tool Approval** | `GET /v1/apeireth/approval-requests`, `POST /v1/apeireth/grant` | 纯说明性文字提示 | BACKEND READY / UI MISSING | 增加交互式审批抽屉/弹窗，一键 Approve/Reject 与风险等级说明 |
| **Tool Registry Browser** | `apeireth-tool-registry` 动态 schema / `/v1/tools` | 列表简易展示 | BACKEND READY / UI PARTIAL | 动态展现 30+ 原生工具 schema、参数定义与安全等级 |
| **Memory List & Search** | `SqliteMemoryStore`, `GET /v1/panel/memory/episodes` | `MemoryView.svelte` 简单列表 | BACKEND READY / UI PARTIAL | 支持 5 类标签 (事实/偏好/事件/反馈/参考) 筛选、1-10 重要度星级与搜索 |
| **Memory Graph** | `SqliteMemoryStore` (`factg-*`, `link-*`), `/v1/panel/graph` | 无 | BACKEND READY / UI MISSING | 增加知识图谱网络可视化视图 (实体-关系-客体交互探索) |
| **Experience & Reflection** | `ExperienceStore`, `recent_episodes(reflect-*)` | 无 | BACKEND READY / UI MISSING | 增加自成长与反思日志视图，呈现 AI 经验沉淀与成长轨迹 |
| **Provenance & Context** | `L0/L1 Essential Story`, `DeepRecall` 注入机制 | 无 | BACKEND READY / UI MISSING | 在消息详情中展示每轮对话注入的长期记忆与人格溯源 |
| **Goals Management** | `GoalService` (长期目标/里程碑/进度) | 无 | BACKEND READY / UI MISSING | 建立活动中心 Goals 视图，展示目标状态、里程碑与进度 |
| **Runs & Audit Trail** | `ActionStream`, `GET /v1/panel/audit` | 无 | BACKEND READY / UI MISSING | 建立活动中心 Runs 视图，展示工具审计历史与执行留痕 |
| **Workflows** | `apeireth-workflow` (DAG 状态机) | 无 | BACKEND READY / UI MISSING | 建立活动中心 Workflows 视图，展示工作流 DAG 节点与执行触发 |
| **Scheduled / Cron** | `apeireth-cron` (调度与定时) | 无 | BACKEND READY / UI MISSING | 建立活动中心 Scheduled 视图，展示定时策略与历史记录 |
| **MCP Integration** | `apeireth-mcp` (MCP 客户端协议) | 无 | BACKEND READY / UI MISSING | 展示 MCP 外部工具源连通状态与动态加载的工具 |
| **Companion Presence** | `RuntimeBrain`, `EmotionMemory`, `RhythmEstimate` | 仅静态头像 | BACKEND READY / UI MISSING | 后端推导表现态 (idle/thinking/reflecting 等)，前端渲染 Widget 与节律脉冲 |
| **Proactive Rhythm** | `GET /v1/apeireth/events` (SSE 涌现与主动推送) | 无 | BACKEND READY / UI MISSING | 监听 SSE 事件通道，展示涌现问候与桌面通知 |
| **Quick Window** | Tauri `index.html?window=quick` | 仅注册了空窗口声明 | BACKEND READY / UI PARTIAL | 实现 Spotlight/Alfred 式快捷呼出窗口 (Alt+Space)，支持快捷提问与状态概览 |
| **Native Tray & Shell** | Tauri 2 Shell (`src-tauri/src/lib.rs`) | 基础托盘与单窗控制 | BACKEND READY / UI PARTIAL | 增强系统托盘、快捷键、全局异常捕获与原生通知分发 |
| **Settings & OOBE** | 本地配置 + `GET /v1/models` 探活 | 基础 BaseURL/ApiKey 输入框 | PATTERN UX BETTER | 重构现代化设置视图与开箱向导 (OOBE)，支持 Persona/Subject 配置 |
| **Computer Use** | `enigo`, `xcap`, `agentos.exe` | 无 | DEFER | 暂不纳入本次融合主线，后续作为独立专项设计 |
