# Pattern Reference Parity Matrix (Phase 9)

本报告详细记录 Apeireth Unified Desktop 对 Pattern 原有产品功能的吸收与对齐状态。

| 模块 (Module) | Pattern 原有能力 (Pattern Reference) | Apeireth 统一桌面实现 (Apeireth Unified) | 对齐评级 (Status) | 说明 (Notes) |
|---|---|---|---|---|
| **流式对话渲染** | 基础 Markdown / KaTeX 流式输出 | 增量 Markdown/KaTeX 防跳动渲染 + 打字机光标 | **BETTER** | 完全保留并优化了平滑体验与错误重试机制 |
| **推理过程展示** | `<think>` 标签直接剥除或纯文本 | 折叠展开式思考卡片 (`reasoning-box`)，计时与流式动效 | **BETTER** | 支持深度思考过程实时解析与用时统计 |
| **执行步骤留痕** | ExecutionTimeline 伪步骤 | `ExecutionTimeline` 直连 ToolBridge 事件流 | **BETTER** | 展示真实工具调用、参数 schema 与执行回执 |
| **高危工具审批** | ReviewWindow 前端虚拟弹窗 | `ApprovalDrawer` 直连 `apreq-*` 与 `POST /v1/apeireth/grant` | **BETTER** | 严格基于 Master Token 与权限洋葱架构，真安全拦截 |
| **工具注册表** | McpView 工具列表 | `ToolRegistryView` 动态展现 30+ 原生工具与参数 Schema | **BETTER** | 支持 Tier 1-3 分级筛选与参数实时解析 |
| **记忆条目管理** | MemoryCard 9宫格简易卡片 | `MemoryView` 5 类标签筛选、重要度、子串搜索与追加 | **BETTER** | 直连 `SqliteMemoryStore` / `GET /v1/panel/memory/episodes` |
| **知识图谱浏览** | 纯静态示意图 | 实体-关系-客体三元组动态网络探索 | **BETTER** | 直连 `GET /v1/panel/graph` (`factg-*`, `link-*`) |
| **自成长与反思** | 无真实自成长 | 自成长反思日志流 (直连 `ExperienceStore` & `reflect-*`) | **BETTER** | 完整呈现 CompanionDaemon 周期深度反思与提炼成果 |
| **活动中心** | TasksView 混杂所有任务 | 统一活动中心 (`Goals` / `Runs` / `Workflows` / `Scheduled`) | **BETTER** | 严格保持 Apeireth 原生领域语义划分，不造假层 |
| **伴随体表现层** | CompanionWidget 悬浮窗口 | `CompanionWidget` 7 种表现态 (idle/thinking/working/reflecting 等) | **BETTER** | 表现态 100% 由后端信号推导，支持 SSE 主动涌现问候 |
| **快捷呼出窗口** | QuickWindow 简易浮窗 | `QuickWindowView` (Spotlight 式快捷提问、常用指令与全键盘支持) | **EQUIVALENT** | Alt+Space / index.html?window=quick 快捷激活 |
| **设置与自检** | SettingsView 多页面 | `SettingsView` (分区配置、Master Token 隔离、端点延迟自测) | **BETTER** | 结构清晰，提供实时连通性与延迟测速 |
| **Computer Use** | agentos.exe / enigo / xcap | 明确暂缓 (DEFERRED) | **DEFERRED** | 按规范不反向侵入旧架构，后续专项独立演进 |
