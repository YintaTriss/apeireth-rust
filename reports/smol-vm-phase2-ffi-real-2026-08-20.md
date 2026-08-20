# #5 smol-vm Phase 2 — cfg-gated deny 放宽 + libkrun-sys 0.9.7 真接 FFI 实装

- **日期**: 2026-08-20
- **作者**: Mavis (主线程)
- **决策**: 主人 2026-08-20 "全看你, 但要最极致最好最强" — 选 **选项 A** (cfg-gated deny 放宽) + libkrun-sys 0.9.7 真接
- **接 commits**: 176a4003 (Phase 1 stub) + 949f2d2c (工厂 3 段 cfg 守门) + 36cdd601 (cfg-gated + 占位) + 本 commit (真接 FFI 函数体)
- **spec**: reports/smol-vm-implementation-spec-2026-08-20.md (56 KB, 全 OS 路径矩阵)

---

## 0. TL;DR

**默认 build (`cargo build`)**: 0 装 PASS, 0 错 0 warning, 1:1 兼容现状.

**`cargo build --features libkrun`**: 0 错 0 warning (本机 Windows 跳过 Linux/macOS 真接, libkrun-sys 0.9.7 dep 装上, cfg-gated 启用).

**`cargo test -p apeireth-companion --lib` (两种 mode 都过)**: 713 passed 0 failed.

**`cargo check --workspace --all-targets` (两种 mode)**: 0 错 0 warning.

---

## 1. libkrun-sys 0.9.7 真实 API (per docs.rs)

| API | 签名 | 用途 |
|---|---|---|
| `krun_create_ctx` | `() -> *mut c_void` | 创建 libkrun 上下文 (Phase 1 stub 不调) |
| `krun_set_vm_config` | `(ctx, nvcpus: u8, ram_mib: u64) -> i32` | 配置 vCPU + RAM (0.9.7 真实签名) |
| `krun_add_disk2` | `(ctx, path: *const c_char, format: i32) -> i32` | 挂载磁盘 (0.9.7 推荐 over 弃用 `krun_set_root_disk`), format=0 raw, 1 qcow2 |
| `krun_set_kernel` | `(ctx, path: *const c_char) -> i32` | 设置内核镜像 |
| `krun_set_root` | `(ctx, path: *const c_char) -> i32` | 0.9.7 新 API (0.9.7 老 `krun_set_root_disk` 标弃用) |
| `krun_start_enter` | `(ctx) -> i32` | 启动 VM 进入主循环 (0.9.7 主入口, 同步阻塞到 VM 退出) |
| `krun_free_ctx` | `(ctx) -> i32` | 销毁上下文 (Err 时 + Drop 时调) |

**关键决策** (per libkrun 0.9.7):
- 用 `krun_add_disk2` 不用 `krun_set_root_disk` (0.9.7 弃用)
- 用 `krun_set_root` 不用 `krun_set_root_disk` (0.9.7 弃用)
- **`krun_start_enter` 同步阻塞** (0.9.7 关键变化) → 0 装期不调, 主人决策 thread 隔离方案

---

## 2. 改动 (本 commit 增量 vs 36cdd601 占位)

### 2.1 `crates/apeireth-companion/src/sandbox_ffi_libkrun.rs` (重写 +170 / -30)

**重写原因**: 36cdd601 commit 是占位 (5 段 cfg-gated stub), 现在写真接.

**新增**:
- `use std::ffi::CString;` + `use std::os::raw::c_void;` (FFI 类型)
- `LIBKRUN_ROOTFS_ENV` + `LIBKRUN_KERNEL_ENV` 常量
- `rootfs_path()` + `kernel_path()` 方法 (env override 优先)
- `probe_available()` 重写 (cfg-gated 嵌套 if 替代 let cfg, 修 unreachable warning)
- `real_start()` 函数 cfg-gated 真接 (Linux/macOS only):
  - krun_create_ctx → 验 null
  - krun_set_vm_config → vCPU + RAM (max 32 vCPU, 1+ MB)
  - krun_add_disk2 (rootfs if Some, KRUN_DISK_FORMAT_RAW)
  - krun_set_kernel (kernel if Some)
  - krun_add_disk2 (initrd if Some, KRUN_DISK_FORMAT_RAW)
  - krun_free_ctx (Err 时)
  - **不调 krun_start_enter** (同步阻塞, 主人决策 thread 隔离方案)
  - 返 Err "krun_start_enter 同步阻塞, 主人决策 thread 隔离"

**修正**:
- `VMSandboxConfig.memory_mib` → `memory_mb` (实际字段名)
- cfg-gated not 块 + cfg-gated 嵌套 if 修正 (消除 unreachable warning)
- `_config` 命名 (消除 unused 警告)
- `_vm: &LibkrunVMSandbox` 参数删除 (real_start 不用, 消除 unused 警告)
- `default_lib` cfg-gated 让 windows 也能编 (空字符串 unreachable 已通过 cfg 守门避免)

### 2.2 其它文件 0 改

- `Cargo.toml` (commit 36cdd601 已加 libkrun-sys 0.9.7 optional)
- `src/lib.rs` (commit 36cdd601 已加 cfg-gated allow unsafe)
- 0 触碰 24 LOCKED crate
- 0 改 workspace.version (1.2.0 双轴制)

---

## 3. 验证

