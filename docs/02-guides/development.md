# Apeireth Development Guide

> 对齐实际工作流（2026-08-19 post-1.0.0）。给想参与开发的人：构建/测试/代码地图/纪律。

## 构建与测试

```bash
cargo build --workspace              # 85 + 1 desktop crates 全量构建
cargo check --workspace --all-targets  # 编译全 target（含 examples/bins/tests）— 必跑
cargo test --workspace               # 23,874 组 0 失败（含 post-1.0.0 增量）
cargo test -p apeireth-cron --test integration_cron  # cron 25 case integration tests
cargo test -p apeireth-companion --lib  # 伙伴器官 644 测试（最快的核心反馈环）
cargo fmt --all --check              # 格式
```

## CI 复刻 SOP — push 前必跑 (post-1.0.0 加固, commit 95c358af)

**为什么需要**: 本地 `cargo test` 绿 ≠ push 后 GitHub Actions 必绿。常见 fail 原因:
- **格式漂移** (rustfmt.yml fail): 多人并发 commit, 有人 commit 时没跑 `cargo fmt` → CI fail
- **锁文件过期** (`--locked`): Cargo.lock 落后, CI fail
- **测试集差异** (nextest vs libtest): CI 用 `cargo nextest`, 本地 `cargo test` 跑的不一样

**SOP (push 前必跑, 10 分钟)**:

```bash
# === 1. 格式 + 锁文件 ===
cargo fmt --all --check --locked     # 一行: format check + locked
# ⚠️ Windows 用户注意: cargo fmt --all 在 windows cargo 1.97.1 有 bug,
#    跑 "文件名或扩展名太长 (os error 206)"; workaround:
cargo fmt --package <each-crate>     # 逐 crate 跑 (等价效果, 见下方脚本)
# 推荐: 直接用 git bash 跑
bash -c 'cargo fmt --all -- --check'

# === 2. 完整 CI 复刻 (Makefile 一键) ===
make ci                               # = ci-build + ci-test + ci-release (commit 818c6857)

# 等价手动命令:
cargo build --workspace --tests --locked                              # 5 分钟
cargo nextest run --workspace --profile ci --locked                    # 3 分钟 (需装 cargo-nextest)
cargo build --release --workspace --locked                            # 3 分钟

# === 3. 8 硬墙守门 (仓库级) ===
make release-prep                     # 8 硬墙 + PII + 12 项 checklist (post-1.0.0)
# BLOCKING 模式 (1 P0 fail 退出 1):
make release-prep-block

# === 4. 全绿后才 push ===
git push origin master
```

**4 个 SOP,缺一不可**:
| 步骤 | 命令 | 含义 |
|---|---|---|
| 1. fmt | `cargo fmt --all -- --check` | CI rustfmt.yml 必绿 |
| 2. ci-build | `cargo build --workspace --tests --locked` | CI rust.yml line 63 |
| 3. ci-test | `cargo nextest run --workspace --profile ci --locked` | CI rust.yml line 67 |
| 4. ci-release | `cargo build --release --workspace --locked` | CI rust-ci.yml line 106 |

**4 个绿了** → push 必绿(同一份 source, Linux CI 重跑一遍)。

**Windows 已知 workaround**: `cargo fmt --all` 在 windows cargo 1.97.1 触发"文件名或扩展名太长 (os error 206)"。
- 短期: 逐 crate 跑 `cargo fmt --package <crate>` (手动模拟 --all)
- 中期: 装 git bash, `bash -c "cargo fmt --all -- --check"`
- 长期: 升 rustfmt 修 (需 cargo upstream fix)

**真实案例**: 2026-08-19 PR #83 commit `a77f16f` 没 fmt, CI rustfmt.yml fail。
修: `cargo fmt --package apeireth-guard` → commit `95c358af` → push → CI 必绿。

## 守门 1 宽限规则 (post-1.0.0, commit 8551e912)

**背景**: 主人 R148 (2026-08-13) 拍板 24 LOCKED crate 入口签名冻结 = **0 约束力**, 24 LOCKED 降级为历史记录。后续 PHL-07 (13 键 verdict cache) 升级, 多个 commit 触碰 LOCKED crate 文件 (13c25025, 894dd260) 但纯 logic 守门, fmt 漂移导致 CI 误报。

**修法** (8 哲学锚 S-2 实事求是 + O-5 不假装):
- 守门 1 (24 LOCKED crate 不触碰) 改用 `git diff -w --ignore-blank-lines --ignore-cr-at-eol` 检测
- **如果 -w 0 diff = pure fmt, 放行**
- **如果 -w 有 diff = 真的 logic 改, 继续 fail**

**意味着**:
- 改 24 LOCKED crate 的 fmt drift (`cargo fmt --package`) → CI pass
- 改 24 LOCKED crate 的 logic (`pub fn`, `enum`, `const`, 3 不可变脊柱) → CI fail
- 工程规范 (S-3 质量工程化): `cargo fmt` 守门保留, fmt 漂移仍 fail, 0 降级

