# Apeireth 前端数据契约（companion :8090）

> **契约即真相**：本文档每个字段都标注了代码出处（`文件:行号`），与代码不一致时以代码为准并修正本文档。
> 核实日期：2026-08（基于工作区当前 HEAD，路由表 `companion_serve.rs:1718-1751`）。
> **W6 回写（2026-08-22）**：路由注册段已移至 `companion_serve.rs:1718-1751`（新增 `/v1/tools/list` 与 CorsLayer，行号整体后移）；新增 §1.4 工具注册表端点；差距表 G3 / G7 已修。handler 行号：health `:2034`、list_models `:2043`、tools_list `:2053`、chat `:1066`、grant `:1935`、approval-requests `:1927`、events `:998`、test-event `:990`。
> 适用范围：`frontend/companion-desktop`（Svelte 5 + Tauri 2 薄壳）↔ `crates/apeireth-companion/examples/companion_serve.rs`（HTTP/SSE，默认 `:8090`）。

---

## 0. 总览：薄壳三层架构

```
┌─────────────────────────────────────────────────────────────┐
│ Tauri 2 shell (companion-desktop)                           │
│   薄壳：窗口/系统集成；无本地业务逻辑                         │
│   apiKey / masterToken 只存内存，永不落盘                     │
│   (runtime.ts:126-178, localStorage 写入前剔除)              │
├─────────────────────────────────────────────────────────────┤
│ Svelte 5 UI                                                 │
│   App.svelte / ConversationsView / MemoryView /             │
│   ToolsView / ActivityView / SettingsView                   │
│   └── src/lib/runtime.ts  (fetcher 层, 唯一数据入口)         │
│   └── src/lib/types.ts    (前端侧类型)                       │
├────────────────────── HTTP / SSE ───────────────────────────┤
│ companion_serve (:8090, env PORT 可改, serve.rs:1442-1445)   │
│   axum Router (companion_serve.rs:1677-1693)                │
│   ├── OpenAI 兼容主链路  /v1/chat/completions               │
│   ├── 权限洋葱          /v1/apeireth/grant + approval-requests│
│   ├── SSE 事件频道      /v1/apeireth/events                 │
│   └── 只读面板          /v1/panel/* (apeireth-api            │
│                         panel_readonly.rs, nest_service)     │
│   同进程常驻: CompanionDaemon (做梦/反思/涌现, select! 交替)   │
└─────────────────────────────────────────────────────────────┘
```

要点：

- **鉴权**：所有 handler 均**不校验** `Authorization` header；启动横幅自述 "Key 任意非空"（`companion_serve.rs:1836` 横幅段）。唯一凭证检查在 `POST /v1/apeireth/grant` 的 body 字段 `master_token`（对比 env `APEIRETH_MASTER_TOKEN`，`:1955-1967`）。服务绑定 `0.0.0.0`（`:1753`），**请勿暴露到局域网之外**。
- **CORS（W6 新增）**：`CorsLayer` 放开本地跨域（Any origin，GET/POST/OPTIONS + content-type/authorization/x-apeireth-continuity，`:1736-1750`，与 `apeireth-api/src/server.rs` R27 同款），供 companion-desktop dev 服（:5199）与 Tauri webview 跨域调用。
- **会话标签**：`X-Apeireth-Continuity` header，缺省回落启动时的 `subject`（默认 `companion-main`，`:1045-1050`、`:1449`）。它是日志/目标锚点，**不改变记忆归属**（记忆会话统一 `"me"`，`:23` 注释、`:206`）。
- **模型**：env `APEIRETH_LLM_MODEL` 可覆盖，缺省 `MiniMax-M3`（`:83`、`:93-109`）。

### 端点总表（18 条路由 = 15 数据端点 + 3 静态页路由）

| # | 方法 | 路径 | handler 出处 | 用途 |
|---|------|------|--------------|------|
| 1 | GET | `/` | `companion_serve.rs:1984` | 内置聊天页（`assets/chat.html`，编译期内嵌） |
| 2 | GET | `/health` | `:2034` | 存活探针 |
| 3 | GET | `/v1/models` | `:2043` | OpenAI 兼容模型列表 |
| 3b | GET | `/v1/tools/list` | `:2053` | 工具注册表只读投影（W6 新增，修 G7，见 §1.4） |
| 4 | POST | `/v1/chat/completions` | `:1066` | 对话主链路（非流式 + SSE 透传流式） |
| 5 | POST | `/v1/apeireth/grant` | `:1935` | 主人 master token 授权（PermissionPack） |
| 6 | GET | `/v1/apeireth/approval-requests` | `:1927` | 待批授权请求（仅 pending） |
| 7 | GET | `/v1/apeireth/events` | `:998` | SSE 事件频道（涌现问候推送） |
| 8 | POST | `/v1/apeireth/test-event` | `:990` | 开发用 SSE 链路验证 |
| 9 | GET | `/panel` | `:1989` | Web 面板 v2 入口页（内嵌 HTML） |
| 10 | GET | `/panel/:asset` | `:1994` | 面板静态资产（白名单 8 个文件名，其余 404） |
| 11 | GET | `/v1/panel/sessions` | `panel_readonly.rs:71` | 会话列表 |
| 12 | GET | `/v1/panel/sessions/:id/timeline` | `:109` | 会话时间线 |
| 13 | GET | `/v1/panel/memory/streams` | `:158` | 6 历史流查询 |
| 14 | GET | `/v1/panel/memory/episodes` | `:207` | 记忆条目列表 + 子串搜索 |
| 15 | GET | `/v1/panel/graph` | `:282` | 图谱事实/链接 |
| 16 | GET | `/v1/panel/approvals` | `:328` | 授权请求（全状态，chain 去重） |
| 17 | GET | `/v1/panel/audit` | `:385` | 工具调用审计留痕 |

路由注册出处：`companion_serve.rs:1718-1751`（`/v1/panel` 子树经 `nest_service` 挂 `panel_router`，`panel_readonly.rs:55-65`；CorsLayer 挂于 `:1736-1750`）。

---

## 1. 基础端点

### 1.1 `GET /` — 内置聊天页

- 返回 `text/html`，内容为编译期内嵌的 `assets/chat.html`（`companion_serve.rs:1849-1851`）。
- 零依赖演示页，与 companion-desktop 无关；前端不需要消费。

### 1.2 `GET /health` — 存活探针

handler：`companion_serve.rs:1899-1906`。响应 `200 application/json`：

```json
{
  "status": "ok",
  "service": "apeireth-companion-serve-v4",
  "version": "<CARGO_PKG_VERSION>",
  "features": ["persistent_memory", "daemon_resident", "dream_llm_summarizer", "utterance_llm", "constitution_llm_judicator", "memory_injection", "today_summary", "tool_bridge_all", "openai_compat", "companion_app", "l0_identity", "l1_essential_story"]
}
```

| 字段 | 类型 | 出处 |
|------|------|------|
| `status` | string，恒 `"ok"` | `:1901` |
| `service` | string，恒 `"apeireth-companion-serve-v4"` | `:1902` |
| `version` | string，编译期 crate 版本 | `:1903`（`env!("CARGO_PKG_VERSION")`） |
| `features` | string[12]，能力标签 | `:1904` |

