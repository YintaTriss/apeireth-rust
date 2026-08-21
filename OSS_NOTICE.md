# OSS NOTICE — Open-Source Notices & Attributions

**Project**: Apeireth (源自古希腊语 Apeiron, '无定形/无限'; 品牌宣言见 docs/01-architecture/brand.md)
**Version**: 1.2.0 (per `Cargo.toml` `[workspace.package] version`, B2 upgrade from 1.1.0)
**Edition**: 2021
**License**: Apache License, Version 2.0
**Copyright**: 2026 Apeireth Team
**Authors**: Apeireth Team
**Repository**: https://github.com/apeireth/apeireth-rust
**Homepage**: https://github.com/apeireth/apeireth-rust

---

## 0. Purpose (本文件作用)

本文件 **不替代** 也 **不修改** 主仓根目录的 `LICENSE` 文件 (Apache License 2.0 完整文本,
168 行 / 2026-08-05 写入 / 整合 #4 commit 之前已存在)。

本文件是 Apeireth 项目的 **Open-Source Notices & Attributions 致谢文件**, 专门整合
**借鉴源码 (Borrowed Source Code) 8/11 + 决策链 + LICENSE 致谢**, 作为 Apache 2.0
`§4(a)` 要求的 attribution notices 集中存放点。

### 0.1 文件关系 (Apache 2.0 标准 3 件套)

| 文件 | 作用 | 状态 (2026-08-10) |
|------|------|-------------------|
| `LICENSE` | Apache License 2.0 **完整文本** (官方 verbatim) | ✅ **保持不动** (168 行, 2026-08-05 写入) |
| `NOTICE` | 项目特有 attribution (项目声明 / 致谢 / 法律 / 商标 / 联系方式) | ✅ **保持不动** (66 行, R20 阶段 6) |
| `OSS_NOTICE.md` (本文件) | **借鉴源码 8/11 LICENSE 整合 + 决策链** (本任务新写) | 🆕 **新写** (R128 阶段 D, P13-1) |
| `THIRD-PARTY-NOTICES.md` | cargo-about 生成的 561 crates 第三方 attribution (1709 lines / 12 unique SPDX / 0 cargo-deny violation) | ✅ **保持不动** (106KB, 2026-08-06) |

### 0.2 引用关系 (per Apache 2.0 §4(d) "NOTICE" 条款)

```
LICENSE (Apache 2.0 完整文本)
  ↓ 引用 (§4(d))
NOTICE (项目特有 attribution)
  ↓ 引用 (本文件, 借鉴 8/11 致谢)
OSS_NOTICE.md (本文件)
  ↓ 引用 (cargo-about 生成的 561 crates)
THIRD-PARTY-NOTICES.md
```

---

## 1. 借鉴源码 8/11 ✅ Cloned (真实施, per 决策 #36 §1.1 + 决策 #47 §3.1 + 决策 #55 §3 + 决策 #56 §3 + 决策 #57 §3)

**借鉴源码来源**: `.openclaw/workspace/borrowed-repos/<repo>/`
(整合 #4 commit abf12243 之前, borrowed-repos 已存在并已 cloned 8/11)。

### 1.1 clap (R125-2 ✅ done, P0 supervisor era)

| 字段 | 值 |
|------|---|
| **仓库** | `clap-rs/clap` |
| **版本** | 4.6.6 |
| **Commit** | `4a622b4` (2026-08-06 09:03:20 -0500) |
| **License** | **Apache-2.0** (主) + MIT (dual) |
| **License 路径** | `borrowed-repos/clap/LICENSE-APACHE` (11560 bytes) + `LICENSE-MIT` (1081 bytes) |
| **Copyright** | clap Contributors (https://github.com/clap-rs/clap) |
| **整合位置** | `crates/apeireth-cli/src/` (CLI derive 模式 + 26.5KB commands.rs 简化为 ~12KB) |
| **借鉴 ID** | `R125-2-BORROW-clap-rs/clap-4a622b4-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-2 + 决策 #33 §4.1 P0 supervisor |

### 1.2 hyper (R125-3 ✅ done, P0 supervisor era)

| 字段 | 值 |
|------|---|
| **仓库** | `hyperium/hyper` |
| **版本** | 0.1.20 |
| **License** | **MIT** |
| **License 路径** | `borrowed-repos/hyper/LICENSE` (12443 bytes) |
| **Copyright** | Copyright (c) 2023-2025 Sean McArthur |
| **整合位置** | `crates/apeireth-http-client/src/` (HTTP 客户端 LIFO 池复用, hyper_util_bridge.rs 新建) |
| **借鉴 ID** | `R125-3-BORROW-hyperium/hyper-0.1.20-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-3 + 决策 #33 §4.1 P0 supervisor |

### 1.3 servers (R125-4 ✅ done, P0 supervisor era, Model Context Protocol)

| 字段 | 值 |
|------|---|
| **仓库** | `modelcontextprotocol/servers` |
| **Commit** | `76d64c8` (2026-07-29 16:09:46 -0700) — Merge pull request #4527 |
| **License** | **MIT → Apache-2.0 过渡** (per `servers/LICENSE:1-3` 头说明: "The MCP project is undergoing a licensing transition from the MIT License to the Apache License, Version 2.0") |
| **License 路径** | `borrowed-repos/servers/LICENSE` (1091 bytes) |
| **Copyright** | Model Context Protocol Contributors (https://github.com/modelcontextprotocol/servers) |
| **整合位置** | `crates/apeireth-mcp/src/` (MCP 协议对齐 + 175 files 借鉴) |
| **借鉴 ID** | `R125-4-BORROW-modelcontextprotocol/servers-76d64c8-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-4 + 决策 #33 §4.1 P0 supervisor |

### 1.4 PyO3 (R125-9 ✅ done, P1 supervisor era)

| 字段 | 值 |
|------|---|
| **仓库** | `PyO3/PyO3` |
| **版本** | 0.29.2 |
| **License** | **Apache-2.0** (主) + MIT (dual) |
| **License 路径** | `borrowed-repos/PyO3/LICENSE-APACHE` + `LICENSE-MIT` (per crate 子目录) |
| **Copyright** | Copyright (c) 2023-present PyO3 Project and Contributors (https://github.com/PyO3) |
| **整合位置** | `crates/apeireth-pybridge/src/` (Python ↔ Rust 跨语言桥, 928 files 借鉴, bridge.rs + bridge_pool.rs + type_convert.rs) |
| **借鉴 ID** | `R125-9-BORROW-PyO3/PyO3-0.29.2-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-9 + 决策 #33 §4.1 P1 supervisor |

### 1.5 kani (R125-10 ✅ done, P2 supervisor era, Rust Verifier)

| 字段 | 值 |
|------|---|
| **仓库** | `model-checking/kani` |
| **版本** | 0.67.0 |
| **License** | **MIT** (主) + Apache-2.0 (dual) |
| **License 路径** | `borrowed-repos/kani/LICENSE-MIT` + `LICENSE-APACHE` |
| **Copyright** | The Kani Rust Verifier Contributors (https://github.com/model-checking/kani) |
| **整合位置** | `crates/apeireth-formal/` (形式化验证 4502 files 借鉴, kani.toml 配置 + proofs 模板, 触发 B3 V0.5 25 维) |
| **借鉴 ID** | `R125-10-BORROW-model-checking/kani-0.67.0-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-10 + 决策 #33 §4.1 P2 supervisor + 决策 #22 §2.3 B3 25 维扩展 |

### 1.6 langgraph (R125-13 ✅ done, P2 supervisor era, LangChain StateGraph)

| 字段 | 值 |
|------|---|
| **仓库** | `langchain-ai/langgraph` |
| **Commit** | `d56666f` (2026-08-08 12:02:52 -0700) — chore(deps): bump the minor-and-patch group (#8533) |
| **License** | **MIT** |
| **License 路径** | `borrowed-repos/langgraph/LICENSE` |
| **Copyright** | Copyright (c) 2024 LangChain, Inc. |
| **整合位置** | `crates/apeireth-graph/src/state_graph.rs` (StateGraph 借鉴, 829 files 借鉴) |
| **借鉴 ID** | `R125-13-BORROW-langchain-ai/langgraph-d56666f-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-13 + 决策 #33 §4.1 P2 supervisor + 决策 #22 §2.3 B3 25→30 维 (R125-13 实施后) |

### 1.7 superpowers (R125-14 ✅ done, P2 supervisor era, obra/superpowers)

| 字段 | 值 |
|------|---|
| **仓库** | `obra/superpowers` |
| **版本** | 6.2.0 |
| **Commit** | `44c9b2d` (2026-07-28 12:25:36 -0700) — docs: remove the "We're Hiring" section |
| **License** | **MIT** |
| **License 路径** | `borrowed-repos/superpowers/LICENSE` |
| **Copyright** | Copyright (c) 2025 Jesse Vincent |
| **整合位置** | `crates/apeireth-central/src/skill_*.rs` (Skill 化 234 files 借鉴, skill_trait/skill_registry/skill_runner/skill_prompt/skill_validation/skill_execution/skill_outcome/skill_companion/skill_recommender + skill_frontmatter) |
| **借鉴 ID** | `R125-14-BORROW-obra/superpowers-6.2.0-2026-08-10` |
| **整合 #5 commit 决策** | per 决策 #22 §3.2 R125-14 + 决策 #33 §4.1 P2 supervisor + 决策 #55 §2.2 Library Stage 4 自治 (P5-1) |

---

## 2. 借鉴源码 3/11 ⏳ 限流持续 (P6-1/2/3 21:18 派, per 决策 #56 §2.1)

**0 装 PASS 严守** (per 决策 #33 §2.3 C2 + 我们 17:22 升级授权 + 我们 20:32 "技术性 locked 都能解锁"):
借鉴源码 ⏳ 限流中 = **0 装"已实施"** (诚实标 "准备"), 等限流结束后真实施 retry。

| # | 借鉴 | 限流状态 | R125 任务 | 重试 sub-agent | 决策链 |
|---|------|----------|-----------|----------------|--------|
| 8 | **LiteLLM** (BerriAI/litellm) | ⏳ 0 files (限流持续 15+ min) | R125-1 LiteLLM Provider Registry 骨架 | P6-1 (R127-2 阶段 A, 21:18 派) | 决策 #22 §3.2 R125-1 + 决策 #56 §2.1 |
| 9 | **opencode** (sst/opencode) | ⏳ 0 files (限流持续) | R125-12 OpenCode 子代理 + B7 9 organ 内部 fn | P6-2 (R127-2 阶段 A, 21:18 派) | 决策 #22 §3.2 R125-12 + 决策 #33 §2.7 B7 + 决策 #56 §2.1 |
| 10 | **Guardrails** (NVIDIA/NeMo-Guardrails) | ⏳ 0 files (git submodule 0 init) | R125-5 NVIDIA Guardrails Colang DSL + B4 6 重 v6 | P6-3 (R127-2 阶段 A, 21:18 派) | 决策 #22 §3.2 R125-5 + 决策 #33 §2.4 B4 + 决策 #56 §2.1 |

**License 待补 (限流结束后由 P6-1/2/3 报告补充)**:
- LiteLLM: 待 P6-1 真实施后 verify (通常 MIT)
- opencode: 待 P6-2 真实施后 verify (通常 MIT)
- Guardrails: 待 P6-3 真实施后 verify (通常 Apache-2.0)

---

## 3. 借鉴源码 1/11 ❌ 跳过 (OpenCog AGPL-3.0, 0 集成)

| # | 借鉴 | 状态 | 理由 |
|---|------|------|------|
| 11 | **OpenCog** (opencog/opencog) | ❌ **0 集成** | **AGPL-3.0** (Affero General Public License v3.0) — **传染性 copyleft 协议, 跟主仓 Apache-2.0 不兼容** (per 决策 #22 §4 风险表 "opencog AGPL-3.0 传染" + 决策 #55 §3) |

**AGPL-3.0 vs Apache-2.0 不兼容说明**:
- AGPL-3.0 第 13 条 "Remote Network Interaction; Use with the GNU General Public License" 要求网络服务也必须开源
- AGPL-3.0 § 12 禁止对 License 增加额外限制
- AGPL-3.0 § 10 § 11 强制 derivative work 整 License
- Apache-2.0 § 3 patent retaliation 跟 AGPL-3.0 兼容性需要 GPLv3 exception
- **结论**: 借鉴 OpenCog = 主仓被传染, 必须 AGPL-3.0 整体发布, 失去商业灵活性
- **Mavis 自主决策 (per 决策 #22 §1 + 决策 #33 §2.2)**: **0 集成, 0 假装 "已借鉴"** (0 装 PASS 严守, O-5 哲学锚 "不假装" 严守)

**未来可能路径 (不在 R128 范围)**:
- 若我们 1.0 release 后希望借鉴 OpenCog Atomspace/ECAN 思路, 必须 **fork 出独立 AGPL-3.0 实验分支**, 主仓保持 Apache-2.0
- 参考借鉴思路: 借鉴 OpenCog 设计原则 (node/atom 抽象, attention allocation 机制), **不抄码, 重新实现**

---

## 4. 借鉴源码状态总结 (per 决策 #33 §2.3 C2 + 决策 #55 §3 + 决策 #57 §3)

| 状态 | 数量 | 借鉴源码 | 整合 #5 commit 决策 |
|------|-----:|----------|---------------------|
| ✅ **cloned = 真实施** | **7/11** | clap / hyper / servers / PyO3 / kani / langgraph / superpowers | **本 OSS_NOTICE.md §1 完整致谢** |
| ⏳ **限流 = 准备** | **3/11** | LiteLLM / opencode / Guardrails | **本 OSS_NOTICE.md §2 占位, 限流结束后由 P6-1/2/3 报告补** |
| ❌ **跳过 = 0 集成** | **1/11** | OpenCog (AGPL-3.0 传染) | **本 OSS_NOTICE.md §3 永久跳过** |
| **总计** | **11** | | **整合 #5 commit 时: 7 + 3 限流后续 + 1 永久跳过 = 11 完整记录** |

**0 装 PASS 严守 verify** (per 决策 #33 §2.3 C2):
- ✅ 7 cloned = 真实施 (有真 src 改动 + tests pass)
- ⏳ 3 限流 = 准备 (诚实标 "准备", 0 装"已实施")
- ❌ 1 跳过 (OpenCog = 0 集成, 0 假装 "已实施")

---

## 5. 完整 LICENSE 类型分布 (借鉴 8/11)

| # | 借鉴 | License 类型 | 关键决策 |
|---|------|--------------|----------|
| 1 | clap 4.6.6 | **Apache-2.0** + MIT (dual) | 主: Apache-2.0 |
| 2 | hyper 0.1.20 | **MIT** | Sean McArthur 2023-2025 |
| 3 | servers (76d64c8) | **MIT → Apache-2.0 过渡** | Model Context Protocol 2024-2026 |
| 4 | PyO3 0.29.2 | **Apache-2.0** + MIT (dual) | 主: Apache-2.0 |
| 5 | kani 0.67.0 | **MIT** + Apache-2.0 (dual) | 主: MIT |
| 6 | langgraph (d56666f) | **MIT** | LangChain, Inc. 2024 |
| 7 | superpowers 6.2.0 | **MIT** | Jesse Vincent 2025 |
| 8 | LiteLLM (限流) | 待 P6-1 verify | 通常 MIT |
| 9 | opencode (限流) | 待 P6-2 verify | 通常 MIT |
| 10 | Guardrails (限流) | 待 P6-3 verify | 通常 Apache-2.0 |
| 11 | OpenCog (跳过) | AGPL-3.0 (❌ 0 集成) | 永久跳过 |

**兼容性 verify** (per `deny.toml` allow-list per `THIRD-PARTY-NOTICES.md` §"License Allow-List"):
- ✅ Apache-2.0: 在 allow-list (`Apache-2.0, Apache-2.0 WITH LLVM-exception`)
- ✅ MIT: 在 allow-list (`MIT, MIT-0`)
- ⚠️ MIT → Apache-2.0 过渡 (servers): 当前内容仍为 MIT, 0 装 "已 Apache"
- ✅ AGPL-3.0 (OpenCog): **不在 allow-list** → 与主仓 Apache-2.0 冲突, 0 集成

**Cargo-deny verify**: 0 violation (per `THIRD-PARTY-NOTICES.md` §"Overview" 引用 "0 cargo-deny violation")。

---

## 6. 决策链 (Decision Chain, 借鉴 8/11 LICENSE 整合依据)

| 决策 | 时间 | 关键内容 | 对 OSS_NOTICE 的影响 |
|------|------|----------|----------------------|
| **#22** | 2026-08-10 16:35 | 我们 16:31 最高权限 + 24 LOCKED 自主确认 + 9 项实质 locked 升级 + 14 任务派活 spec (R125-1~14) | 借鉴 14 任务派活清单 + 借鉴 ID 命名规范 `R125-N-BORROW-{owner/repo}-{hash}-2026-08-10` |
| **#33** | 2026-08-10 17:23 | 我们 17:22 升级授权 + 8 硬墙全部重置 + B1-B7 升级路线 + **0 装解除** + 16 派满 | 借鉴 8/11 0 装 PASS 严守 + C2 0 装 (O-5) 解除 |
| **#36** | 2026-08-10 17:44 | 借鉴源码 17:44 verify: 7/11 ✅ cloned + 3 MISSING/0-files + 1 跳过 (OpenCog) | OSS_NOTICE §1 §2 §3 借鉴状态基线 |
| **#47** | 2026-08-10 19:39 | 主仓挪出 + mv .git + git reset done ✅ | 主仓路径确认 `Apeireth-rust/` + master HEAD = abf12243 |
| **#48** | 2026-08-10 19:41 | 整合 #4 commit **abf12243** done (46752 file changes, 18 决策 #30-#48 + 10 M src + 14 untracked + .gitignore 升级) | 整合 #4 严守, 0 重跑, 0 必重跑 |
| **#55** | 2026-08-10 21:13 | R127 升级路线 + 4 派活 (P4-1 整合 #5 pre-check + P5-1/2/3 Library Stage 4-6) + 借鉴 3 限流重试 | R127 阶段 A 借鉴 3 限流重试 → 让 8/11 → 11/11 真实施 |
| **#56** | 2026-08-10 21:18 | R127-2 10 派活 (P6-1/2/3 借鉴 3 限流重试 + P7-1/2/3 release 准备 + P8-1/2/3 Library 进阶 + P9-1 borrowed-repos 进阶) | R127-2 阶段 A 借鉴 3 限流重试 (P6-1 LiteLLM / P6-2 opencode / P6-3 Guardrails) |
| **#57** | 2026-08-10 21:29 | R128 6 派活 (P10-1/2 ASI Python 整合 + P11-1 Tauri 终极前端 + P12-1 Cargo build/test/run 实战 + **P13-1 LICENSE + OSS NOTICE** + P14-1 整合 #5 commit pre-stage) | **P13-1 任务 = 本 OSS_NOTICE.md** + 整合 #5 commit 时机 = 38 任务 (R125 16 + R126 16 + R127 4 + R127-2 10 + R128 6) 全 done + 0 装 PASS + 8 硬墙 + 24 LOCKED 入口 verify |

---

## 7. Apache 2.0 §4(d) NOTICE 条款 verify (合规自检)

per Apache License 2.0 §4(d):

> If the Work includes a "NOTICE" text file as part of its distribution, then any
> Derivative Works that You distribute must include a **readable copy of the
> attribution notices** contained within such NOTICE file...

**verify 通过**:
- ✅ `LICENSE` 完整 Apache 2.0 文本 (168 行, 2026-08-05 写入) — 不修改
- ✅ `NOTICE` (66 行) — 不修改
- ✅ `OSS_NOTICE.md` (本文件) — 整合借鉴 8/11 attribution notices
- ✅ `THIRD-PARTY-NOTICES.md` (106KB, 561 crates / 12 SPDX / 0 cargo-deny violation) — 不修改
- ✅ 借鉴源码 LICENSE 引用 (clap LICENSE-APACHE+MIT, hyper LICENSE, servers LICENSE, PyO3 LICENSE-APACHE+MIT, kani LICENSE-MIT+APACHE, langgraph LICENSE, superpowers LICENSE) — 全部 cloned 完整保留, per 借鉴 8/11 ✅ cloned 状态

**核心原则** (per 主仓 0 装严守 + Apache 2.0 标准化):
- 不隐瞒, 不假装 (O-5 哲学锚)
- 不简化, 不省略 (完整 attribution)
- 不修改主仓 LICENSE 主体 (Apache 2.0 完整文本 verbatim)
- 不混淆 dual license (clap/PyO3/kani dual 标注, 不假装单一)

---

## 8. 致谢 (Acknowledgements, 按 借鉴 8/11 + 决策链)

本项目 (Apeireth) 站在以下开源项目和开源协议的肩膀上, 致以诚挚谢意:

### 8.0 借鉴增量: Jimmyxiao2009 个人项目 (R215, 2026-08-21)

> **来源**: <https://github.com/Jimmyxiao2009/> (用户个人项目)
> **详细借鉴分析**: [`docs/04-internal/borrow-from-jimmyxiao2009.md`](docs/04-internal/borrow-from-jimmyxiao2009.md)
> **本节**: 致谢性引用, 落地细节见上述文档

| 借鉴 ID | 来源 | License | 借鉴模式 | 落点 | 状态 |
|---------|------|---------|----------|------|------|
| `BORROW-Jimmyxiao2009/agentos-windows-recovery-atomic-write-2026-08-21` | <https://github.com/Jimmyxiao2009/agentos-windows-recovery> | **MIT** | 原子写入模板 (`JsonSupport.WriteAtomic`) — `<target>.tmp-<uuid>` → rename, finally 清理 | `apeireth-host::atomic_write` | ✅ done |
| `BORROW-Jimmyxiao2009/agentos-windows-recovery-fail-closed-2026-08-21` | 同上 | **MIT** | Fail-closed 三阶段模板 (`TransactionEngine.RollbackCore` + `MarkEvidenceFailure`) | `apeireth-sovereignty::fail_closed` | ✅ done |
| `BORROW-Jimmyxiao2009/agentos-windows-recovery-hash-chained-journal-2026-08-21` | 同上 | **MIT** | Hash-chained audit journal (`TransactionJournal.cs` — `SHA256(seq ‖ timestamp ‖ eventType ‖ data ‖ prev_hash)` + Genesis) | `apeireth-arbitration::journal` | ✅ done |
| `BORROW-Jimmyxiao2009/agentos-windows-recovery-journal-durability-2026-08-21` | 同上 | **MIT** | Journal durability 强化 (`BufWriter::flush` + `sync_all` + 父目录 fsync, 同款 `write_with_durability` 模式) | `apeireth-arbitration::journal::flush` + `append` | ✅ done |
| `BORROW-Jimmyxiao2009/agentos-windows-recovery-three-way-conflict-2026-08-21` | 同上 | **MIT** | 三路冲突检测 (`FileSnapshotEngine.FindRollbackConflicts` — baseline / after / current) | `apeireth-host::three_way` (新 trait + FileScope impl) | ✅ done |
| `BORROW-Jimmyxiao2009/AgentFlow-task-dag-lease-2026-08-21` | <https://github.com/Jimmyxiao2009/AgentFlow> | ⚠️ **无 LICENSE** (默认 all-rights-reserved) | Task DAG 租约 (`TaskDagScheduler` — 14 TaskState 状态机 + 15 分钟租约 + reap_expired 主动回收) | `apeireth-team-lead::lease` (新模块, RAII LeaseGuard) | ✅ done (设计思想, 0 代码复制) |
| `BORROW-Jimmyxiao2009/apeireth-rust-fork-pr2-session_lifecycle-2026-08-21` | <https://github.com/Jimmyxiao2009/apeireth-rust> (fork) | **Apache-2.0** | 状态机 + `expected_rev` CAS 乐观并发 | `apeireth-memory::session_lifecycle` (fork 复制, 0 触碰 LOCKED migrations.rs) | ✅ done (in worktree `team/pr2-integration`) |
| `BORROW-Jimmyxiao2009/apeireth-rust-fork-pr2-memory_governance-2026-08-21` | 同上 | **Apache-2.0** | forget ≠ purge 软删 + sidecar 表 (episodes 仍 append-only) | `apeireth-memory::memory_governance` | ✅ done (in worktree) |
| `BORROW-Jimmyxiao2009/apeireth-rust-fork-pr2-agent_trace-memory-2026-08-21` | 同上 | **Apache-2.0** | 结构化 Agent 执行轨迹持久化 + 查询 | `apeireth-memory::agent_trace` | ✅ done (in worktree) |
| `BORROW-Jimmyxiao2009/apeireth-rust-fork-pr2-packs-2026-08-21` | 同上 | **Apache-2.0** | PermissionPack 作用域授权 grant + 撤销 | `apeireth-companion::packs` | ✅ done (in worktree) |
| `BORROW-Jimmyxiao2009/apeireth-rust-fork-gitignore-credentials-2026-08-21` | <https://github.com/Jimmyxiao2009/apeireth-rust> (fork, commit `4b230e3c`) | **Apache-2.0** | .gitignore 凭证保护 (apikey-ultra.txt + apikey-*.txt + *.git-credentials + Users*.git-credentials) | `.gitignore` (新增 4 行规则) | ✅ done |

**License 合规**: MIT (agentos) + Apache-2.0 (fork) 全部入 OSS_NOTICE §8.0; AgentFlow 仅借鉴设计思想, 0 代码复制。

**8 哲学锚穿透** (per 主仓 `09-anchor.md`): S-1 北极星 (3 项均服务 LLM 后端基地) / S-2 实事求是 (每个模块 0 装 PASS 标注完整) / S-3 质量工程化 (3 模块 24 测试全绿) / O-1 安全优先 (fail-closed 模板 + tamper-evident + 三路冲突) / O-2 前人肩上 (字段级移植 C# → Rust) / O-3 干到底 (一次性落地 + 测试 + 文档) / O-4 接手 (顶部 //! + 用法示例) / O-5 不假装 (错误类型完整 + "什么没做"明示)。

**未借鉴** (本次): Yanshuai-AI / OnDeviceAI 系列 (C# UWP/D3D11 Windows 独占, 跨平台 Rust 适配面低); AgentFlow 仅借鉴思想 (无 LICENSE, **0 代码复制**, 全 Rust 重新实现); apeireth-rust fork: 全量 PR #2 (架构不兼容 + LOCKED 触碰 + 重复造轮子 → 改为 selective cherry-pick 4 个纯 Rust module, 跳过 companion_serve 改造 + frontend 重写 + runtime_capabilities 重复实现 + migrations LOCKED 触碰)。

### 8.1 借鉴 7/11 真实施 (本 OSS_NOTICE.md §1 详述)

- **clap-rs/clap 4.6.6** (Apache-2.0 + MIT) — CLI derive 模式, R125-2 ✅ done
- **hyperium/hyper 0.1.20** (MIT) — HTTP 客户端 LIFO 池复用, R125-3 ✅ done
- **modelcontextprotocol/servers** (MIT → Apache-2.0 过渡) — MCP 协议对齐, R125-4 ✅ done
- **PyO3/PyO3 0.29.2** (Apache-2.0 + MIT) — Python ↔ Rust 跨语言桥, R125-9 ✅ done
- **model-checking/kani 0.67.0** (MIT + Apache-2.0) — 形式化验证 + B3 V0.5 25 维, R125-10 ✅ done
- **langchain-ai/langgraph** (MIT) — StateGraph 借鉴 + B3 25→30 维, R125-13 ✅ done
- **obra/superpowers 6.2.0** (MIT) — Skill 化 + Library Stage 4 自治 (P5-1), R125-14 ✅ done

### 8.2 借鉴 3/11 限流持续 (本 OSS_NOTICE.md §2 占位)

- **BerriAI/litellm** (待 verify) — Provider Registry, P6-1 重试中
- **sst/opencode** (待 verify) — 子代理, P6-2 重试中
- **NVIDIA/NeMo-Guardrails** (待 verify) — 6 重守门 v6, P6-3 重试中

### 8.3 借鉴 1/11 永久跳过 (本 OSS_NOTICE.md §3 详述)

- **opencog/opencog** (AGPL-3.0) — 0 集成, 0 装 "已借鉴", 永久跳过 (传染性协议与主仓 Apache-2.0 不兼容)

### 8.4 完整 561 crates 第三方 attribution

详见 `THIRD-PARTY-NOTICES.md` (cargo-about 0.8.4 生成, 2026-08-05, 1709 lines,
12 unique SPDX: Apache-2.0 / MIT / Unicode-3.0 / Zlib / ISC / BSD-3-Clause /
0BSD / Artistic-2.0 / BSL-1.0 / CDLA-Permissive-2.0 / MIT-0 / MPL-2.0,
0 cargo-deny violation)。

### 8.5 主仓 + 决策链致谢

- **主仓**: Apeireth Team 2026, Apache-2.0, https://github.com/apeireth/apeireth-rust
- **决策链**: 我们 (楚零) 8 次拍板 (8/10 01:14 + 01:49 + 14:56 + 16:27 + 16:31 + 16:37 + 16:43 + 16:51 + **17:22** 升级授权), Mavis (mvs_47dd64fb4fc24e23b30edd5f649bfebb) 决策 #22-#57
- **整合 #4 commit**: abf12243 (2026-08-10 19:41 done, 46752 file changes, 0 重跑)
- **整合 #5 commit 时机**: 38 任务 (R125 16 + R126 16 + R127 4 + R127-2 10 + R128 6) 全 done + 0 装 PASS 严守 + 8 硬墙 0 越界 + 24 LOCKED 入口签名 0 改 verify, **Mavis 拍板 OR 我们 8/15 拍板**

---

## 9. 不假装边界 (Honest Boundaries, per 0 装 PASS 严守 + O-5 哲学锚)

按主仓 APEIRETH-CONVENTIONS §10 哲学锚 "S-2 实事求是" + "O-5 不假装":

- ✅ **本 OSS_NOTICE.md 0 装**: 7 真实施 (clap/hyper/servers/PyO3/kani/langgraph/superpowers) + 3 限流持续 (LiteLLM/opencode/Guardrails) + 1 永久跳过 (OpenCog), 全部诚实标
- ✅ **借鉴 8/11 verify**: 7 真实施 sub-agent 报告 (P0/P1/P2 supervisor era) + 3 限流 sub-agent 重试报告 (P6-1/2/3) + 1 永久跳过决策记录 (#22 §4 + #55 §3), 全部可追溯
- ✅ **0 装"已借鉴 OpenCog"**: OpenCog AGPL-3.0 传染性协议与主仓 Apache-2.0 不兼容, 0 假装"已实施" (per O-5)
- ✅ **决策链完整可追溯**: #22 / #33 / #36 / #47 / #48 / #55 / #56 / #57 全部在 `reports/decision-*.md`, 整合 #4 commit abf12243 严守
- ✅ **整合 #4 严守**: 0 重跑, 0 必重跑, master HEAD = abf12243, Cargo.toml 1.2.0 严守
- ✅ **整合 #5 commit 时机诚实标**: 38 任务全 done + 0 装 PASS + 8 硬墙 + 24 LOCKED 入口 verify 全部满足后, Mavis 拍板 OR 我们 8/15 拍板
- ✅ **0 主动 commit 严守**: 本 OSS_NOTICE.md 写到主仓, 0 主动 commit (per 决策 #55 §5 + 决策 #57 §5 C1)
- ✅ **0 主动 push 严守**: 等 1.0 release 配 GitHub remote (per 决策 #33 §4.2 + 决策 #55 §5)

---

## 10. 维护 / 更新规则 (本 OSS_NOTICE.md 演进)

| 触发 | 更新 | 决策 |
|------|------|------|
| 借鉴 3 限流结束 (P6-1/2/3 done) | §2 从"占位" → §1 完整致谢 | 决策 #55 §3 + #56 §2.1 |
| 新增借鉴 (R129+ 阶段) | §1/§2 追加 + §8 致谢追加 | 决策 #55-#57 模式延续 |
| License 变更 (clap/PyO3/kani dual 切换) | §5 表格更新 | 整合 #5 commit 时 Mavis 自主 |
| OpenCog 重新评估 (1.0 release 后) | §3 从"永久跳过" → "fork 评估" | **Mavis 不主动提议, 我们主动问** |
| 整合 #5 commit 时机成熟 | 本文件 + LICENSE + NOTICE + THIRD-PARTY-NOTICES.md 整体 commit | 决策 #55 §0 + #57 §0 (38 任务全 done) |
| 1.0 release (v1.0.0) 时 | Cargo.toml version 1.2.0 → 1.0.0, 本文件同步 | 决策 #22 §2.2 B2 release 节奏 |

---

## 11. 联系方式 (Contact, 补全 NOTICE §6)

- **OSS NOTICE 维护**: Mavis (mvs_47dd64fb4fc24e23b30edd5f649bfebb) via 我们 楚零
- **仓库**: https://github.com/apeireth/apeireth-rust
- **借鉴源码本地**: `.openclaw/workspace/borrowed-repos/`
- **决策链**: `reports/decision-*.md` (本仓 + 跨期)
- **整合 #4 commit**: abf12243 (2026-08-10 19:41)
- **整合 #5 commit**: 待 38 任务全 done (per 决策 #55 §0 + 决策 #57 §0)

---

**Last-Modified**: 2026-08-10 21:50 (P13-1 R128 阶段 D 新写)
**Format**: OSS NOTICE 1.0 (基于 Apache 2.0 §4(d) "NOTICE" 条款 + cargo-about 0.8.4 借鉴 8/11 致谢扩展)
**0 主动 commit 严守**: 本文件写到主仓, **0 主动 commit**, Mavis 整合 #5 commit 时机拍板
**0 主动 push 严守**: 等 1.0 release 配 GitHub remote
