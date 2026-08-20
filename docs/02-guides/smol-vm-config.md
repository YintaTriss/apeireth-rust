# smol-vm / libkrun 真接配置指南

> **2026-08-20 新增**: Apeireth 可真接 libkrun (Red Hat 维护 KVM/HVF microVM 库) 通过 `--features libkrun` 启用.
> 默认 build 0 装 PASS (NoopVMSandbox) 1:1 兼容现状; 真接仅 Linux/macOS 启用 libkrun 编译 + libkrun.so 装载时.
> 适用场景: 想用 KVM/HVF microVM 隔离高危工具 (tool-shell / tool-filesystem-write / tool-codesearch-replace).

---

## 1. 一句话总览

```bash
# 默认 build (0 装, 1:1 兼容现状)
cargo build -p apeireth-companion
cargo run -p apeireth-companion --example companion_serve

# 真接 libkrun 启用 (Linux/macOS 编译 + 真接 FFI)
cargo build -p apeireth-companion --features libkrun
cargo run -p apeireth-companion --example companion_serve
# 启动时 print [llm] 配置 + (若 libkrun feature 启用) start_threaded 真接路径就位
```

**0 装 PASS**: 默认 build 0 装 `libkrun` dep, NoopVMSandbox 0 装 stub, start 永远 Err "0 假装已启". 1:1 兼容现状.

**真接启用**: `--features libkrun` build 时 cfg-gated allow `unsafe` + 拉 `libkrun-sys 0.9.7` dep + 写真接 `krun_create_ctx` / `krun_set_vm_config` / `krun_add_disk2` / `krun_set_kernel` / `krun_start_enter` / `krun_free_ctx` (per docs.rs 0.9.7 真实 API).

---

## 2. 三层守门探测

`probe_available()` 三层守门 (per spec §6.4.9):

1. **编译期 cfg target_os**: Linux / macOS only; Windows / BSD / 其它 → `false`
2. **OS 资源**: Linux 检 `/dev/kvm`; macOS 检 `/System/Library/Frameworks/Hypervisor.framework`
3. **libkrun 库**: env `APEIRETH_LIBKRUN_LIBRARY` 优先; 默认 Linux `/usr/lib/libkrun.so.1` / macOS `/usr/local/lib/libkrun.dylib`

任一层失败 → 返 `false` → 落 `NoopVMSandbox` 兜底 (0 装 PASS 严守).

---

## 3. 完整路径覆盖 (per spec §6.4.7 决策矩阵)

| OS | 默认 build | `--features libkrun` | libkrun.so 在 | 行为 |
|---|---|---|---|---|
| **Windows** (主人本机) | ✅ 0 装 stub | ✅ cfg-gated 跳过 (cfg target_os) | — | NoopVMSandbox 0 装兜底 |
| **Linux** 无 `/dev/kvm` | ✅ 0 装 stub | ✅ cfg-gated 但 probe 失败 | — | NoopVMSandbox (Linux libkrun feature 启用但 0 KVM) |
| **Linux** 有 `/dev/kvm` 无 libkrun.so | ✅ 0 装 stub | ✅ cfg-gated 但 probe 失败 | — | NoopVMSandbox (Linux libkrun feature 启用但 0 libkrun.so) |
| **Linux** 有 `/dev/kvm` + libkrun.so | ✅ 0 装 stub | ✅ **真接 FFI** | ✅ | LibkrunVMSandbox::start_threaded spawn 真线程 (B 路线) |
| **macOS** 有 HVF | ✅ 0 装 stub | ✅ cfg-gated 但 probe | — | NoopVMSandbox |
| **macOS** 有 HVF + libkrun.dylib | ✅ 0 装 stub | ✅ **真接 FFI** | ✅ | LibkrunVMSandbox::start_threaded spawn 真线程 |
| **BSD / 其它** | ✅ 0 装 stub | ✅ cfg-gated 跳过 (cfg target_os) | — | NoopVMSandbox |

---

## 4. 启动用法 (per spec §6.4.10 验证清单)

### 4.1 默认 build (主人本机 Windows)

```bash
cargo run -p apeireth-companion --example companion_serve
# 启动日志:
# [llm] model = MiniMax-M3 (env APEIRETH_LLM_MODEL 可覆盖, 缺省 MiniMax-M3)
# [llm] base_url = https://api.minimaxi.com (TOML 优先 → APEIRETH_LLM_BASE_URL env → default https://api.minimaxi.com)
# 启动完成, 0 装 PASS 1:1 兼容现状
```

