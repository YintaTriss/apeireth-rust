# Apeireth Unified Desktop Baseline (Phase 0)

## 1. Git 信息

- **Source remote**: `https://github.com/Jimmyxiao2009/apeireth-rust.git`
- **Source branch**: `master`
- **Source HEAD**: `0bd63405bfd57492a03ed095c8d511fb6caf2d2c`
- **Current worktree**: `E:/Desktop/项目/CrossPlatform/Apeireth/apeireth-unified`
- **Current branch**: `integration/apeireth-unified-desktop`
- **Clean worktree status**: YES

## 2. 编译与测试验证记录

- **Rust Workspace**:
  - 命令: `cargo check --workspace`
  - 结果: PASS (0 errors, 2m 02s, 86+ crates checked)
- **后端核心测试**:
  - 命令: `cargo test -p apeireth-companion -p apeireth-api -p apeireth-memory --lib`
  - 结果: PASS (317 passed; 0 failed; 0 ignored; finished in 0.91s)
- **桌面前端检查与构建**:
  - 命令: `pnpm check && pnpm build` (in `frontend/companion-desktop`)
  - 结果: PASS (svelte-check found 0 errors, 0 warnings; vite build completed in 13.72s)

## 3. 当前 Runtime 真实端点清单

### 对话与流式
- `GET /health` — 伴随体服务健康检查
- `GET /v1/models` — 兼容 OpenAI 的模型列表 (`MiniMax-M3` 等)
- `POST /v1/chat/completions` — OpenAI 兼容对话端点 (支持 SSE 流式、工具自动调用与执行、5 轮上下文、X-Apeireth-Continuity 会话标签)

### 授权与控制
- `POST /v1/apeireth/grant` — 授权显式放行或撤销
- `GET /v1/apeireth/approval-requests` — 待处理工具授权请求列表
- `GET /v1/apeireth/events` — SSE 伴随体事件通道 (涌现问候、节律更新、做梦/反思通知)
- `POST /v1/apeireth/test-event` — 事件测试注入

### 只读数据面板 (`/v1/panel`)
- `GET /v1/panel/sessions` — 会话列表与活跃时间
- `GET /v1/panel/sessions/:id/timeline` — 会话事件时间线
- `GET /v1/panel/memory/streams` — 6 历史流记录
- `GET /v1/panel/memory/episodes` — 记忆条目 (episodes) 列表与检索
- `GET /v1/panel/graph` — 图谱事实与关系 (`factg-*`, `link-*`)
- `GET /v1/panel/approvals` — 授权请求历史
- `GET /v1/panel/audit` — 审计操作流 (`ActionStream`)

## 4. 当前桌面端能力与现状

- **基础壳**: Tauri 2 + Svelte 5 (Runes) + Vite 6 + TypeScript 5
- **功能视图**:
  - `chat`: 流式会话、基础 Markdown 渲染、流式中断、错误提示
  - `conversations`: 简易本地会话列表切换与持久化
  - `memory`: 基础记忆列表与追加、工具列表、器官状态
  - `settings`: Base URL、API Key 与 Model 配置
- **主要缺口 (Gaps)**:
  - 表现层缺少结构化推理过程 (`<think>`) 展示
  - 交互缺少实时工具调用进度、结果与交互式审批流程
  - 缺少原生活动中心 (Goals, Runs, Workflows, Scheduled)
  - 缺少知识图谱可视化与自成长反思记录
  - 伴随体 Avatar 表现态与节律未同后端 RuntimeBrain 联动
  - 缺少 Quick Window 浮动快捷唤醒 (Alt+Space) 与原生通知