**相关 commit**:
- `f7c67ac4` PII exclude fix (守门 #5)
- `5e55f729` fmt fix 8 个非 LOCKED
- `c8bfe11b` → `8541e912` LOCKED fmt fix 撤回 + 选项 trace
- `8551e912` 守门 1 宽限 (本 commit, 选项 A)

## 前端开发 (companion-desktop, post-1.0.0 新增)

`frontend/companion-desktop/` 是**独立 [workspace]** (Svelte 5 + Tauri 2), 不在 root cargo workspace.
其 CI 守门在 `.github/workflows/companion-desktop-ci.yml` 单独跑 (cargo check Tauri shell + pnpm svelte-check + 8 硬墙).

```bash
# 前置: Node 20+ + pnpm 9+ (Windows: WebView2 runtime)
cd frontend/companion-desktop
pnpm install
pnpm dev                            # Vite + Svelte (http://localhost:1420)
pnpm check                          # svelte-check (类型 + 语法)
```

> 真实 LLM 流式 (CoT + tool_call + tool_result SSE) **🟡 TP34 后端 50% 落地 (2026-08-19)**: companion_serve 加 streaming 分支 (`stream_forward` 透传, 跳过 tool loop), `extract_minimax_cot` helper 拆 `<!-- -->` 边界 (MiniMax M3 0 OpenAI 风格 reasoning 字段, 验证报告 `_research_mem/sub_agent_reports/2026-08-19/MiniMax_reasoning_verification.md`), 8 个单测全过 (`cot_extraction_tests` mod). 前端 `<!-- ... -->` 状态机切分 + `reasoning-delta` / `content-delta` RuntimeEvent 触发 v1.5 续. 当前 non-streaming `stream: false` 仍写死, 但响应 `x_apeireth.reasoning_content` 字段已挂 reasoning,
> 前端 6 种 RuntimeEvent 中部分不可触发, mock SSE e2e 跑通. 详见 `docs/04-internal/next-team-handbook.md` TP34.

**注意**：`cargo test --workspace` 不编译 examples——改公共结构后必须 `--all-targets`。

## 代码地图（从哪开始读）

### 伙伴器官（apeireth-companion，~25K 行）

| 读序 | 文件 | 内容 |
|---|---|---|
| 1 | `src/assemble.rs` | CompanionApp 装配器——所有机制怎么接起来 |
| 2 | `src/context.rs` | 注入管线（L0/L1 常驻 + 预算截断）|
| 3 | `src/memory_extractor.rs` | 记忆 v2 核心（importance/对账/排名）|
| 4 | `src/memory_graph.rs` | 双时态事实图 + crawl |
| 5 | `src/world_model.rs` → `src/causal_world_model.rs` | 世界模型 W1 → W2/W3 |
| 6 | `src/curiosity.rs` → `src/hypothesis.rs` → `src/emotion_memory.rs` → `src/value_cases.rs` | 她本身（E4/F4/F1/F6）|
| 7 | `src/emergence.rs` | 开口策略（E7）|
| 8 | `src/oracle.rs` + `src/intent_brier.rs` | 校准 + 自我诊断 |

### 工具链

| 读序 | crate | 内容 |
|---|---|---|
| 1 | `apeireth-tool-runtime` | parser/executor/record |
| 2 | `apeireth-tool-approval` | 5 规则审批 |
| 3 | `apeireth-tools` | schema/guardrail/yaml_spec |

### 安全

| crate | 内容 |
|---|---|
| `apeireth-http-client::egress` | 出站默认拒绝 + 审计链（**trait 口已备, 实装待补** per backlog S4 P1 未实施, 2026-08-18 复核） |
| `apeireth-guard` | PII 脱敏 |
| `apeireth-companion::job_object` | Windows Job Object 沙箱 |

### CI 守门 (post-1.0.0 加固)

| workflow | 守门内容 |
|---|---|
| `rust.yml` | cargo nextest (3 OS matrix) + 8 硬墙 job (LOCKED / version / R11 baseline / 13 键 / V1136) |
| `companion-desktop-ci.yml` | cargo check (Tauri shell) + pnpm svelte-check + 8 硬墙 |
| `pii-leak-detection.yml` | 8 关键词 grep (防前轮 11 轮 filter-repo 清洗回潮) |
| `release-1.0.0.yml` | 8 包齐发矩阵 (deb/rpm/brew/scoop/tarball/msi/docker×2) + 5/5 gate |

详见 `docs/04-internal/ci-fix-log-2026-08.md` 历史 + `docs/04-internal/next-team-handbook.md` 排期.

## 机制设计模式（本项目特色）

1. **trait 策略注入**：lib 零 LLM 依赖——`MemoryExtractor`/`DreamSummarizer`/`ReflectionReflector`/`ConstitutionLlm` 等全是 trait，测试用 Mock 实现，生产注入真 LLM。新机制照此模式。
2. **确定性机制件**：curiosity/hypothesis/emotion_memory/value_cases 全是确定性无 LLM——可单测、可复现（固定种子 LCG）。LLM 行为是下游消费方的事（0 装标注）。
3. **集成而非分立**：新需求挂既有机制（oracle/memory/bus/approval 链），不造平行系统。
4. **0 装 PASS**：未实现标 `trait 口已备未接`；无环境标"待实测"；真实 API 测试带限流退避。

## 常见陷阱（前人踩过）

| 陷阱 | 规则 |
|---|---|
| std Mutex 不可重入 | 持 guard 期间禁止调用会再取同一把锁的方法（migrate_subject 死锁教训）|
| Windows cmd 嵌套引号 | 子进程测试直接 spawn，不经 `cmd /c`（powershell 脚本会被解析坏）|
| Job Object 内存限制语义 | 超限 = 拒绝分配（OOM），不是杀进程（与 CPU 时间限制不同）|
| HashMap 迭代序 | 确定性测试要求排序后再比较（curiosity 采样教训）|
| 并行测试静态原子 | 共享状态必须共享锁（TUI hand 竞态教训）|
| 真实 API 压测 | 必须带退避，否则限流自造失败 |

## 提交规范

- 改码必改对应 README/docs（文档同步自觉）
- 改公共结构（enum/struct/签名）→ grep 所有构造点 + all-targets
- 验收标准：全量测试绿 + all-targets 干净 + 文档同步
- 分支：开发分支 → 全量验证 → 合入 integration → 发布时同步 master