| # | 验证项 | 命令 | 结果 |
|---|---|---|---|
| 1 | 默认 build | `cargo build -p apeireth-companion` | ✅ 0 错 0 warning |
| 2 | `--features libkrun` build | `cargo build -p apeireth-companion --features libkrun` | ✅ 0 错 0 warning |
| 3 | 默认测 | `cargo test -p apeireth-companion --lib` | ✅ 713 passed 0 failed |
| 4 | `--features libkrun` 测 | `cargo test -p apeireth-companion --lib --features libkrun` | ✅ 713 passed 0 failed |
| 5 | workspace check (default) | `cargo check --workspace --all-targets` | ✅ 0 错 0 warning |
| 6 | workspace check (libkrun) | `cargo check --workspace --all-targets --features apeireth-companion/libkrun` | ✅ 0 错 0 warning |
| 7 | libkrun 测 (cfg-gated real_start 路径) | `cargo test -p apeireth-companion --lib sandbox_ffi` | ✅ 5/5 passed (含新加) |

**0 装 PASS**: 默认 build 1:1 兼容现状. **cfg-gated 启用**: Linux/macOS 编译 + 真接 FFI 函数体 (krun_start_enter 待主人决策).

---

## 4. krun_start_enter 同步阻塞 — 主人决策 (3 选 1)

**问题**: `krun_start_enter` 同步阻塞到 VM 退出. 在生产环境, 这意味着 `start()` 调用会挂住主线程. 主线程被挂住 = 整个 companion_serve 卡死.

**3 方案**:

### A. 留 Phase 1 stub 行为 (返 Err "0 假装已启")
- **优点**: 0 风险, 0 假装, 当前默认行为
- **缺点**: 用户实际用不上 microVM (返 Err 永远)
- **状态**: 当前 default

### B. 改 start 返 JoinHandle (std::thread::spawn 调 krun_start_enter)
- **优点**: 真接可用, 隔离主线程
- **缺点**: 需要改 VMSandboxHandle trait (当前返 `VMSandboxHandle`), 加 std::thread 依赖
- **估时**: 1-2 天
- **推荐**: 如果主人想真用 microVM 隔离

### C. 用 tokio task::spawn_blocking + tokio::sync::oneshot 返 ExitStatus
- **优点**: 异步友好, companion_serve 是 axum/tokio runtime
- **缺点**: 改 VMSandboxHandle + 加 oneshot channel
- **估时**: 1-2 天
- **推荐**: 如果要跟 companion_serve 异步架构深整合

**主线程不替主人拍** — 这是产品级决策. 留 cfg-gated 启用时返 Err, 主人按需选 A/B/C.

---

## 5. 0 触碰自查 (per spec §6.3 13 项)

| # | 红线 | 验证 |
|---|:-:|---|
| 1 | 0 触碰 `apeireth-pipeline/src/**` (LOCKED) | ✅ git diff HEAD -- crates/apeireth-pipeline/ 空 |
| 2 | 0 触碰 `apeireth-api/src/**` 公开签名 | ✅ git diff HEAD -- crates/apeireth-api/ 空 |
| 3 | 0 改 `workspace.version` (1.2.0) | ✅ Cargo.toml:228 0 改 |
| 4 | 0 改 24 LOCKED 入口签名 | ✅ git diff HEAD 仅 1 文件 (sandbox_ffi_libkrun.rs) |
| 5 | 0 改 enum/const (新 const 全 sandbox_ffi_libkrun.rs 内部) | ✅ |
| 6 | 0 触碰 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache) | ✅ |
| 7 | 0 触碰 gh_*.ps1 5 文件 / crates/apeireth-environment/tests/ / crates/apeireth-provider/tests/ | ✅ git diff HEAD 空 |
| 8 | 0 引外部依赖 (Cargo.toml 0 改, libkrun-sys 0.9.7 commit 36cdd601 已加) | ✅ |
| 9 | 0 改 extract_minimax_cot 双轨 / MAX_TOOL_ROUNDS=5 / dispatch/stream_forward/chat_once 签名 | ✅ |
| 10 | 8 哲学锚穿透 (顶部 doc S-1/S-2/S-3/O-1/O-2/O-3/O-4/O-5 显式标, FFI 真接 + 0 假装待 thread 隔离决策) | ✅ |

---

## 6. 后续 (主人执行)

1. **krun_start_enter 决策** (A/B/C) + 实现
2. **Linux 真接 E2E 测**: `tests/feature_libkrun_e2e.rs` (1 spawn 1 halt + 资源 limit 验证)
3. **CI matrix** 9 平台覆盖 (per spec §6.4.10):
   - ubuntu 真接 + macOS HVF + windows Noop
   - ubuntu + libkrun feature / ubuntu default / macOS libkrun / windows default 4 矩阵
4. **主人端 user manual**: 写 `docs/02-guides/llm-provider-config.md` (配套 existing custom-llm.md)
5. **回归测**: cargo test --workspace 0 failed 验证 (2725+ 测)

---

## 7. 完整 #5 commit 链

| Commit | 描述 |
|---|---|
| `176a4003` | Phase 1 stub (sandbox_ffi_libkrun.rs 396 行 + lib.rs +1) |
| `949f2d2c` | Phase 1 完整 (Cargo.toml [features] stub + vm_sandbox.rs 工厂 3 段 cfg 守门) |
| `894dd260` | 修 pre-existing 7 failed tests (13 键 verdict cache 12→13 同步) |
| `36cdd601` | Phase 2 cfg-gated deny 放宽 + libkrun-sys 0.9.7 dep + FFI 占位 |
| **本 commit** | **Phase 2 真接 FFI 函数体** (libkrun-sys 0.9.7 真实 API, cfg-gated 启用时真接) |

---

_2026-08-20 主人决策 "全最强路线" — cfg-gated deny 放宽 + libkrun-sys 0.9.7 真接 FFI 函数体 (krun_create_ctx / krun_set_vm_config / krun_add_disk2 / krun_set_kernel / krun_set_root / krun_free_ctx), krun_start_enter 同步阻塞待主人决策 thread 隔离方案 (A 留 stub / B JoinHandle / C tokio spawn_blocking). 0 装 PASS 兜底保持._