**注意**：响应中**没有** `core` / `provider` 子对象（见 §9 差距表「意外发现」）。
前端消费：`checkHealth` / `checkHealthDetailed` 只看 `res.ok`，不读 body（`runtime.ts:195-227`）。

### 1.3 `GET /v1/models` — 模型列表

handler：`companion_serve.rs:2042-2047`。响应：

```json
{
  "object": "list",
  "data": [{"id": "MiniMax-M3", "object": "model", "created": 0, "owned_by": "minimax"}]
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `object` | string，恒 `"list"` | `:1910` |
| `data[].id` | string | 当前生效模型名（`model()`，`:100-109`；env `APEIRETH_LLM_MODEL` 可覆盖） |
| `data[].object` | string，恒 `"model"` | `:1911` |
| `data[].created` | number，恒 `0` | `:1911`（占位，非真实时间） |
| `data[].owned_by` | string，恒 `"minimax"` | `:1911`（硬编码；换 provider 也不会变，**不要据此判断真实 provider**） |

前端消费：`listModels` 只取 `data[].id`（`runtime.ts:331-337`）。

### 1.4 `GET /v1/tools/list` — 工具注册表只读投影（W6 新增）

handler：`companion_serve.rs:2053-2068`。数据源 = `tools_schema(st.bridge.registry)`（与 chat 主链路上发给 LLM 的同一真表，`:472-493`），只读、无执行语义变更。响应 `200`：

```json
{
  "object": "list",
  "count": 34,
  "tools": [{"name": "recall_memory", "description": "查主人长期记忆", "args_schema": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}]
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `object` / `count` | string / number | 恒 `"list"` / 工具数 |
| `tools[].name` | string | 注册表真名（`ToolRegistry::list()`，`registry.rs:124`） |
| `tools[].description` | string | 手写 schema 描述；未覆盖的工具为通用描述（`:482-491`） |
| `tools[].args_schema` | object | JSON Schema 参数表（`tools_schema` 的 `parameters` 原样投影） |

前端消费：`fetchTools` 首选项命中（`runtime.ts:845-852`），`ToolsView` 工具列表与 `checkHealthDetailed` 探针 #5（`runtime.ts:292-309`）恢复真实状态。**匹配 ✓（W6 修复后）**。

---

## 2. 对话主链路 `POST /v1/chat/completions`

handler：`companion_serve.rs:1040-1363`。OpenAI 兼容请求，服务端在调用 LLM 前做记忆注入、滚动摘要、工具桥循环。

### 2.1 请求

请求体反序列化为 `OpenAiChatRequest`（`crates/apeireth-api/src/protocol_handlers.rs:204-219`）：

```json
{
  "model": "MiniMax-M3",
  "messages": [{"role": "user", "content": "你好"}],
  "stream": false,
  "temperature": 0.6,
  "max_tokens": 8192
}
```

| 字段 | 类型 | 必填 | 说明 / 出处 |
|------|------|------|--------------|
| `model` | string | 是 | 服务端实际忽略其路由语义，恒用 `model()` 选 pipeline（`:1195`、`:1244`；`select_pipeline` 恒返默认，`:174-176`） |
| `messages` | array | 是 | 元素为 `OpenAiChatMessage`：`{role: string, content: string|array, tool_calls?, tool_call_id?}`（`protocol_handlers.rs:223-231`） |
| `stream` | bool | 否，默认 `false` | `true` → SSE 透传（见 2.3） |
| `temperature` | number? | 否 | 客户端值**不生效**：服务端恒以 `0.6` 调 LLM（`:1197`、`:1246`） |
| `max_tokens` | number? | 否 | 客户端值优先，clamp 到 `[256, 16384]`；未给时取 env `APEIRETH_MAX_TOKENS`，默认 8192（`:201-204`、`:1159-1168`） |
| `stop` / `tools` / `tool_choice` | — | 否 | 结构体接受但服务端覆盖：转发给 LLM 时恒带全量 `tools` + `tool_choice:"auto"`（`:1201-1202`、`:1250-1251`） |

请求 header：

| header | 说明 / 出处 |
|--------|--------------|
| `X-Apeireth-Continuity` | 可选会话标签；空/缺省 → 启动 subject（`:1045-1050`）。前端在 `streamChat` 里当 `sessionId` 非空时发送（`runtime.ts:366-368`） |
| `Authorization` | 服务端不校验；可发任意非空 Bearer |

服务端副作用（前端可感知）：每条对话都会喂节律 + 重置做梦安静期（`:1053`），并可能触发异步记忆提炼（fire-and-forget，`:1355-1360`）。

### 2.2 非流式响应（`stream=false`，默认）

经过「记忆注入 → LLM+工具循环（上限 `MAX_TOOL_ROUNDS = 5`，`:201`、`:1326-1329`）→ CoT 剥离」后返回（JSON 构造处：`:1332-1351`）：

```json
{
  "id": "chatcmpl-apeireth-3f2c…",
  "object": "chat.completion",
  "created": 1724900000,
  "model": "MiniMax-M3",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "主人，本座在。"},
    "finish_reason": "stop"
  }],
  "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
  "x_apeireth": {
    "continuity": "companion-main",
    "tool_rounds": 2,
    "tools_executed": ["[recall_memory] 已执行"],
    "reasoning_content": "<think>…</think>",
    "features": ["memory_injection", "today_summary", "tool_bridge", "daemon_resident", "memory_extractor", "l0_identity", "l1_essential_story", "cot_extraction"],
    "note": "Apeireth 伙伴主链路: …"
  }
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `id` | string | `chatcmpl-apeireth-{uuid v4}`（`:1333`） |
| `object` | string，恒 `"chat.completion"` | `:1334` |
| `created` | number，unix 秒 | `:1335` |
| `model` | string | 当前模型名（`:1336`） |
| `choices[0].message.content` | string | 最终可见文本（已剥离 CoT，`:1264-1265`）；工具循环达上限时为固定提示语（`:1327`） |
| `choices[0].finish_reason` | string，恒 `"stop"` | `:1340`（即使工具循环截断也是 `"stop"`，**不可据此判断完整性**） |
| `usage.*` | number，恒 `0` | `:1342`（占位，**不做计费/用量依据**） |
| `x_apeireth.continuity` | string | 本次生效的 continuity 标签（`:1344`） |
| `x_apeireth.tool_rounds` | number | 实际工具循环轮数（`:1345`） |
| `x_apeireth.tools_executed` | string[] | 形如 `"[{tool_name}] 已执行"`（`:1298`、`:1346`） |
| `x_apeireth.reasoning_content` | string | 剥离出的 CoT（含 `<think>…</think>` 或 `<!-- … -->` 标记原文；无 CoT 时为空字符串，`:1264-1268`、`:1007-1037`） |
| `x_apeireth.features` / `note` | string[8] / string | 静态说明（`:1348-1349`） |

**前端当前消费**：`chatOnce` 只读 `choices[0].message.content`（`runtime.ts:509-512`）；`x_apeireth` 整包**前端 0 消费**（`runtime.ts` 全文无 `x_apeireth` 引用）。

### 2.3 流式响应（`stream=true`）— SSE 透传

分支：`companion_serve.rs:1193-1237`。架构是**字节透传**：`stream_forward`（`protocol_handlers.rs`，`:1219-1224` 调用）把上游 MiniMax 的 SSE 原样转发，**跳过服务端工具循环**（`:1189` 注释明示 "tool loop 跳过"）。

- 响应 `Content-Type: text/event-stream`，帧格式即上游 OpenAI 兼容 chunk：

```
data: {"choices":[{"delta":{"content":"主人","tool_calls":null},"finish_reason":null}]}

data: {"choices":[{"delta":{"content":"，"}}]}

data: [DONE]
```

- CoT：MiniMax M3 无独立 `reasoning_content` 字段，CoT 嵌在 `delta.content` 的 `<think>…</think>` / `<!-- … -->` 标记内，**边界标记可能跨 chunk 切分**（`:988-1006` 注释）。前端 `streamChat` 维护字符串缓冲解析 `data:` 行，并按 `delta.content` / `delta.reasoning_content` / `delta.tool_calls` 三路分发（`runtime.ts:401-485`）。
- 工具调用：`delta.tool_calls` 仍在流里（前端可见、可展示），但**服务端不执行**——工具结果不会回到 LLM，模型可能"自称执行了工具"而实际未执行（`:1187-1191` 明示为 v1.5 known limit）。前端在 `finish_reason` 到达时把 running 工具一律标记 `succeeded`（`runtime.ts:473-482`）——**这是前端侧已知的不精确语义，不是后端保证**。
- 帧尾：上游 `[DONE]` 由前端识别终止（`runtime.ts:405-407`）。

### 2.4 错误形态

| 场景 | HTTP | body | 出处 |
|------|------|------|------|
| LLM 三连失败（限流/空响应） | 503 | `{"error": {"message": "模型服务暂时不可用 (MiniMax 限流) — 本座已尽力, 请过 10-30 秒再试"}}` | `:1253-1258` |
| 流式请求序列化失败 | 500 | `{"error": {"message": "stream serialize: …"}}` | `:1206-1213` |
| 流式上游转发失败 | 502 | `{"error": {"message": "stream forward: …"}}` | `:1228-1235` |
| 请求体非法 JSON / 缺 `model`/`messages` | 400/422 | axum `Json` 提取器默认错误（非自定义形状） | axum 框架行为 |

统一特征：错误 body 均为 `{"error": {"message": string}}`（chat 链路）或 `{"error": string}`（grant/panel，见下文）——**两种错误形状并存，前端解析需兼容**。

---

## 3. 权限洋葱端点

### 3.1 `POST /v1/apeireth/grant` — 主人授权

handler：`companion_serve.rs:1800-1846`。主人带 master token 直接授予限时 PermissionPack；AI 不接触 token。

请求：

```json
{"tool": "FileOperator", "hours": 24, "master_token": "<env APEIRETH_MASTER_TOKEN>"}
```

| 请求字段 | 类型 | 说明 / 出处 |
|----------|------|--------------|
| `tool` | string | 必填，trim 后非空，否则 400（`:1801-1812`） |
| `hours` | number | 可选，默认 1，clamp `[1, 720]`（30 天）（`:1813-1818`） |
| `master_token` | string | 对比 env `APEIRETH_MASTER_TOKEN`；env 未设（空）或不匹配一律 403（`:1819-1832`） |

响应 `200`（`:1841-1845`）：

```json
{"ok": true, "tool": "FileOperator", "hours": 24, "note": "已按权限洋葱授权 (PermissionPack); 到期自动失效"}
```

错误：`400 {"error": "需要 tool (工具名)"}`（`:1808-1810`）；`403 {"error": "master token 不匹配 (主人授权权在主人手里)"}`（`:1827-1830`）。

前端消费：`grantToolPermission`（`runtime.ts:886-913`），body 字段名一致（`tool` / `hours` / `master_token`）；错误读 body 的 `error` 字段。**匹配 ✓**。

### 3.2 `GET /v1/apeireth/approval-requests` — 待批授权请求

handler：`companion_serve.rs:1792-1796` → `apeireth_companion::approval_requests::pending_json`（`crates/apeireth-companion/src/approval_requests.rs:229-242`）。AI 工具被拒时产生 `apreq-*` 请求（append-only episodes），此端点**只返回 pending**。

响应 `200`：

```json
{
  "count": 1,
  "requests": [{
    "id": "apreq-9f8c…",
    "tool": "ShellExec",
    "args_preview": "{\"cmd\":\"dir\"}",
    "reason": "需要主人批准",
    "created_at": 1724900000
  }],
  "note": "主人批准后, 对话里让本座重试即可"
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `count` | number | pending 条数（`approval_requests.rs:232`） |
| `requests[].id` | string | `apreq-{uuid}`（`:233`；结构体 `:25-26`） |
| `requests[].tool` | string | 工具名（`:234`） |
| `requests[].args_preview` | string | 参数 JSON 截断 200 字符（`:54-58`、`:235`） |
| `requests[].reason` | string | 拒绝理由（`:236`） |
| `requests[].created_at` | number，unix 秒 | `:237` |
| `note` | string | 固定提示（`:240`） |

**注意**：此处的 `requests[]` **不含** `chain` / `rev` / `status` / `updated_at`（`pending_json` 只投影 5 个字段）——与 `/v1/panel/approvals`（§4.6）的全字段不同。去重语义：同 chain 取最新 rev（`list()`，`approval_requests.rs:131-153`）。

**⚠ 前端消费不匹配**：`fetchApprovalRequests` 把响应当**裸数组**解析（`Array.isArray(list)`，`runtime.ts:859-871`），而后端返回的是**对象** `{count, requests, note}` → 恒返 `[]`，审批请求在 UI 永不显示。见 §5 差距表 G1。

---

## 4. 只读面板数据端点 `/v1/panel/*`

子路由：`panel_readonly.rs:55-65`（7 个 GET，全部只读，数据源 `SqliteMemoryStore` 真持久层）。
通用约定：`limit` 参数一律 clamp 到 ≤ 200（`MAX_LIMIT`，`:39`、`:41-43`）；错误统一 `{"error": string}`。

### 4.1 `GET /v1/panel/sessions` — 会话列表

handler：`panel_readonly.rs:71-102`。响应：

```json
{
  "count": 1,
  "sessions": [{
    "id": "me",
    "started_at": 1724900000,
    "last_active_at": 1724900100,
    "closed_at": null,
    "episode_count": 42
  }]
}
```

| 字段 | 类型 | 可空 | 说明 / 出处 |
|------|------|------|--------------|
| `count` | number | 否 | `:100` |
| `sessions[].id` | string | 否 | `SessionRecord.id`（`session_note.rs:22`），投影 `:85` |
| `sessions[].started_at` | number，unix 秒 | 否 | `:86`（`session_note.rs:25`） |
| `sessions[].last_active_at` | number，unix 秒 | 否 | `:87`（`session_note.rs:27`）；**列表按此字段倒序**（`:92-97`） |
| `sessions[].closed_at` | number \| null | **是** | `:88`（`session_note.rs:29`，`None` = 会话进行中） |
| `sessions[].episode_count` | number | 否 | 该会话 episode 数（`:83`、`:89`） |

错误：`500 {"error": "list sessions: …"}`（`:74-79`）。
前端消费：`fetchBackendSessions` 字段全对（`runtime.ts:729-735`）；`checkHealthDetailed` 探活（`runtime.ts:254`）。**匹配 ✓**。

### 4.2 `GET /v1/panel/sessions/:id/timeline` — 会话时间线

handler：`panel_readonly.rs:109-141`。query：`limit`（默认 100，≤200，`:104-107`、`:114`）。

```json
{
  "session_id": "me",
  "count": 2,
  "episodes": [{
    "id": "ep-1",
    "timestamp": 1724900000,
    "role": "user",
    "content": "你好",
    "session_id": "me"
  }]
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `session_id` | string | 回显路径参数（`:133`） |
| `count` | number | `:133` |
| `episodes[].id / timestamp / role / content / session_id` | string / number(unix 秒) / string / string / string | 投影自 `Episode`（`apeireth-core/src/memory.rs:17-28`），`:122-128`。倒序由存储层保证（`:115` 注释） |

错误：`500 {"error": "timeline: …"}`（`:136-139`）。
前端消费：`fetchSessionTimeline`（`runtime.ts:738-750`），字段一致。**匹配 ✓**。

### 4.3 `GET /v1/panel/memory/streams` — 6 历史流

handler：`panel_readonly.rs:158-196`。query 参数（`StreamParams`，`:147-156`）：

| 参数 | 默认 | 说明 |
|------|------|------|
| `kind` | `"action"` | 枚举：`thought` / `proposal` / `action` / `relation` / `evolution` / `reflection`（`StreamKind::from_str`，`apeireth-memory/src/lib.rs:236-249`）；非法值 → 400 |
| `subject` | `"companion-main"` | 主体 id（`DEFAULT_SUBJECT`，`:36`）。审计条目的 subject 约定为 `tool_call:{工具名}`（测试 `:587-590`） |
| `limit` | 50 | ≤200 |
| `since` | 无 | 起始时间 unix 秒 |

响应：

```json
{
  "kind": "action",
  "subject": "companion-main",
  "count": 1,
  "entries": [{
    "id": "act-1",
    "subject_id": "tool_call:WebSearch",
    "session_id": null,
    "created_at": 1724900000,
    "payload": {"tool_name": "WebSearch", "…": "…"},
    "source": "ai_generated",
    "tags": []
  }]
}
```

| 字段 | 类型 | 可空 | 说明 / 出处 |
|------|------|------|--------------|
| `kind` / `subject` / `count` | string / string / number | 否 | 回显 + 条数（`:183-186`） |
| `entries[].id` | string | 否 | `HistoryEntry.id`（`append_only.rs:42`），投影 `:171` |
| `entries[].subject_id` | string | 否 | `append_only.rs:44`；`:172` |
| `entries[].session_id` | string \| null | **是** | `append_only.rs:48`；`:173` |
| `entries[].created_at` | number，unix 秒 | 否 | `append_only.rs:50`；`:174` |
| `entries[].payload` | any（自由 JSON） | 否 | `append_only.rs:52`；`:175`。action 流的 payload 即 `ToolCallRecord`（见 §4.7） |
| `entries[].source` | string | 否 | `ai_generated` / `human_overridden` / `council_synthesized`（`append_only.rs:53-54`）；`:176` |
| `entries[].tags` | string[] | 否 | `append_only.rs:56`；`:177` |

注意：`HistoryEntry` 的 `subject_rev` / `tombstoned_at` **不投影**到响应（`:170-178` 未含）。
错误：`400 {"error": "streams: …"}`（`:191-194`）。

**⚠ 前端消费不匹配**：`fetchMemoryStreams` 期望 `{streams: Record<string, episode-like[]>}`（`runtime.ts:757`），与后端实际形状 `{kind, subject, count, entries}` 完全不符 → 恒返 `{}`。见 §5 差距表 G2。（当前 `MemoryView.svelte` 导入了该函数但主流程未调用，`MemoryView.svelte:32`、`116-132`。）

### 4.4 `GET /v1/panel/memory/episodes` — 记忆条目搜索

handler：`panel_readonly.rs:207-257`。query（`EpisodeParams`，`:198-205`）：`session`（按会话过滤）、`q`（内容子串，**大小写不敏感**，`:234-241`）、`role`、`limit`（默认 50，≤200）。
带 `q` 时后端先放大拉取窗口到 `limit*5`（上限 1000）再内存过滤（`:213-217`）。

```json
{
  "count": 1,
  "episodes": [{
    "id": "9f8c…",
    "timestamp": 1724900000,
    "role": "assistant",
    "content": "主人喜欢深烘咖啡",
    "session_id": "me"
  }]
}
```

字段同 §4.2 的 `episodes[]`（投影 `:244-250`，源 `Episode`，`memory.rs:17-28`）。
错误：`500 {"error": "episodes: …"}`（`:227-232`）。
前端消费：`fetchMemoryEpisodes`（`runtime.ts:775-788`）。**匹配 ✓**（`MemoryView.svelte:125` 用于全部/搜索）。

### 4.5 `GET /v1/panel/graph` — 图谱事实/链接

handler：`panel_readonly.rs:282-316`。直读 `factg-*` / `link-*` 前缀 episodes 的 content JSON（`episodes_by_prefix`，`:272-280`；固定扫 `"me"` 会话最近 500 条）。
query（`GraphParams`，`:263-269`）：`subject` / `predicate` / `object`（均为**子串包含**匹配，`:287-293`）、`limit`（默认 100，≤200，facts 与 links 各自截断）。

```json
{
  "facts_count": 1,
  "links_count": 1,
  "facts": [{"id": "factg-1", "subject": "主人", "predicate": "喜欢", "object": "咖啡", "importance": 80}],
  "links": [{"id": "link-1", "from": "factg-1", "to": "ep-1", "weight": 0.9}]
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `facts_count` / `links_count` | number | `:310-311` |
| `facts[]` | object[] | **原样透传 episode content 解析出的 JSON**（`:294-302`）。形状由写入方决定：记忆提炼器 prompt 约定 `{subject, predicate, object, importance(1-10)}`（`companion_serve.rs:239`），`id` 与 episode id 一致（测试种子 `panel_readonly.rs:492`）。`importance` 数值范围无强制（测试种子用 80） |
| `links[]` | object[] | 同上；写入约定 `{id, from, to, weight}`（测试种子 `:507`）。**注意 links 不做 subject/predicate/object 过滤**（`:303-306` 只截 limit） |

错误：无自定义错误分支（存储失败时 `unwrap_or_default()` 静默返空，`:275`）。
**⚠ 前端消费不匹配**：`fetchGraphData` 把 facts/links 当 episode 形 `{id, timestamp, role, content, session_id}` 解析（`runtime.ts:795-799`），`MemoryView` 渲染 `fact.content`（`MemoryView.svelte:301`、`:318`）→ 实际为 `undefined`，**图谱页显示空白文本**。见 §5 差距表 G3。

### 4.6 `GET /v1/panel/approvals` — 授权请求（全状态）

handler：`panel_readonly.rs:328-372`。query（`ApprovalParams`，`:322-326`）：`status`（`pending` / `approved` / `expired` 等字符串精确匹配；缺省全部）。
去重：按 `chain` 取最新 `rev`（`:333-352`，语义对齐 `approval_requests.rs:131-153`），再按 `created_at` 倒序（`:362-367`）。

```json
{
  "count": 1,
  "requests": [{
    "id": "apreq-7c1e…",
    "chain": "apreq-3b9a…",
    "rev": 2,
    "tool": "ShellExec",
    "args_preview": "{\"cmd\":\"dir\"}",
    "reason": "需要主人批准",
    "status": "approved",
    "created_at": 1724900000,
    "updated_at": 1724900060
  }]
}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `count` / `requests[]` | number / object[] | `:370`；元素为 `ApprovalRequest` 全字段 JSON（`approval_requests.rs:25-41`：`id, chain, rev, tool, args_preview, reason, status, created_at, updated_at`），经 `serde_json::from_str::<Value>` 原样透传（`:339`） |
| `requests[].status` | string | 注释约定 `pending / approved / expired`（`approval_requests.rs:37`）；orchestrator bridge 回写还可能产生 `rejected`（`:206-209`） |

错误：无自定义分支（读取失败静默空列表，`:336`）。
前端消费：**companion-desktop 未消费此端点**（前端用 §3.2）；静态面板页 `approvals.html` 消费。**匹配 ✓（对面板而言）**。

### 4.7 `GET /v1/panel/audit` — 工具调用审计

handler：`panel_readonly.rs:385-449`。query（`AuditParams`，`:378-383`）：`tool`（按工具名过滤，走 `RecordStore::list_for_tool`，`:391-400`）、`limit`（默认 50，≤200）。
无 `tool` 时读 `action_stream` 最近条目并把 payload 反序列化为 `ToolCallRecord`（`:401-430`）。**响应按时间倒序（最新在前）**（`:435` `.rev()`）。

```json
{
  "count": 1,
  "records": [{
    "id": "call-1",
    "tool_name": "WebSearch",
    "started_at_ms": 1724900000000,
    "finished_at_ms": 1724900000500,
    "duration_ms": 500,
    "status": "success",
    "success": true,
    "call_content": {"query": "rust axum"},
    "masked": false
  }]
}
```

| 字段 | 类型 | 可空/省略 | 说明 / 出处（`ToolCallRecord`，`apeireth-tool-runtime/src/record.rs:43-80`） |
|------|------|-----------|----------------|
| `id` | string | 否 | UUID v4（`:44-45`） |
| `tool_name` | string | 否 | `:46-47` |
| `caller_signature` / `caller_type` / `request_ip` / `source_node` | string | **None 时字段整个省略**（`skip_serializing_if`） | `:48-59` |
| `started_at_ms` / `finished_at_ms` / `duration_ms` | number，**毫秒** | 否 | `:60-65` |
| `status` | string | 否 | `"success" / "failure" / "timeout"`（`:66-67`） |
| `success` | bool | 否 | `:68-69` |
| `call_content` | any | 否，但会被脱敏替换 | `:70-71`；当 `masked=true`，响应里替换为字符串 `"[masked by audit] (隐私已脱敏)"`（`panel_readonly.rs:437-442`） |
| `return_content` / `error_text` | any / string | **None 时省略** | `:72-77` |
| `masked` | bool | 否 | 是否经 privacy mask（`:78-79`） |

错误：`500 {"error": "audit list_for_tool: …" / "memory conn: …" / "list action_stream: …"}`（`:394-428`）。

**⚠ 前端消费不匹配**：`fetchAuditLogs` 期望 `records[]` 含 `{id, timestamp, action, tool, status, detail}`（`runtime.ts:807-816`），实际字段是 `tool_name` / `started_at_ms` → `timestamp` 恒回落 `Date.now()`、`title` 恒 "操作记录"；且前端以 `status === 'failed' | 'error'` 判严重度，后端实际值是 `"failure"` → 失败记录显示为 info。见 §5 差距表 G4。

---

## 5. SSE 事件频道 `GET /v1/apeireth/events`

handler：`companion_serve.rs:972-986`。机制：`tokio::sync::broadcast::Sender<String>`（容量 64，`:1532`）→ 每个订阅者 `SseEvent::default().data(text)`。**当前帧只有 `data:` 行，无 `event:` / `id:` 字段**；`KeepAlive::default()` 周期性发送 `:` 开头的注释保活帧（`:985`）；订阅者落后（Lagged）直接跳过积压消息（`:980`）。

### 5.1 当前实际广播内容（三类 data 行共流）

SSE 管道现为**三类 `data:` 行共流**（v5，`companion_serve.rs:16-19`；presence 事件完整契约见 **§8.1**）：

**① legacy 纯文本行（涌现问候 / 测试事件）**：daemon 涌现 → `CompanionDelivery` → 出站前 PII 检测 + 脱敏（`daemon.rs:460-471`）→ `BroadcastSink` 加前缀广播（`daemon.rs:400-405`）：

```
data: [他说] 主人，夜深了，本座留意到你还在忙。

```

即：**纯文本**，格式为 `[他说] {自然语言问候}`，不是 JSON。测试事件（`POST /v1/apeireth/test-event`，`companion_serve.rs:964-969`）广播固定文本 `测试事件: 本座在 (SSE 链路验证)`，响应 `{"ok": true, "note": "已推送测试事件到 SSE"}`。

**② presence JSON 行（内心状态事件，v5 已接入）**：单行 JSON（serde 内部标签 `type` + `at` RFC3339），四类事件 `emotion` / `initiative` / `dream` / `memory_recall`，字段、生产点与频率纪律见 **§8.1**（事件定义 `presence.rs:145-163`）。其中 **dream 事件已推送**（生产点：presence 段读真库 `mem-dream-*` 增量，`companion_serve.rs:1813-1844`）；**反思（reflection）仍不推 SSE**——反思周期只写库 + stderr 日志 + 提炼经验，无 `events.send`（`companion_serve.rs:1855-1877`），保留此诚实标注。

**③ `presence_error` 兜底帧**：presence 事件序列化失败时的显式错误行 `{"type":"presence_error","error":"…"}`（`presence.rs:330-334`）——结构上不会触发，但前端解析器应容忍该 `type`。

分流规则（行首 `{` → JSON，否则按 legacy 文本）与消费者现状见 §8.1 与差距表 G5。

### 5.2 前端两个消费者的口径分裂（重要）

- `App.svelte` → `subscribeCompanionEvents`（`runtime.ts:1015-1070`）：把 `data:` 行当**纯文本**展示为主动问候（`App.svelte:455-460`）。**与当前后端匹配 ✓**。自带指数退避重连 2s→30s（`runtime.ts:1022`、`:1059-1060`）。
- `ActivityView.svelte:177-212`：`new EventSource(.../v1/apeireth/events)` 后对每帧 `JSON.parse`，期望 span 形 `{id, type, action, tool, summary, detail, status, ts, trace_id, span_id, kind}`。**v5 起频道为三类行共流**（§5.1）：legacy 纯文本行 parse 抛错被 catch 丢弃；presence JSON 行 parse 成功但非 span 形，落为通用「Agent 活动」条目（`ActivityView.svelte:195-206`）。该 span 形状对应 `agent_trace.rs:200-…` 的 `span_event_json`（`TraceRecorder`），但 **companion_serve 没有把 `TraceRecorder` 接到 `tx_events`**（全文件 grep 无引用）。见差距表 G5（§7）。

---

## 6. 静态 Web 面板页 `/panel`

- `GET /panel` → 内嵌 `assets/panel/index.html`（`companion_serve.rs:1854-1856`）。
- `GET /panel/:asset` → 白名单 8 个：`index.html / sessions.html / memory.html / graph.html / approvals.html / audit.html / panel.css / panel.js`，其余 404（`:1859-1897`）。
- 这是独立于 companion-desktop 的浏览器面板（数据走 §4 的 `/v1/panel/*`）。Tauri 壳内若要内嵌，直接 `iframe` / 系统浏览器打开即可，无额外契约。

---

## 7. 差距表：前端当前消费 vs 后端当前提供

> 范围：`frontend/companion-desktop` 现行代码（`runtime.ts` + 各 View）对 companion_serve :8090。
> 图例：✓ 匹配；⚠ 形状不匹配（静默退化）；✗ 端点不存在。

| # | 前端调用（出处） | 后端现状（出处） | 状态 | 实际影响 |
|---|------------------|------------------|------|----------|
| G1 | `fetchApprovalRequests` 期望**裸数组**（`runtime.ts:859-871`） | `/v1/apeireth/approval-requests` 返回 `{count, requests, note}` 对象（`approval_requests.rs:229-242`） | ⚠ | `Array.isArray` 失败恒返 `[]`，**待批授权永不显示**；`App.svelte:280` / `ToolsView.svelte:76` 受影响 |
| G2 | `fetchMemoryStreams` 期望 `{streams: Record<kind, episodes[]>}`（`runtime.ts:757-771`） | 返回 `{kind, subject, count, entries[]}`（`panel_readonly.rs:183-188`） | ⚠ | 恒返 `{}`；当前无 View 实际调用，属潜伏 bug |
| G3 | `fetchGraphData` 期望 episode 形 `{id,timestamp,role,content,session_id}`（`runtime.ts:795-799`） | facts/links 为图 JSON `{id,subject,predicate,object,importance}` / `{id,from,to,weight}`（`panel_readonly.rs:294-314`） | ✅ 已修（W6） | `fetchGraphData` 已适配图谱 JSON（组装 `subject · predicate · object` 可读文本，`timestamp=0` 由 MemoryView 隐藏时间行、改显重要度） |
| G4 | `fetchAuditLogs` 期望 `{id,timestamp,action,tool,status,detail}`（`runtime.ts:807-816`） | `ToolCallRecord`：`tool_name` / `started_at_ms` / `status:"failure"`（`record.rs:43-80`） | ⚠ | ActivityView 持久审计区时间恒为"现在"、标题恒"操作记录"、失败不显红（`ActivityView.svelte:154-165`） |
| G5 | `ActivityView` SSE 按 **JSON span** 解析（`ActivityView.svelte:181-193`）；`subscribeCompanionEvents` 按纯文本收（`runtime.ts:1047-1052`） | 频道现为**双 data 行共流**：纯文本 `[他说] …`（`daemon.rs:403`）+ presence JSON 行（emotion / initiative / dream / memory_recall，`presence.rs:145-163`，契约见 §8.1）；`TraceRecorder` 的 span JSON 仍未接线 | ⚠ | 两消费者均未按 `type` 分流 presence 行：App 侧会把 JSON 行当问候文本展示；ActivityView 侧 parse 成功但字段不符 span 形，落为通用「Agent 活动」条目 |
| G6 | `fetchCapabilities` → `GET /v1/apeireth/capabilities`（`runtime.ts:597-623`） | 路由不存在 | ✗→兜底 | 404 → `legacyCapabilityManifest()` 保守声明（`runtime.ts:693-726`）。**设计内降级**，但意味着 V2 能力全部 gate 关闭 |
| G7 | `fetchTools` → `/v1/tools/list` 再 `/v1/panel/tools`（`runtime.ts:845-852`）；`checkHealthDetailed` 探针 #5（`:292-309`） | **W6 已补 `GET /v1/tools/list`**（`companion_serve.rs:2053`，真注册表投影，见 §1.4）；`/v1/panel/tools` 仍不存在 | ✅ 已修（W6） | `ToolsView` 工具列表恢复真实数据；健康面板"工具注册表"恢复 online |
| G8 | `appendMemoryEpisode` → `POST /v1/memory/append`（`runtime.ts:921-940`） | 路由不存在（存在于 `v2_endpoints.rs:2222`，另一 runtime） | ✗ | MemoryView 手动记忆恒「写入记忆失败」（`MemoryView.svelte:134-158`） |
| G9 | `fetchOrgans` → `GET /v1/organs`（`runtime.ts:978-987`） | 路由不存在（`v2_endpoints.rs:2225`） | ✗ | catch 后恒返 `[]`（静默） |
| G10 | V2 mutation 族：`/v1/apeireth/sessions` CRUD、`/v1/apeireth/memory/episodes/:id`（update/forget/protect/unprotect）、`/v1/apeireth/grants(+revoke)`、`/v1/panel/traces(/:id)`（`runtime.ts:1178-1279`） | 全部不存在于 companion_serve 路由表 | ✗ | 由 legacy manifest 声明 unsupported → UI 禁用（`ToolsView.svelte:77`、`ActivityView.svelte:29` 的 `capabilitySupported` gate）。设计内，但 fetcher 真调会 404 |
| G11 | 后端提供 `x_apeireth` 扩展包（`companion_serve.rs:1343-1350`：continuity/tool_rounds/tools_executed/reasoning_content/features） | 前端 0 消费 | ⚠ 反向 | CoT/工具轮次在非流式链路对前端不可见（流式链路靠 delta 自行拼） |
| G12 | `/health` 提供 `version` + `features`（`:1903-1904`） | 前端只看 `res.ok`（`runtime.ts:215-227`） | ⚠ 反向 | 版本协商信息已存在但未被用于 UI 提示（见 §9.3） |

**意外发现（测试 ↔ 代码不一致）**：`crates/apeireth-companion/tests/no_key_runtime_smoke.rs:183-265`（非 `#[ignore]`）断言 companion_serve 的 `/health` 含 `core.status=="healthy"` 与 `provider.status=="unconfigured"` 子对象、且 `GET /v1/apeireth/capabilities` 返 200、`POST /v1/apeireth/sessions` 可用——**当前路由表（`companion_serve.rs:1677-1693`）没有这些路由，`health()` 也无 core/provider 字段**。该测试描述了另一版（规划或已回退的）serve；以当前代码构建 example 后运行此测试会失败。前端不应以该测试为契约依据。

---

## 8. 规划中（未接，0 装）

> 8.1 已接线（见下，2026-08-21 回写）；8.2 / 8.3 在 companion_serve 与 companion-desktop 两侧**仍均无实现**，前端开发严禁提前消费。

### 8.1 内心状态事件频道（presence）— ✅ 已接入（v5，2026-08-21）

> 状态更正：本节原标「接线中（占位）」，已滞后于代码。presence 四类事件**已真实接入** SSE 广播 `GET /v1/apeireth/events`。事件定义以 `crates/apeireth-companion/src/presence.rs` 为准，生产点以 `companion_serve.rs` 为准（本次回写前均已逐行核实）。

**线格式（双 data 行共流）**：同一条 SSE 流上现有两类 `data:` 行（`companion_serve.rs:16-19`、`presence.rs:5-8`）——

1. legacy 纯文本行：`[他说] {问候}`（`daemon.rs:403`）与测试事件行（`companion_serve.rs:967`），原样保留；
2. presence JSON 行：**单行 JSON**，serde 内部标签 `type` 平铺 + `at`（RFC3339 字符串）+ 负载字段（`presence.rs:145-163`；`to_json_line()`，`:330-334`，序列化失败兜底为 `{"type":"presence_error","error":…}`）。

帧仍**无 `event:` 字段**；前端分流规则 = 按行首是否为 `{`（或解析后看 `type`）区分两类行。

#### 事件：`emotion`（PAD 情绪快照）

生产点：daemon_loop presence 段每 tick 一条心跳（ticker 60s，`companion_serve.rs:1780`、`:1790-1793`）；主人消息到达后经 interactions 通道在 daemon_loop 再推一条（`:1891-1900`）。

```json
{"type":"emotion","at":"2026-08-21T08:30:00Z","pad":{"p":0.12,"a":0.05,"d":0.0},"dominant":"joy","intensity":0.46,"response_style":"friendly","tone":"礼貌克制, 谨慎而友好"}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `type` | string，恒 `"emotion"` | serde 内部标签（`presence.rs:153-156`） |
| `at` | string（RFC3339） | 推送端 `Utc::now()`（`:168-169`） |
| `pad` | `{p, a, d}` number，各 ∈ [-1, 1] | consciousness `EmotionEngine` 真引擎快照（`:170-171`） |
| `dominant` | string | `joy` / `sadness` / `anger` / `fear` / `surprise` / `disgust`（`BaseEmotion::as_str()`，`:172-173`） |
| `intensity` | number | PAD 距 baseline 的欧氏距离（`:174-175`） |
| `response_style` | string | 7 档：`warm` / `friendly` / `gentle` / `cautious` / `diplomatic` / `curious` / `professional`（`:176-177`、`:240-250`） |
| `tone` | string，**可省** | 三层器官语调（`AwakeCompanion::tone()`）；`Option`，None 时字段不出现（`:178-181`） |

#### 事件：`initiative`（开口决策）

生产点：presence 段读 `AwakeCompanion` 决策留痕（`companion_serve.rs:1794-1812`）。**spoke 每次都推；held 仅门控原因变化时推（按原因去抖，`:1778`、`:1805`）**。

```json
{"type":"initiative","at":"2026-08-21T08:30:00Z","outcome":"held","gate":"quiet_hours","gate_label":"安静时段"}
{"type":"initiative","at":"2026-08-21T08:30:00Z","outcome":"spoke","action":"问候"}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `outcome` | string | `spoke` / `held`（`presence.rs:189-190`、`:204-211`） |
| `gate` | string，held 时必现 | 13 种真实门控（snake_case，`:84-99`）：`sovereignty_frozen` / `user_quiet` / `quiet_hours` / `daily_limit` / `llm_budget` / `depth_low` / `rhythm_unknown` / `rhythm_veto` / `drive_low` / `emotion_low` / `council_veto` / `policy_inactive` / `gate_block` |
| `gate_label` | string，held 时必现 | 门控原因中文说明（`:103-119`、`:194-196`） |
| `action` | string，spoke 时必现 | 机制动作标签 `Action::label()`（`:197-200`）；完整话术由 `[他说]` 文本行送达，此处不重复 |

**契约稳定提示**：`gate` 的 serde 标签即线上值，「改标签 = 改前端契约」（`presence.rs:28-31`，单测锁定 `:409-431`）。

#### 事件：`dream`（做梦整合完成）

生产点：presence 段轮询真库 `mem-dream-*` 增量（`companion_serve.rs:1813-1844`）；排除 `mem-dream-thought-*`（思维链盘点，`:1823`）；`last_dream_ts` 起点 = 启动时刻 → **只报 serve 启动后新发生的做梦，旧做梦不重播**（`:1777-1779`、`:1824`）。

```json
{"type":"dream","at":"2026-08-21T08:30:00Z","merged_count":2,"summary_prefix":"【做梦摘要】主人在准备考试"}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `merged_count` | number | 本次做梦整合写回真库的条数（`presence.rs:218-219`） |
| `summary_prefix` | string | 最新一条整合内容前 40 字（含【做梦整合】/【做梦摘要】原始前缀，`:220-221`；`companion_serve.rs:1832-1838`） |

#### 事件：`memory_recall`（记忆被唤起，脱敏）

生产点：chat 工具循环中 `recall_memory` 工具**执行成功**时推（`companion_serve.rs:1320-1329`）。

```json
{"type":"memory_recall","at":"2026-08-21T08:30:00Z","found":3,"keywords":["考试","数学","线代"],"redacted":true}
```

| 字段 | 类型 | 说明 / 出处 |
|------|------|--------------|
| `found` | number | 命中条目数（`RecallMemoryTool` 输出的 `found`，`presence.rs:229-230`） |
| `keywords` | string[] | query 切词：空白 + 中英文标点切分，≤ 8 词、每词 ≤ 16 字（`:231-232`、`:254-263`） |
| `redacted` | bool，恒 `true` | 脱敏占位：命中原文（`top`）**设计上不进事件**；前端若需原文，走授权的记忆面板接口而非 SSE（`:233-235`） |

#### 诚实标注（断点仍在，0 装）

- emotion **只能由 daemon_loop 异步推**：daemon（含情绪引擎）内部 RefCell 跨 await 非 Send，chat handler 同步路径拿不到 PAD（`presence.rs:21-23`、`companion_serve.rs:29-32`）。且主人消息后的那条推的是**当前真实 PAD（多为基线）**——`on_user_message` 只喂节律不触情绪事件（`companion_serve.rs:1895`）。
- memory_recall **只接工具桥路径**：`build_injection` 记忆注入路径的召回条目数锁在 `assemble.rs::inject_memory` 内部（局部变量不外露），该路径未接（`presence.rs:24-25`、`companion_serve.rs:33-34`）。
- presence 事件**只进 SSE 广播，不进 Lark/Telegram 离线 sink**（sink 只收渲染文本）（`presence.rs:26`、`companion_serve.rs:35`）。

#### 频率纪律与前端建议

- emotion：60s tick 心跳 + 事件触发（主人消息）——前端应做**平滑插值**，不要逐帧跳变。
- initiative held：按 `gate` 原因去抖——前端宜作「状态条」而非「消息流」展示。
- dream：只报启动后增量——前端不要把首次连接的空窗当异常。
- 分流：行首 `{` → presence JSON（再按 `type` 分发）；否则按 legacy 文本行处理。

#### 前端当前消费现状（诚实）

两侧消费者**均未识别 presence JSON 行**（并入差距表 G5）：`subscribeCompanionEvents` 会把 JSON 行当纯文本问候展示（`runtime.ts:1047-1052`）；`ActivityView` 的 `JSON.parse` 对 presence 行能成功，但期望 span 字段，会落为通用「Agent 活动」条目（`ActivityView.svelte:181-206`）。

### 8.2 屏幕感知 — 未接，0 装

无对应端点、无前端代码（前后端 grep `screen` 无相关命中）。待后端提供 `/v1/apeireth/screen-*` 或 SSE 事件后再补契约。

### 8.3 语音 — 未接，0 装

无对应端点、无前端代码（前后端 grep `voice|audio|tts|whisper` 无相关命中）。待 TTS/STT 链路确定后再补契约。

---

## 9. 前端开发守则

### 9.1 脱敏与隐私

1. **memory 内容属隐私**：`/v1/panel/memory/*`、`/v1/panel/sessions/:id/timeline` 返回的 `content` 是主人与伙伴的原文（含偏好/约定/情绪信号提炼）。UI 默认折叠/截断展示，不打日志、不上报遥测。
2. **面板默认 redacted**：审计接口已对 `masked=true` 的记录把 `call_content` 替换为 `"[masked by audit] (隐私已脱敏)"`（`panel_readonly.rs:437-442`）——前端**不得尝试还原**，并应在 UI 上以脱敏样式呈现。SSE 出站消息也已过 PII 检测 + Mask（`daemon.rs:462-470`），前端同样按"已脱敏但仍是个人内容"处理。
3. **凭证永不落盘**：`apiKey` / `masterToken` 仅存内存；`localStorage` 只写 `{baseUrl, model, theme}`，读取时主动剔除历史遗留的 key 字段（`runtime.ts:126-178`）。新增配置字段时沿用此白名单模式。
4. `master_token` 只出现在 `POST /v1/apeireth/grant` 请求 body，不入 URL、不入日志。

### 9.2 断线重连策略建议

- **SSE `/v1/apeireth/events`**：参照 `subscribeCompanionEvents` 的指数退避（首试 2s，×1.5，封顶 30s，连接成功后复位，`runtime.ts:1022`、`:1035`、`:1059-1060`）。注意服务端 broadcast 容量 64、落后即丢（`companion_serve.rs:980`、`:1532`）——重连后**不要**假设能补到断线期消息，应主动拉一次相关 REST（如 approval-requests）对齐状态。`EventSource` 自带重连不可控，建议统一走 fetch+reader 的手动循环（现有实现即如此）。
- **轮询兜底**：`App.svelte:463-465` 已每 15s 刷新健康 + 审批请求。SSE 断线期间这是唯一状态来源，保留。
- **chat 请求**：503（限流）按 body 提示 10-30s 后由用户重试（`companion_serve.rs:1256`）；`streamChat` 中断（`done` 提前/网络错）时前端已能保留已收 `fullText`，不要整句丢弃。
- **所有 panel 只读端点**失败时按「数据源暂缺」降级展示，不阻塞主对话链路。

### 9.3 版本协商方式

- **现状**：无显式协商。可用信号有两个——① `GET /health` 的 `version`（crate 版本，`companion_serve.rs:1903`）与 `features` 标签数组（`:1904`）；② `GET /v1/apeireth/capabilities` 的 capability manifest（**当前 404**，前端回落 `legacyCapabilityManifest()`，`runtime.ts:597-726`）。
- **约定**：manifest 端点上线前，前端一律按 legacy profile gate mutation 按钮（未知 capability id 视为 unsupported，`runtime.ts:632-640`）；manifest 上线后以 `schema_version` + `capabilities[].supported/available/reason` 为准（类型见 `types.ts:139-182`，语义镜像 Rust 侧）。
- 新增字段一律**向后兼容**：后端加字段不改名（前端未知字段忽略）；前端消费新字段前先用 `capabilities` 或 `features` 存在性 gate，不用 UA/路径探测（404-probing 已被 manifest 取代，`runtime.ts:587-596` 注释）。

---

## 附录 A：前端 fetcher ↔ 端点映射速查

| fetcher（`runtime.ts`） | 端点 | 匹配 |
|---|---|---|
| `checkHealth` / `checkHealthDetailed` | `/health`、`/v1/models`、`/v1/panel/sessions`、`/v1/panel/memory/streams`、`/v1/tools/list`✗ | 部分（G7） |
| `listModels` | `GET /v1/models` | ✓ |
| `streamChat` / `chatOnce` | `POST /v1/chat/completions` | ✓（注意 §2.3 流式 known limits） |
| `fetchCapabilities` | `GET /v1/apeireth/capabilities` ✗ → legacy 兜底 | 设计内降级（G6） |
| `fetchBackendSessions` | `GET /v1/panel/sessions` | ✓ |
| `fetchSessionTimeline` | `GET /v1/panel/sessions/:id/timeline` | ✓ |
| `fetchMemoryStreams` | `GET /v1/panel/memory/streams` | ⚠ G2 |
| `fetchMemoryEpisodes` / `fetchEpisodes` | `GET /v1/panel/memory/episodes` | ✓ |
| `fetchGraphData` | `GET /v1/panel/graph` | ⚠ G3 |
| `fetchAuditLogs` | `GET /v1/panel/audit` | ⚠ G4 |
| `fetchTools` | `/v1/tools/list` ✗ → `/v1/panel/tools` ✗ | ✗ G7 |
| `fetchApprovalRequests` | `GET /v1/apeireth/approval-requests` | ⚠ G1 |
| `grantToolPermission` / `grantApproval` | `POST /v1/apeireth/grant` | ✓ |
| `appendMemoryEpisode` | `POST /v1/memory/append` ✗ | ✗ G8 |
| `fetchOrgans` | `GET /v1/organs` ✗ | ✗ G9 |
| `subscribeCompanionEvents` / `ActivityView` EventSource | `GET /v1/apeireth/events`（双 data 行共流，见 §8.1） | legacy 文本行 ✓ / presence JSON 行 ⚠ G5（两消费者均未按 `type` 分流） |
| `fetchBackendSessionsV2` / `create|rename|archive|restore|closeBackendSession` | `/v1/apeireth/sessions*` ✗ | ✗ G10 |
| `update|forget|protect|unprotectMemoryEpisode` | `/v1/apeireth/memory/episodes/:id*` ✗ | ✗ G10 |
| `fetchGrants` / `revokeGrant` | `/v1/apeireth/grants*` ✗ | ✗ G10 |
| `fetchTraces` / `fetchTraceDetail` | `/v1/panel/traces(/:id)` ✗ | ✗ G10 |

*文档完。任何字段与代码冲突时，以代码为准并回写本文档（含出处行号）。*
