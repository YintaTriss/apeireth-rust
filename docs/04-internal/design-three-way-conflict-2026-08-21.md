# 三路冲突检测机制 (Three-Way Conflict Detection) — 设计

> **生成时间**: 2026-08-21
> **作者**: 主代理 (本会话)
> **状态**: ✅ Phase 1 设计 + Phase 2 落地完成
> **哲学锚穿透**: O-1 安全优先 (升 R126), S-2 实事求是, O-5 不假装, O-2 走在前人肩上

---

## 0. 背景

`apeireth-upgrade::rollback` (7 阶段 OTA 的阶段 4 image switch 与阶段 6 rollback)、
`apeireth-arbitration::undo`、`apeireth-sovereignty::restore` 等 destructive 操作
都面临同一个风险:**从 capture baseline 到实际 destructive 操作之间,磁盘可能被外部
进程修改过**,这时如果还按 baseline 假设去覆盖,就会把别人的工作吞掉。

**例子**: 用户点了 upgrade → 我们 capture baseline `v1` → 用户手动去磁盘删了几个
老文件 → 我们开始 destructive 阶段 → 系统按 `v1` baseline 假设覆盖 — 结果是用户
手动删的文件被"恢复"回来,看起来像我们没做 destructive 操作。

**借鉴来源**: [Jimmyxiao2009/agentos-windows-recovery](https://github.com/Jimmyxiao2009/agentos-windows-recovery)
(MIT License) — 三路对比 `(baseline, expected_after, current)` 检测冲突是工业界
**git merge / database WAL / VM snapshot** 都用的模式,我们直接吸收。

---

## 1. 借用 ID 与 License

- **借用 ID**: `BORROW-Jimmyxiao2009/agentos-windows-recovery-three-way-conflict-2026-08-21`
- **来源仓库**: https://github.com/Jimmyxiao2009/agentos-windows-recovery (MIT, Copyright 2026 Jimmyxiao2009)
- **License**: MIT (本实现署名归 apeireth-rust 主仓, License 详见 `LICENSE-MIT-Jimmyxiao2009`)
- **与 P0-1 原子写入的关系**: 同源、同日、同 PR 周期内提交,放在同一 facade 内(`apeireth-host`)。

---

## 2. API 形状 (Rust trait + struct + 错误)

```rust
use serde::{Serialize, de::DeserializeOwned};

/// 任何能产出三种 snapshot 的资源都可参与三路对比。
///
/// - `capture_baseline()` 在 destructive 操作**之前**调用一次,通常写到
///   saved snapshot 文件。
/// - `expected_after()` 推导"该操作完成后应有的状态",通常用 operation
///   描述推导 (例如: upgrade rollback 后 image v0 应存在)。
/// - `probe_current()` 在 destructive 操作**之前**再现场 probe 一次磁盘,
///   用来发现外部改动。
pub trait ThreeWayComparable {
    type Snapshot: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug;

    fn capture_baseline(&self) -> Result<Self::Snapshot, ThreeWayError>;
    fn probe_current(&self) -> Result<Self::Snapshot, ThreeWayError>;
    fn expected_after(&self) -> Result<Self::Snapshot, ThreeWayError>;
}

/// 冲突报告:`current` 与 `baseline` 的逐项 diff。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictDiff {
    /// 路径 → (baseline 中的快照值, current 中的快照值)。
    pub changed_paths: BTreeMap<String, (String, String)>,
    /// baseline 不存在但 current 存在 (外部新增)。
    pub added_paths: BTreeMap<String, String>,
    /// baseline 存在但 current 缺失 (外部删除)。
    pub removed_paths: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct ThreeWayConflict<S: ThreeWayComparable> {
    pub baseline: S::Snapshot,
    pub current: S::Snapshot,
    pub diff: ConflictDiff,
}

/// 主入口。返回 `Ok(None)` = 无冲突,`Ok(Some(c))` = 有冲突由 caller 决定如何处理,
/// `Err(_)` = probe 失败 (例如目录不存在)。
///
/// **关键设计**: baseline 由 caller 提前 capture,本函数不重新 capture。
/// 否则在 destructive 前最后一刻 capture,会把外部改动"基线化",导致冲突永远查不出。
pub fn detect<S: ThreeWayComparable>(
    c: &S,
    baseline: S::Snapshot,
) -> Result<Option<ThreeWayConflict<S>>, ThreeWayError>;

/// helper: caller 拿到 `Some(conflict)` 后可以决定
/// `if user_opted_in(force) { proceed } else { reject }`。
pub fn detect_with_force<S: ThreeWayComparable>(
    c: &S,
    baseline: S::Snapshot,
    force: bool,
) -> Result<DetectOutcome<S>, ThreeWayError>;

pub enum DetectOutcome<S: ThreeWayComparable> {
    NoConflict,
    ConflictBypassedByForce(ThreeWayConflict<S>),  // force=true 跳过
    Conflict(ThreeWayConflict<S>),                  // force=false 拒绝
}
```

---

## 3. 文件级示例 (FileScope)

最小落地的实现 scope:**对一个目录递归 capture 每个文件的 SHA-256(content) + mtime + size**,
存为 `BTreeMap<RelativePath, FileEntry>`,这样 diff 可以精确到 path。

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub sha256_hex: String,   // 64-char lowercase hex
    pub size: u64,
    pub mtime_unix_ms: i64,    // millis since epoch (跨平台稳)
}

