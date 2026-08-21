# Apeireth Desktop RC1 — Native Desktop Acceptance Protocol

> **定位**: Apeireth Desktop RC1 (Backend / Runtime Integration 已经 100% 验证) 的最后一公里真实 GUI / 进程级验收协议。
> **目标**: 针对真实端到端桌面动作进行 10 场景实测与证据留痕。

---

## 一、RC1 状态与分层定义

```text
Apeireth Desktop RC1 阶段划分:
├─ [RC1-Core] Core Runtime Integration (317 backend tests + 9 live integration tests)  ──> ✅ PASS
├─ [RC1-Core] Data Persistence & SQLite (episodes, graphs, action_streams, goals)     ──> ✅ PASS
├─ [RC1-Core] Security Onion (Master Token Memory Isolation)                         ──> ✅ PASS
├─ [RC1-Core] Cognitive Data Path (6 Memory Streams, Depth Query)                     ──> ✅ PASS
├─ [RC1-Core] Frontend Build & Type Safety (Svelte 5 + TS 0 errors, clean dist)       ──> ✅ PASS
│
└─ [RC1-GUI]  Native Desktop Live Acceptance Pass (10 Real Product Actions)          ──> ⏳ ACCEPTANCE READY
```

---

## 二、Native Desktop 10 项真人/实机验收检查清单 (Checklist)

| # | 验收场景 | 具体操作步骤 | 预期判定标准 (PASS Criteria) | 实测状态 |
|---|---|---|---|---|
| **01** | **真启动 Desktop + Backend** | 启动 `companion_serve` (:8090) 并运行 Tauri 桌面端 | 界面顶部状态绿灯，显示 `已连接 (0.0.0.0:8090)`，探活延迟正常呈现 | 待签收 |
| **02** | **真实模型对话与流式输出** | 输入用户消息并发送（如“介绍一下你自己”） | 思考过程折叠 `<think>` 正常展开计时，正文流式输出且平滑跟随自动滚动 | 待签收 |
| **03** | **真实工具调用 (Tool Call)** | 触发只读工具调用（如获取当前系统时间或项目配置） | 生成工具调用卡片，ActionStream 正常记录入参和回执 | 待签收 |
| **04** | **权限洋葱审批 (Approval)** | 触发高危工具（如执行 Shell 或修改关键文件） | 拦截弹出抽屉，输入 Master Token 放行后工具成功执行 | 待签收 |
| **05** | **后端强制中止 (Kill Backend)** | 在任务管理器或终端终止 `companion_serve` 进程 | 桌面端顶部状态秒变灰色/黄色 `后端离线`，发送按钮安全置灰，不崩溃 | 待签收 |
| **06** | **离线状态 UI 表现** | 尝试在离线状态下操作 | 明确提示连接中断，提供重试选项，禁止非法网络请求 | 待签收 |
| **07** | **后端重启 & SSE 自动恢复** | 重新拉起 `companion_serve` 进程 | 无需刷新前端，10s 内健康探活变绿，SSE 通道自动恢复监听，不重复推送历史事件 | 待签收 |
| **08** | **Alt+Space 快捷窗连续触发** | 连续快速按下 `Alt+Space` 10 次 | Spotlight 快捷窗平滑显示/隐藏，无窗口抖动，单实例保持，焦点准确捕获 | 待签收 |
| **09** | **主窗口与快捷窗双开/多实例保护** | 打开主窗口同时调起 Quick Window | 两窗口数据互通（通过同源/同一后端），主窗口关闭时隐藏到托盘而非闪退 | 待签收 |
| **10** | **退出重启与记忆恢复** | 托盘点击 `退出`，重启桌面应用与后端 | 上一轮对话历史无损加载，记忆/图谱视图中保存的 `Episode` 正常召回 | 待签收 |

---

## 三、快速验收启动命令

```powershell
# 1. 终端 A: 启动后端 Companion Daemon & API
$env:APEIRETH_API_KEY = "sk-live-test"
cargo run -p apeireth-companion --example companion_serve

# 2. 终端 B: 启动 Tauri 2 桌面伙伴客户端
cd frontend/companion-desktop
pnpm dev
# 或直接运行 Tauri 调试窗口
pnpm tauri dev
```

---

## 四、签收裁定结论

完成上述 10 项真人实机操作后，在各栏记录实测时间与操作人，即可移除所有限定词，正式完成 **Apeireth Desktop RC1** 终极交付。
