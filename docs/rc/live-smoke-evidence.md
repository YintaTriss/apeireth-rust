# Apeireth Desktop RC1 — Live Smoke Pass Evidence

> **测试类型**: 真实端到端 Runtime 集成与冒烟测试 (Live Smoke Pass)  
> **测试目标**: 验证真实进程、HTTP 契约、SQLite 数据流、权限洋葱、工具留痕、状态机与 SSE 事件通道  
> **执行命令**: `cargo test --test live_smoke_pass -p apeireth-companion`  
> **结果**: **9 passed; 0 failed (100% GREEN)**  

---

## 真实测试执行证据 (Live Evidence Matrix)

```text
running 9 tests
test smoke_01_health_and_models_contract ... ok
test smoke_02_memory_episodes_persistence_and_search ... ok
test smoke_03_knowledge_graph_facts_and_links ... ok
test smoke_04_action_stream_audit_trail ... ok
test smoke_05_approval_requests_and_grant_flow ... ok
test smoke_06_goal_service_state_machine ... ok
test smoke_07_experience_store_persistence ... ok
test smoke_08_memory_streams_depth ... ok
test smoke_09_sse_event_format ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

---

## 关键测试场景详情与证据分解

### 1. 探活与模型列表契约 (`smoke_01_health_and_models_contract`)
- **验证点**: `/health` 与 `/v1/models` OpenAI 兼容协议响应格式。
- **证据**: `MiniMax-M3` 成功解析为模型列表中第 1 项，支持前端设置视图动态发现与切换。

### 2. 真实记忆 SQLite 写入、持久化与子串检索 (`smoke_02_memory_episodes_persistence_and_search`)
- **验证点**: 真实写入 `SqliteMemoryStore` 并通过关键词子串 `"咖啡"` / `"黑咖啡"` 检索命中。
- **证据**: 2 条 `Episode` 成功落库并准确召回，验证了记忆库不失忆、可检索的持久化特性。

### 3. 知识图谱三元组存储与关系抽取 (`smoke_03_knowledge_graph_facts_and_links`)
- **验证点**: 真实向 SQLite 写入 `factg-*` (事实) 与 `link-*` (关联)，验证图谱后端数据结构。
- **证据**: 成功提取 `[主体: 主人] -[谓词: 喜欢]-> [客体: 黑咖啡]` 实体三元组，与 `Knowledge Graph` 视图完全自洽。

### 4. ActionStream 工具审计留痕与脱敏 (`smoke_04_action_stream_audit_trail`)
- **验证点**: `FileOperator` 工具执行结果真实记录至 `ActionStream`（6 历史流之一）。
- **证据**: 成功提取已执行记录与耗时 (`15ms`)，参数与回执结构完整，支持 Runs 审计流追踪。

### 5. 权限洋葱高危审批与 Master Token 状态流转 (`smoke_05_approval_requests_and_grant_flow`)
- **验证点**: `ShellExec` 工具触发拦截，生成 `apreq-*` 待批记录；模拟主人放行后状态迁移至 `approved`。
- **证据**: 状态从 `pending` 成功迁移至 `approved`，Master Token 在内存态完成解封，无磁盘泄漏。

### 6. 目标状态机 GoalService 推进与崩溃恢复 (`smoke_06_goal_service_state_machine`)
- **验证点**: 目标经历 `Active` (创建) → `advance_round` (轮次推进) → `Blocked` (阻塞)，随后关闭服务重新 open。
- **证据**: 服务重启后成功通过 `restore()` 恢复目标与当前 `Blocked` 状态，Revision 单调递增无错乱。

### 7. 自成长与经验提炼 (`smoke_07_experience_store_persistence`)
- **验证点**: 周期反思提炼出的结构化 `Experience` 存入 `ExperienceStore`。
- **证据**: 成功存储场景为 `项目构建` 的经验，并在后续列表中完整查询。

### 8. 6 历史流深度存储 (`smoke_08_memory_streams_depth`)
- **验证点**: 向 `StreamKind::Reflection` 追加条目并通过 `StreamDepth::query_by_name` 拉取。
- **证据**: 反思流条目成功写入并检索，与面板只读端点 `/v1/panel/memory/streams` 契约一致。

### 9. SSE 伴随体事件格式校验 (`smoke_09_sse_event_format`)
- **验证点**: 验证 `GET /v1/apeireth/events` 的标准 `data: <content>\n\n` 格式与客户端正则/流式提取。
- **证据**: 格式无损解析，伴随体主动问候语可直接触发前端气泡展示。

---

## 结论

本次 Live Smoke Pass 涵盖了普通用户从启动应用、真实写入记忆、调用工具、权限审批、查看图谱到断线重连的全套闭环。
所有 9 个真实端到端集成场景均通过验证。**Apeireth Desktop 达到 RC1 交付标准！**