pub type FileSnapshot = BTreeMap<String, FileEntry>; // path -> entry

pub struct FileScope {
    pub root: PathBuf,
    /// 排除的相对路径(例如 `.git`, `target/`)
    pub excludes: Vec<String>,
}

impl ThreeWayComparable for FileScope {
    type Snapshot = FileSnapshot;
    fn capture_baseline(&self) -> Result<FileSnapshot, ThreeWayError> { /* walk + sha256 */ }
    fn probe_current(&self)  -> Result<FileSnapshot, ThreeWayError> { /* walk + sha256 */ }
    fn expected_after(&self) -> Result<FileSnapshot, ThreeWayError> {
        // FileScope 默认 = baseline 不变(只是 describe 一下)。调用方可以 wrap 一个
        // 携带 expected mutation 的 enum,这里保持 minimal.
        self.capture_baseline()
    }
}
```

---

## 4. 使用模式

### 4.1 Pattern A — strict (default)

```rust
let scope = FileScope { root: target_dir, excludes: vec!["target".into()] };
// 1) 在 destructive 窗口**开启前** capture baseline (持久化到 disk)
let baseline = scope.capture_baseline()?;
// ... 用户思考 / 等待 / 任何耗时步骤 ...
// 2) destructive 前最后一次 detect
match detect_with_force(&scope, baseline, false)? {
    DetectOutcome::NoConflict => {
        // 推进 destructive 操作
    }
    DetectOutcome::Conflict(c) => {
        return Err(format!(
            "检测到 {} 个外部改动,拒绝执行。请用户确认后用 --force 重试:\n{}",
            c.diff.changed_paths.len() + c.diff.added_paths.len() + c.diff.removed_paths.len(),
            format_diff(&c.diff),
        ));
    }
    DetectOutcome::ConflictBypassedByForce(_) => unreachable!(),
}
```

### 4.2 Pattern B — force override

```rust
let scope = FileScope { ... };
let baseline = load_persisted_baseline()?;
if let DetectOutcome::Conflict(c) | DetectOutcome::ConflictBypassedByForce(c) =
    detect_with_force(&scope, baseline, force)?
{
    tracing::warn!(
        paths_changed = c.diff.changed_paths.len(),
        paths_added = c.diff.added_paths.len(),
        paths_removed = c.diff.removed_paths.len(),
        "three-way conflict detected; force={}",
        force
    );
    log_audit_trail(&c);  // 必须留下记录
}
// 继续 destructive
```

### 4.3 Caller 责任

**下列事情不在本 utility 范围内 — 必须由 caller 处理**:

1. **文件锁 / 互斥** — destructive 期间禁止其他进程写。
2. **幂等 restore / 重试** — 失败后怎么回到 baseline。
3. **权限 / owner 检查** — 当前 FileScope 只看内容 hash,不看 ACL。
4. **symlink / hardlink 跟随策略** — 当前 minimal 版本 follow symlink。
5. **force override 的签名/审计** — caller 自己决定是否需要签名。

(per S-2 实事求是 / O-5 不假装:本 utility 不是 silver bullet,它只是一个**最薄一层
的安全网**,真正的安全要靠 caller 自己堆防御。)

---

## 5. 测试矩阵 (5+ 测试)

| # | 测试名 | 场景 | 期望 |
|---|--------|------|------|
| 1 | `empty_dir_baseline_equals_current` | 空目录 | `Ok(None)` (无冲突) |
| 2 | `single_file_modified` | baseline 后改一个文件 | `Some(conflict)` 且 diff 包含该 path |
| 3 | `single_file_unchanged` | baseline 后无变化 | `Ok(None)` |
| 4 | `force_override_bypasses_conflict` | force=true 即便有冲突 | `ConflictBypassedByForce(c)` |
| 5 | `nested_dir_changed` | 嵌套子目录文件被改 | conflict 传播到嵌套文件 |
| 6 | `non_existent_dir_baseline` | probe 路径不存在 | `Err(MissingRoot)` (graceful) |
| 7 | `expected_after_differs_from_baseline_no_external_change` | baseline == current,但 expected_after 不同 | `Ok(None)` (冲突定义只看 baseline vs current,不看 expected vs current — 是 caller 责任) |

> 注: 测试 #7 揭示了一个**设计决策**:`detect()` **不**报 `expected_after vs current` 的 diff —
> 这是 caller 责任 (因为 expected_after 的"什么算偏差"是 domain-specific)。
> Utility 只负责最关键的安全问题:**baseline 到 destructive 之间有没有外部改动**。

---

## 6. LOCATION 选择 — 选 (5):`apeireth-host::three_way`

**候选位置**:
1. ❌ `apeireth-three-way` 新独立 crate — overkill,只有一个 trait + 一个 struct,workspace 已经有 100+ crates,加新 crate 编译图变大但收益小。
2. ❌ `apeireth-upgrade::three_way` — 这个机制 upgrade / arbitration / sovereignty 都要用,塞 upgrade 会让其他 crate 反向依赖。
3. ❌ `apeireth-core::three_way` — `apeireth-core` 是 organism invariants + 类型,加 utility 污染关注点。
4. ❌ `apeireth-storage::three_way` — `apeireth-storage` 还没长大,且语义是"持久化"非"校验"。
5. ✅ **`apeireth-host::three_way`** — host facade 已经有 keyring / machine_id / atomic_write (R178 落地),都是"host 提供的安全/基础设施服务",加 three_way 是**职责一致**的扩张,且 `apeireth-upgrade`、`apeireth-arbitration`、`apeireth-sovereignty` 都已经依赖 `apeireth-host` (per audit R215),**零额外依赖链**。

**决策**: 选 (5),理由如上。

---

## 7. 风险与约束

### 7.1 不动 3 不可变脊柱

- 不动 `workspace.version`(在 workspace Cargo.toml,本 utility 走 `version.workspace = true`)。
- 不动 `apeireth-host/Cargo.toml` 的 `[dependencies]` 列表 — sha2 / serde / thiserror / fs-err 都已经在了,**0 新 dep**。
- 不动 `apeireth-host/src/lib.rs` 的现有模块声明,只在末尾追加。

### 7.2 不假装清单 (O-5 不假装)

落地时必须明确写出"什么没做":

1. **不做 file lock / mutex** — caller 责任。
2. **不做幂等 retry** — caller 责任。
3. **不做权限 / owner 检查** — 当前 minimal 版本只看 content hash。
4. **不做 symlink / hardlink 策略** — 默认 follow,可能误判 dangling symlink 为"被外部删"。
5. **不做 expected_after vs current 的 semantic diff** — utility 只管 baseline vs current,expected_after 由 caller 在 trait 实现里描述。
6. **不做跨平台 mtime 精度归一** — NTFS 100ns / ext4 ns / FAT 2s,我们只用 `unix_ms`,跨时区/跨 DST 不处理。
7. **不做 content-addressed storage** — snapshot 还在调用方内存,大型目录 (GB 级) 会爆内存;后续可加 streaming。

---

## 8. 集成落点 (首调用方)

**Phase 2 之后的下一步**(不在本次任务范围):
- `apeireth-upgrade::rollback` 阶段 4 image switch 前调用 `detect_with_force`。
- `apeireth-upgrade::rollback` 阶段 6 rollback 前调用。
- `apeireth-arbitration::undo` 前调用。
- `apeireth-sovereignty::restore` 前调用。

本次只做 utility,**不**做集成(避免 scope creep)。

---

## 9. 验收清单

- [x] `crates/apeireth-host/src/three_way.rs` 存在且 `cargo build -p apeireth-host` 通过。
- [x] `cargo test -p apeireth-host --lib three_way` ≥ 5 passed 0 failed。
- [x] `cargo clippy -p apeireth-host --lib -- -D warnings` 0 warnings。
- [x] `apeireth-host/src/lib.rs` re-export 5 个 pub item + 1 个 module。
- [x] 借鉴 ID 在 module doc 与本设计文件顶部都出现。
- [x] 不假装清单 (§7.2) 在 module doc 与本设计文件都出现。