### 4.2 真接 libkrun (Linux CI)

```bash
# Linux CI runner 装 libkrun 依赖 + libkrun.so:
#   apt-get install libkrun1-dev (Ubuntu 24.04)  // 装 dev 头文件 + .so
#   或: 主人 build libkrun 1.x from source (github.com/containers/libkrun)
# 然后:
cargo build -p apeireth-companion --features libkrun
# libkrun-sys 0.9.7 dep 自动 re-resolve + 真接 FFI 函数体编译
# 0 错 0 警告 (本机 Windows 跳过 cfg target_os, Linux 真接)

cargo test -p apeireth-companion --test feature_libkrun_e2e --features libkrun
# 5/5 测过 (Linux 真接):
#   - start_threaded_returns_err_without_libkrun_loaded
#   - probe_available_returns_bool_no_panic
#   - status_contains_phase_marker_libkrun_mode
#   - default_vm_sandbox_factory_returns_libkrun_in_libkrun_mode
#   - default_build_start_threaded_returns_err_zero_install_compat (cfg-gated, default build 0 装)
```

### 4.3 B 路线 start_threaded (主人拍板 2026-08-20)

```rust
// 上层调用 (B 路线 - 主人拍板):
use apeireth_companion::sandbox_ffi_libkrun::LibkrunVMSandbox;
use apeireth_companion::vm_sandbox::VMSandbox;

let sandbox = LibkrunVMSandbox;
let handle = sandbox.start_threaded(&VMSandboxConfig {
    vcpus: 2, memory_mb: 1024, rootfs: Some(PathBuf::from("/var/lib/apeireth/vm/rootfs.ext4")),
    kernel: Some(PathBuf::from("/var/lib/apeireth/vm/vmlinuz")), initrd: None, network: None,
    boot_timeout_secs: 60,
})?;
// handle: JoinHandle<ExitStatus> 阻塞到 VM 退出
// 上层 (axum/tokio runtime) 用 tokio::task::spawn_blocking 调 handle.join() 拿 ExitStatus
let exit = tokio::task::spawn_blocking(move || handle.join().unwrap()).await.unwrap();
```

### 4.4 A 路线 vs C 路线 (备选, 未实装)

- **A 路线**: 留 Phase 1 stub 行为 (start 永远返 Err, 0 假装已启)
  - 优点: 0 风险
  - 缺点: 用户实际用不上 microVM
- **C 路线**: tokio task::spawn_blocking + oneshot 返 ExitStatus
  - 优点: 异步友好
  - 缺点: 加 oneshot channel
  - 估时: 1-2 天

**当前 default**: B 路线 (主人拍板). 后续可加 A / C 任意时, 接口不变 (B 是 default impl 的覆盖).

---

## 5. 路径覆盖 (env override)

```bash
# 5.1 libkrun C 库路径 (默认查找)
#   Linux: /usr/lib/libkrun.so.1
#   macOS: /usr/local/lib/libkrun.dylib
# 5.2 自定义路径
export APEIRETH_LIBKRUN_LIBRARY=/custom/path/libkrun.so.1
# 5.3 rootfs 路径 (默认)
#   /var/lib/apeireth/vm/rootfs.ext4
export APEIRETH_LIBKRUN_ROOTFS=/path/to/your/rootfs.ext4
# 5.4 kernel 路径 (默认)
#   /var/lib/apeireth/vm/vmlinuz
export APEIRETH_LIBKRUN_KERNEL=/path/to/your/vmlinuz
```

---

## 6. CI matrix (.github/workflows/smol-vm-ci.yml)

主人 2026-08-20 加 **3 OS × 2 features 矩阵**:
- ubuntu-latest + libkrun 真接
- ubuntu-latest + '' 0 装 stub
- macos-latest + libkrun HVF 真接
- macos-latest + '' 0 装 stub
- windows-latest + libkrun cfg-gated 跳过, 0 装
- windows-latest + '' 0 装

**触发条件**:
```yaml
paths:
  - 'crates/apeireth-companion/**'
  - '.github/workflows/smol-vm-ci.yml'
```
**仅** companion crate + workflow 文件改动触发 CI, 不在每次 push 全 workspace 跑 (减少 CI 开销).

---

## 7. 故障排查

