# Apeireth Documentation

> 文档体系 1.0（2026-08-18 规范重构）：结构分层，历史归档，与实际代码对齐。

## Structure

```
docs/
├── 01-architecture/     # 架构（品牌/愿景/哲学/架构/安全/工程报告）
├── 02-guides/           # 使用（快速开始/用户手册/部署/开发）
├── 03-reference/        # 参考（crates/API/术语）
├── 04-internal/         # 内部工作文档（台账/设计意图/团队）
├── design/              # 前端设计（设计系统/交接/开场动画封存档案）
└── archive/             # 历史归档（stage*/r*/adr/conventions... 保留不展示）
```

## Index

| 文档 | 说明 |
|---|---|
| [01-architecture/brand.md](01-architecture/brand.md) | 品牌：命名（Apeiron）+ 宣言 + Logo Design Brief |
| [01-architecture/vision.md](01-architecture/vision.md) | 愿景：五原型 + 产品北极星 + 三远合一 |
| [01-architecture/philosophy.md](01-architecture/philosophy.md) | 哲学：6 锚 / 双洋葱 / 0 装 PASS |
| [01-architecture/architecture.md](01-architecture/architecture.md) | 架构总览（对齐 85 crates）|
| [01-architecture/security.md](01-architecture/security.md) | 安全模型（对齐实际机制）|
| [01-architecture/engineering-report.md](01-architecture/engineering-report.md) | 工程报告（1.0 实测数据/里程碑/纪律）|
| [02-guides/quick-start.md](02-guides/quick-start.md) | 快速开始（真实命令）|
| [02-guides/user-manual.md](02-guides/user-manual.md) | 用户手册（功能详解/FAQ）|
| [02-guides/deployment.md](02-guides/deployment.md) | 部署（环境变量/持久化/前端接入/故障排查）|
| [02-guides/development.md](02-guides/development.md) | 开发指南（代码地图/模式/陷阱/提交规范）|
| [03-reference/crates.md](03-reference/crates.md) | 85 crates 索引（从代码生成）|
| [03-reference/api.md](03-reference/api.md) | API 参考（真实端点/工具协议/认证）|
| [03-reference/glossary.md](03-reference/glossary.md) | 术语表（品牌/架构/记忆/她本身/安全）|
| [04-internal/design-intent.md](04-internal/design-intent.md) | 设计意图与拍板历史 |
| [04-internal/backlog.md](04-internal/backlog.md) | 唯一权威台账 |
| [04-internal/release-plan.md](04-internal/release-plan.md) | 发布计划 |
| [design/01-DESIGN-SYSTEM.md](design/01-DESIGN-SYSTEM.md) | 前端设计系统（视觉令牌/层序规范）|
| [design/frontend-handoff.md](design/frontend-handoff.md) | 前端交接（现状/联调/坑与纪律/欠账，接手先读）|
| [design/intro-animation.md](design/intro-animation.md) | 开场动画「火之文明史」封存档案（2026-08-22 起默认关闭）|

## Archive

历史设计/轮次/决策文档在 [`archive/`](archive/)（stage1-6、r149-r270、adr、conventions、glossary 等）——保留完整 git 历史，不再作为活跃文档索引。
