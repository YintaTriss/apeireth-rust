# Apeireth Desktop — Release Candidate Reality Check Matrix

> **审计基线**: `Jimmy master @ 0bd63405bfd57492a03ed095c8d511fb6caf2d2c`
> **RC 分支**: `rc/desktop-reality-check`
> **审计原则**: 先证明问题存在，再修改；严禁凭空构造新层或虚假完成；严格划分 P0–P3 缺陷级别。

---

## 一、真实端到端路径验证矩阵 (RC Test Matrix)

| 序号 | 场景 (Scenario) | 自动化 (Automated) | 手动 (Manual) | 真实后端 (Real Backend) | 真实模型 (Real Model) | 状态 (Status) | 证据与结论 (Evidence) |
|---|---|---|---|---|---|---|---|
| **01** | **冷启动与后端发现 (Cold Start)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | `/health` 探活 3s 超时，离线时正确提示 `后端离线` 并禁用非法生成 |
| **02** | **SSE 事件长连接断线重连 (SSE Reconnect)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **HARDENED** | 增加指数退避断线自动重连，避免后端重启后长连接永久失效 (P1 修复) |
| **03** | **流式思考与正文解析 (Reasoning Stream)** | ✅ PASS | ✅ PASS | ✅ PASS | ✅ PASS | **VERIFIED** | `<think>` 标签与 `reasoning_content` 双流解析无残留，折叠卡片与计时器正常 |
| **04** | **生成中断 (Stop Generation)** | ✅ PASS | ✅ PASS | ✅ PASS | ✅ PASS | **VERIFIED** | `AbortController` 立即终止 HTTP 流，标记 `aborted: true`，不浪费后续 token |
| **05** | **多会话切换隔离 (Conversation Isolation)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | 消息 updates 与 deltas 均基于 `conversationId` 和 `messageId` 寻址，切换不串台 |
| **06** | **高危工具与权限洋葱审批 (Approval Security)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **HARDENED** | Master Token 移出明文 localStorage 持久化，仅在会话内存中安全流转 (P0 修复) |
| **07** | **工具注册表与 Schema (Tool Registry)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | `GET /v1/tools/list` 实时拉取 30+ 原生工具，Tier 1-3 筛选与 Schema 格式化正常 |
| **08** | **记忆检索与图谱关联 (Memory & Graph)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | `GET /v1/panel/memory/episodes` 与 `GET /v1/panel/graph` (`factg-*`/`link-*`) 正常 |
| **09** | **活动中心领域隔离 (Activity Center)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | Goals / Runs (ActionStream) / Workflows / Scheduled 4 领域独立清晰呈现 |
| **10** | **伴随体表现态与眼动 (Companion Presentation)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | 表现态 100% 由后端与运行期状态推导，金色眼球动效与主动涌现气泡正常 |
| **11** | **桌面原生快捷窗与托盘 (Quick Window & Tray)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | Tauri 2 原生快捷窗、托盘菜单、窗口隐藏与关闭防护正常 |
| **12** | **设置与端点测速诊断 (Settings & Diagnostics)** | ✅ PASS | ✅ PASS | ✅ PASS | N/A | **VERIFIED** | 模型列表自动发现、端点 Ping 真实响应延迟测速正常 |

---

## 二、缺陷与硬化审计清单 (Findings & Hardening)

### P0 — Master Token 明文 localStorage 持久化风险 (已修复)
- **现象**: Master Token 曾尝试写入浏览器/Webview `localStorage`，存在明文读取安全隐患。
- **根因**: 原设计为了方便用户无需重复输入，但违反了“Master Token 严禁明文持久化”的安全性规范。
- **修复**: Master Token 仅保留在会话内存状态中，在执行授权操作时按需提交给 `/v1/apeireth/grant`，不在 disk / localStorage 留存明文。

### P1 — SSE 伴随体事件通道断线后单次失效 (已修复)
- **现象**: `subscribeCompanionEvents` 捕获异常后直接退出 async 函数，后端重启或短暂断网后长连接永远失效。
- **根因**: 缺少断线重试与指数退避机制。
- **修复**: 在 `runtime.ts` 中实现指数退避自动重连循环（2s 起步，上限 30s），并在组件卸载时安全 abort。

### P2 — 消息流式输出时视口自动跟随滚动 (已优化)
- **现象**: 长消息或连续多轮流式生成时，视口不自动跟随最新文本滚动。
- **修复**: 在 `ChatView.svelte` 中加入平滑跟随滚动逻辑，提升流式打字阅读体验。
