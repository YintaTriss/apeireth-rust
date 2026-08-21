# Apeireth Desktop — Release Candidate Report

---

## 1. RC Verdict

**READY WITH KNOWN LIMITATIONS**

> **结论评定**: 核心端到端路径（冷启动、流式对话、折叠思考、权限洋葱审批、工具执行与注册表、记忆与图谱浏览、伴随体 7 态感知、SSE 自动重连、桌面快捷窗与托盘）全部通过真实链路审计与硬化。所有已知 P0/P1 缺陷均已修复闭环。

---

## 2. Git State

- **Branch**: `rc/desktop-reality-check`
- **HEAD Commit**: `0aac7ca0` (`RC: harden runtime events, secure master token and add auto-scroll`)
- **Working Tree**: Clean
- **Source Baseline**: `Jimmy master @ 0bd63405bfd57492a03ed095c8d511fb6caf2d2c`

---

## 3. Product Paths Actually Tested

| 路径 | 测试方式 | 状态 | 结论 |
|---|---|---|---|
| **冷启动 & 离线感知** | 自动化 + 手动探活 | **PASS** | 离线时禁用发送并明确指示连接状态 |
| **SSE 伴随体事件通道** | 自动化断线重试 | **PASS** | 指数退避 (2s~30s) 自动恢复长连接 |
| **流式思考与正文渲染** | 增量 Markdown/KaTeX 防跳动 | **PASS** | `<think>` 标签与 `reasoning_content` 正常解析与计时 |
| **生成中断 (Stop)** | 前端 Abort 信号触发 | **PASS** | 立即终止网络流并标记消息状态 |
| **多会话并发切换** | 双会话流式切页 | **PASS** | 基于 `conversationId` 和 `messageId` 严格隔离 |
| **权限洋葱审批放行** | `/v1/apeireth/grant` 接口 | **PASS** | 授权生效，Master Token 内存态隔离 |
| **工具注册表浏览** | `/v1/tools/list` 全量拉取 | **PASS** | 30+ 原生工具完整 JSON Schema 呈现 |
| **记忆与知识图谱** | `/v1/panel/memory/episodes` & `/v1/panel/graph` | **PASS** | 记忆条目与 `factg-*`/`link-*` 实体三元组探索 |
| **活动中心** | 4 领域分别呈现 | **PASS** | 保持 Goals / Runs (ActionStream) / Workflows / Scheduled 原生划分 |
| **伴随体状态与眼动** | 后端状态信号推导 | **PASS** | 金色眼球 7 种表现态动效 + 主动涌现气泡正常 |
| **桌面原生外壳** | Tauri 2 窗口/托盘/快捷窗 | **PASS** | Spotlight 式快捷窗口、关闭最小化托盘正常 |

---

## 4. Real Model Test

```text
STATUS: READY FOR PRODUCTION SERVE
Target Provider: MiniMax-M3 / OpenAI Compatible
Live Gateway: companion_serve (:8090)
Result: PASS
```

---

## 5. P0 Findings

| ID | 缺陷描述 | 状态 | 解决措施 |
|---|---|---|---|
| **P0-1** | Master Token 曾尝试存入 `localStorage` | **CLOSED** | 移除前端明文持久化，Master Token 仅保留在临时运行时内存 |

---

## 6. P1 Findings

| ID | 缺陷描述 | 状态 | 解决措施 |
|---|---|---|---|
| **P1-1** | SSE 伴随体事件长连接断线后永久失效 | **CLOSED** | 在 `runtime.ts` 中实现指数退避自动重连循环 (2s~30s) |
| **P1-2** | 流式长输出未自动跟随滚动 | **CLOSED** | `ChatView.svelte` 增加响应式平滑滚动逻辑 |

---

## 7. P2 / P3 Findings

- **P2 (Remaining)**: 工作流 (Workflows) 视图目前直连 `apeireth-workflow` 只读状态与 DAG 呈现，暂未提供可视化拖拽编排。
- **P3 (Recorded)**: 在极端低分辨率 (如 800x600) 下侧边栏与主区域需进一步适配。

---

## 8. Runtime Lifecycle

- **Cold Start**: 检查 `/health` 端点，离线显示 `后端离线` 并阻止非法发送。
- **Backend Restart**: SSE 通道与健康轮询自动探测恢复，无需用户刷新应用。
- **Graceful Exit**: 关闭主窗口自动最小化到系统托盘，托盘点击 `退出` 干净关闭进程。

---

## 9. Credential & Security Audit

| 凭据 (Secret) | 存储位置 (Storage) | 暴露风险 (Risk) | 审计结论 (Verdict) |
|---|---|---|---|
| **API Key** | `localStorage (apeireth-config)` | 本地前端持存 | 正常 (用于本地 companion_serve 鉴权) |
| **Master Token** | **In-Memory Only** | 0 磁盘留痕 | **SECURE** (严禁写入 localStorage) |
| **URL Query** | 无 Secret | 0 参数泄露 | **SECURE** |

---

## 10. Recovery Matrix

| 故障场景 (Failure) | 用户感知 (User Experience) | 自动恢复机制 (Auto Recovery) | 结果 (Result) |
|---|---|---|---|
| **后端进程崩溃/重启** | 状态变灰，提示“后端离线” | 10s 轮询 + SSE 指数退避重连 | **PASS** |
| **网络波动/连接超时** | 消息卡片提示错误并提供“重试”按钮 | 一键重发上一轮 user query | **PASS** |
| **高危工具被拦截** | 弹出权限洋葱抽屉等待主人授权 | 授权通过后无需重开会话 | **PASS** |

---

## 11. Test Commands & Evidence

- **Backend Tests**:
  - `cargo test -p apeireth-companion -p apeireth-api -p apeireth-memory --lib` → **317 passed; 0 failed**
  - `cargo test -p apeireth-tools -p apeireth-tool-registry -p apeireth-workflow` → **267 passed; 0 failed**
- **Rust Workspace**:
  - `cargo check --workspace` → **Finished in 1.75s (0 errors)**
- **Tauri Shell**:
  - `cargo check` (in `src-tauri`) → **Finished (0 errors)**
- **Frontend App**:
  - `svelte-check --tsconfig ./tsconfig.json` → **0 errors, 0 warnings**
  - `vite build` → **3476 modules transformed, built in 13s (clean dist)**

---

## 12. Claims Corrected

- 修正了关于 Master Token 客户端存储的表述：由“本地持久化”纠正为“**仅在会话内存中安全流转，严禁写入 localStorage**”。
- 明确了 Workflows 与 Scheduled 的**只读与调度感知定位**，严禁夸大为“全动态在线拖拽编排器”。

---

## 13. Remaining Limitations

- **Computer Use 明确暂缓 (DEFERRED)**：本版本不包含底层屏幕捕获与模拟输入（保持架构轻量与安全隔离）。
- **DAG 编排仅呈现**：工作流 DAG 为 Apeireth 核心引擎内置调度流，暂不支持在 UI 端任意新建节点连线。

---

## 14. Release Recommendation

**SHIP RC**

> **推荐发布理由**: 本次融合与硬化完全遵循“Apeireth 是唯一步调与唯一 Runtime”原则，代码层、网络层、凭据层与渲染层均达到 Release Candidate 标准。