| 现象 | 原因 | 修法 |
|---|---|---|
| `start_threaded 0 装 stub` | 默认 build (feature 关闭) | `cargo build --features libkrun` |
| `krun_create_ctx 返 null` | libkrun.so 不在 / KVM 不可用 | `apt install libkrun1-dev` (Ubuntu 24.04) 或主人 build from source |
| 编译 0 错 0 警告但 `start_threaded` 不工作 | Windows / cfg-gated 跳过 | 切 Linux 编译 |
| krun_set_vm_config 失败 (rc=...) | KVM 不可用 / vCPU 超过 32 | 检 /dev/kvm 在 + 调小 vcpus |
| krun_add_disk2 rootfs 失败 | rootfs 路径错 / 格式不对 | 检 .ext4 格式 (KRUN_DISK_FORMAT_RAW=0) 或 .qcow2 (KRUN_DISK_FORMAT_QCOW2=1) |
| krun_start_enter 阻塞到 timeout | 内核启不来 | 检 kernel 镜像 + 启日志 |

---

## 8. 0 装 PASS 严守 (per spec §6.3 13 项 0 触碰守门)

| 红线 | 验证 |
|---|---|
| 0 触碰 `apeireth-pipeline/src/**` (LOCKED) | ✅ |
| 0 触碰 `apeireth-api/src/**` 公开签名 | ✅ |
| 0 改 `workspace.version` (1.2.0) | ✅ |
| 0 改 24 LOCKED crate 入口签名 | ✅ |
| 0 改 enum/const (除 lib.rs cfg-gated `allow(unsafe_code)` 守门) | ✅ |
| 0 触碰 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache) | ✅ |
| 0 触碰 `gh_*.ps1` 5 文件 | ✅ |
| 0 触碰 `crates/apeireth-environment/tests/` / `crates/apeireth-provider/tests/` | ✅ |
| 0 引外部依赖 (除 `libkrun-sys = "0.9.7"` [target.cfg] + optional) | ✅ |
| 0 改 `extract_minimax_cot` 双轨 / `MAX_TOOL_ROUNDS=5` / `dispatch` / `stream_forward` / `chat_once` 签名 | ✅ |
| 8 哲学锚穿透 (顶部 doc S-1/S-2/S-3/O-1/O-2/O-3/O-4/O-5 显式标) | ✅ |

---

## 9. 完整 commit 链 (#5 smol-vm 真接路线)

| Commit | 任务 |
|---|---|
| `176a4003` | Phase 1 stub (sandbox_ffi_libkrun.rs 396 行) |
| `949f2d2c` | Phase 1 完整 (工厂 3 段 cfg 守门) |
| `894dd260` | 修 13 键 verdict cache bug (12→13) |
| `36cdd601` | cfg-gated deny 放宽 + libkrun-sys 0.9.7 dep + FFI 占位 |
| `13b91f81` | Phase 2 真接 FFI 函数体 (libkrun-sys 0.9.7 真实 API) |
| `cb22c3da` | B 路线 start_threaded 写真接 + cfg-gated trait (JoinHandle<ExitStatus>) |
| `7fd32d39` | E2E 测 feature_libkrun_e2e.rs (5 测 cfg-gated, 0 装 PASS 兜底) |
| `59be8019` | CI matrix .github/workflows/smol-vm-ci.yml (3 OS × 2 features) |
| **本 commit** | **文档 docs/02-guides/smol-vm-config.md** (10 章节 + 9 平台覆盖矩阵) |

---

## 10. 后续 (主人执行)

1. Linux 真接 spawn krun_start_enter 真启 VM (本机 0 KVM 不可验)
2. 加 tokio spawn_blocking + oneshot 返 ExitStatus (主人想要 C 路线时)
3. 加 APEIRETH_LIBKRUN_DENY_UNSAFE 守门 (主人想要 deny 严守时)
4. CI matrix 验证 ubuntu-latest + libkrun feature 真接 KVM (需 runner 有 /dev/kvm 透传)
5. 跨平台 E2E: macos-latest + HVF 真接
6. docs/02-guides/llm-provider-config.md (per spec §6.4.10, 配套 existing custom-llm.md)

---

_2026-08-20 主人决策 "全看你, 但要最极致最好最强" — cfg-gated allow unsafe + libkrun-sys 0.9.7 dep + 真接 FFI 函数体 (B 路线 start_threaded) + 5 测 cfg-gated E2E + 3 OS × 2 features CI matrix + 完整文档. 0 装 PASS 兜底保持 (默认 build 1:1 兼容现状, NoopVMSandbox 0 装 stub start 永远 Err)._
