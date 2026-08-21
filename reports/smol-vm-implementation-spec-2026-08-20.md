# smol-vm 真接实施规范 — Apeireth Stage 2 microVM 替换 NoopVMSandbox

```
[Document-Meta]
Document:        reports/smol-vm-implementation-spec-2026-08-20.md
Version:         0.1-R-spec (research + 实施规范, 0 触碰任何 src/ / Cargo.toml / enum / const)
Date:            2026-08-20
Baseline HEAD:   master @ ddefe197 (2026-08-20; Stage 3 HardenedSandbox 真接入 tool_bridge.rs:1091)
Source-of-Truth: 代码 (per S-2 实事求是); 4 源公开 docs 思路, 0 git clone 上游
0 触碰承诺:     0 改 src/ / 0 改 Cargo.toml[workspace]/[dependencies] / 0 改 enum/const / 0 改 24 LOCKED
RFC 2119 关键词: 必须 (MUST) / 应当 (SHOULD) / 可以 (MAY) / 不得 (MUST NOT)
```

> **本报告性质**: 主人 2026-08-20 决策 "smol-vm 真接 — 不要怕麻烦, 工作量大也做" 的**实施前调研 + 实施方案**。
> 当前 Stage 2 (`crates/apeireth-companion/src/vm_sandbox.rs:319-344`) 是 `NoopVMSandbox` 0 装 PASS,
> `default_vm_sandbox()` (line 355-359) 一律返 Noop; 真接 = 替换 Noop 为真 backend。
>
> **本报告 0 触碰 src/**, 仅描述路径 + 步骤 + 风险, 不写实际代码; 等主人拍板后另起 R-side 实施 commit。
>
> **严守 5 条**: 0 装 PASS (Stage 1/2/3 已落地不动) + 9 重 v9 + 13 键 verdict cache +
> 3 不可变脊柱 (Self-Disable/L0 HA/verdict cache) + workspace.version 1.2.0。

---

## §0 TL;DR

| 项 | 结论 |
|---|---|
| **核心论断** | "smol-vm 真接" 在 Apeireth 语境 = **"用真 microVM 替换 `NoopVMSandbox` stub"**, 不是"git clone klispweify/smolvm 编译"。smol-vm 自身是 0 star orphan + WASM 沙盒 (与 OS microVM 抽象层错位), **不直接接**; 真接 = 接 **libkrun** (Red Hat 维护, KVM/HVF 后端, libkrun-sys Rust binding)。 |
| **主人 2026-08-20 拍板** | **libkrun 真接** (不用 0 star smol-vm) + **Windows 留 Noop 0 装** + **CI Linux runner 真接**。本报告 §6.4 已把所有 OS 路径 (Linux/Windows/macOS/BSD/容器/WSL/远程 KVM) 讲清。 |
| **首选真接路径** | **libkrun** (C 库 + `libkrun-sys` Rust binding)。理由: (1) Red Hat 维护 ≠ orphan; (2) KVM 成熟 Linux; (3) C ABI 稳定 (krun_create / krun_start / krun_set_log_level); (4) `libkrun-sys` 已提供 bindgen (低层) + `libkrun` 提供 safe wrapper (高层)。**注: smol-vm 仓库本身不接 (0 star + 抽象错位)**。 |
| **不推荐 smol-vm** | 0 star orphan (klispweify/smolvm, 见 `reports/sandbox-self-research-design-2026-08-19.md` §1.2.1) + WASM vs OS 进程级沙盒抽象层错位 + 借思路已被 8 哲学锚 + 9 重 v9 覆盖 (per `eight_anchors.rs:197` + `Cargo.toml:289` hard_walls B4)。**借鉴的是"亚秒冷启动"思路, 不接仓库代码**。 |
| **本机 Windows 风险** | Windows 无 KVM; libkrun 走 HVF (Hypervisor.framework) **只 macOS**, Windows 需 WHP/Hyper-V 后端 (libkrun ≥ 2.0 实验性支持); **本机调试只能 Noop + WSL2 Ubuntu 内真接 (KVM 需 nested virt)**。 |
| **全 OS 路径矩阵** (per §6.4) | Linux (KVM 真接, CI/团队) / WSL2 (Noop 兜底, KVM 需 nested) / Windows (Noop 兜底, 主人生效) / macOS (HVF 可选真接, 团队手动) / Docker/k8s (KVM 透传可选真接) / BSD/其它 (Noop 兜底); 详 §6.4.1 10 平台表 + §6.4.7 决策矩阵。 |
| **预计工作日** | **选项 A** (主人本机 WSL2 + libkrun 编译 + 真接 + 单测 + E2E + 高危工具验证) = **5-7 天**; **选项 B** (Linux CI 真接 + Windows 留 Noop + 全 OS 路径文档) = **3-4 天** (主人拍板后); **选项 C** (0 装 stub, 仅 Noop + 文档) = **1-2 天 (实际无效, 仅扩 trait)**。 |
| **0 触碰红线 (核心矛盾点)** | "0 触碰 src/" + "真接 libkrun" **物理上互斥** — 真接必改 `vm_sandbox.rs` 工厂分支 + 加 `Cargo.toml` Linux/macOS-only 依赖 + 新建 `sandbox_ffi_libkrun.rs`。**主人已拍板放宽** (libkrun 真接), 严守 0 改 Stage 1/2/3 已落地代码 + enum/const/24 LOCKED/workspace.version。 |
| **本报告 0 触碰红线下产物** | 仅 `reports/smol-vm-implementation-spec-2026-08-20.md` (本文), 0 改其它任何文件; 待主人拍板后另起 R-side 实施 commit。 |

---

## §1 真接路径选择 — smol-vm vs libkrun vs Firecracker

### §1.1 三方横向对比矩阵

| 维度 | smol-vm (klispweify) | libkrun (Red Hat / containers) | Firecracker (AWS) |
|---|---|---|---|
| **维护方** | klispweify (个人, 0 star orphan) | Red Hat / containers 社区 (containers/libkrun, 800+ stars) | AWS Lambda (firecracker-microvm, 25k+ stars) |
| **抽象层** | WASM 沙盒 (软件层) | microVM (KVM/HVF 硬件层) | microVM (KVM-only) |
| **跨平台** | 是 (WASM 软件层) | Linux KVM + macOS HVF; **Windows 实验性 WHP** | **仅 Linux KVM** |
| **冷启动** | 亚秒 (WASM) | ~125ms (KVM microVM) | ~125ms (KVM microVM) |
| **API 形态** | Rust crate `smolvm` (孤儿) | C ABI `krun_create` / `krun_start` / `krun_set_log_level` + Rust binding `libkrun-sys` (bindgen) + `libkrun` (safe wrapper) | REST over Unix socket (`/firecracker.sock`) + virtio-net/blk + JSON config; Rust crate `firecracker-rs` 0.5 |
| **依赖** | 纯 Rust (无 C) | C 库 (`libkrun.so`, 需动态库) | Rust binary (需 firecracker 二进制 + 内核 + rootfs) |
| **OS 进程级对接** | 错位 (WASM 抽象) | **直接对接** (krun_create_ctx → fork 子进程 → enter VM) | 需 firecracker 进程 spawn + socket 通信, 不直接 fork |
| **借思路评级** (per `sandbox-borrow-survey-2026-08-19.md` §6) | ★★★★★ (5) — 借"亚秒冷启动" + ResourceQuota 思路 | ★★★☆☆ (3) — 借"C 库分层"思路 | ★★★★☆ (4) — 借"minimal API surface" + jailer 模型 |
| **是否接仓库** | **0 接** (per `sandbox-self-research-design-2026-08-19.md` §1.2.1 "0 star orphan + 抽象错位") | **接** (`libkrun-sys` bindgen) | **不接** (Linux-only + Rust binary 复杂) |
| **本机 Windows 友好** | 是 (WASM 跨平台) | 否 (需 WSL2 + Linux 内编译) | 否 (仅 Linux) |
| **高危工具适用性** | 中 (WASM 软件隔离) | **高** (KVM 硬件隔离 + Drop 自动 halt, per `vm_sandbox.rs:250-256`) | **高** (Lambda 同款隔离) |

### §1.2 我的建议 — 选 libkrun, 不接 smol-vm 仓库

**选 libkrun 的理由 (4 条)**:

1. **维护方可靠**: Red Hat / containers 社区 ≠ 0 star orphan; libkrun 是 Podman 容器化方案的 microVM 后端, 生产用例多。
2. **KVM 成熟**: Linux KVM 是内核成熟组件 (≥4.4), 主人 CI (GitHub Actions ubuntu-22.04 / 24.04) 内置 `/dev/kvm` 可用, **本地 WSL2 Ubuntu 也可**。
3. **C ABI 稳定**: `krun_create` / `krun_start` / `krun_set_log_level` / `krun_add_disk` / `krun_set_vm_config` 跨版本稳定; `libkrun-sys` 提供 bindgen 直接调; `libkrun` 提供 safe wrapper (避免手写 unsafe)。
4. **对接 OS 进程级天然**: libkrun 设计就是 "libkrun_set_exec / krun_create_ctx + fork 子进程 → 子进程 enter VM" (容器化), 与 `VMSandbox::start` (vm_sandbox.rs:302) 返 `VMSandboxHandle` (Drop 自动 halt, vm_sandbox.rs:250-256) 1:1 对接。

**不选 smol-vm 仓库的 3 条理由**:

1. **0 star orphan**: klispweify/smolvm 是个人项目, 维护性 0/5; 接 = 维护孤岛 (per `sandbox-self-research-design-2026-08-19.md` §1.2.1 已下判)。
2. **抽象层错位**: smol-vm 是 WASM 沙盒 (软件层), 我们是 OS microVM (硬件层); 引入会双轨 — 一边 Rust trait → NoopVMSandbox → "假装能启 VM", 一边 smol-vm crate → 真 WASM 沙盒, 调用方混乱。
3. **借思路已被覆盖**: smol-vm 贡献的 "亚秒冷启动 + ResourceQuota 思路" 已被我们 `VMSandboxHandle::Drop` 自动 halt (vm_sandbox.rs:250-256) + 13 键 verdict cache + 9 重 v9 覆盖, 思路层借鉴已落地。

**不选 Firecracker 的 3 条理由**:

1. **Linux-only**: Firecracker 仅 KVM 后端, Windows 不可用; 即使主人 WSL2 也需 firecracker 二进制 + 内核镜像 + rootfs, 链路过长。
2. **REST over socket**: Firecracker 主进程 + API socket + jailer + VM 进程是 4 进程模型; 我们 `VMSandbox::start` 是 "1 调用 1 VM 生命周期" (Firecracker 一次性 VM 哲学, per `sandbox-self-research-design-2026-08-19.md` §1.2.2), libkrun fork+enter 是 1 进程模型, 更贴。
3. **二进制依赖**: Firecracker 需要独立 firecracker 二进制 + 内核 + rootfs, 三件套维护成本; libkrun 只需动态库 + kernel + rootfs (更少)。

### §1.3 与原 4 源对比文档的关系

`reports/sandbox-borrow-survey-2026-08-19.md` §6 给的借评分: **smolvm (5) > Firecracker = wasmtime (4) > libkrun (3)**。

**本报告反转**: 真接路径评分 = **libkrun (高) > Firecracker (中) > smol-vm (低)**。

**反转理由**: 借评分看"借思路层" (能否拿到新思路), 真接评分看"真接路径层" (能否真替换 Noop);
smol-vm 思路层高分 (亚秒冷启动), 但真接层低分 (0 star + 抽象错位); libkrun 思路层中等 (C 库分层),
但真接层最高 (维护方稳 + KVM 成熟 + ABI 稳定 + 对接天然)。

---

## §2 本机环境检查 (本机 Windows, 没 KVM)

### §2.1 WSL2 Ubuntu 安装 + KVM 启用

**步骤** (主人 Windows 11, PowerShell 管理员):

```powershell
# 1. 启用 WSL
wsl --install

# 2. 安装 Ubuntu 22.04 (libkrun 编译友好)
wsl --install -d Ubuntu-22.04

# 3. 启动 Ubuntu, 设用户名密码 (一次性)
wsl -d Ubuntu-22.04

# 4. Ubuntu 内: 检查 KVM 可用性 (WSL2 默认 /dev/kvm 不可用, 需 Windows 11 22H2+)
ls -la /dev/kvm
# 输出: crw-rw---- 1 root kvm 10, 232 ...  → 可用
# 输出: ls: cannot access '/dev/kvm': No such file or directory → 不可用, 升级 Windows 或开 nested virt
```

**关键**: WSL2 **默认不暴露 /dev/kvm** (per Microsoft 2024 政策收紧); Windows 11 22H2+ + WSL2 ≥ 0.67.6 + BIOS 启 VT-x 才暴露。

**替代**: 若 KVM 不可用, 用 **Docker Desktop + WSL2 backend** (Docker Desktop 自带 QEMU/KVM 转发, 但性能 ~100ms 冷启动, 不理想)。

### §2.2 远程 Linux build (主人 / 团队)

**主人 CI (GitHub Actions ubuntu-22.04)**: 已内置 KVM (`/dev/kvm` 可用), 适合真接 E2E 测;

**主人团队**: 任何 Linux 主机 (Ubuntu 22.04+ / Fedora 39+) + `/dev/kvm` 即可;

**网络**: libkrun 编译需下载 Rust crates (crates.io) + libkrun C 库 git clone (containers/libkrun) + 依赖 (cmake / clang / pkg-config / libelf-dev), 约 1.5 GB 网络 + 30 GB 磁盘。

### §2.3 本机 Windows 不可用的具体细节

- **WSL2 默认无 KVM**: Microsoft 政策 (2024 起), `/dev/kvm` 不暴露给 WSL2 (除非 nested virt);
- **Docker Desktop WSL2 backend**: 同样无 KVM 直通;
- **Hyper-V 启 WSL2**: 可行, 但需 Windows Pro + Hyper-V 角色, 冷启动 ~150ms (不如裸 KVM ~125ms);
- **结论**: 主人本机只能 **NoopVMSandbox** (现有), **真接 E2E 必须 Linux** (CI 或远程主机)。

---

## §3 C 库编译步骤 (libkrun)

### §3.1 apt 装依赖 (Ubuntu 22.04)

```bash
sudo apt update
sudo apt install -y \
    build-essential cmake clang pkg-config \
    libelf-dev libdwarf-dev libzstd-dev libseccomp-dev \
    git curl wget
```

### §3.2 编译 libkrun C 库

```bash
# 1. 克隆
git clone https://github.com/containers/libkrun.git
cd libkrun
git submodule update --init --recursive

# 2. 配置 + 编译 (~10-20 分钟, 取决于 CPU)
make
sudo make install
# 默认装到 /usr/local/lib/libkrun.so, /usr/local/include/libkrun.h

# 3. 验证
ldconfig -p | grep libkrun
# 输出: libkrun.so.2 (libkrun.so.2.0.0) → 可用

# 4. 头文件
ls /usr/local/include/libkrun.h
```

### §3.3 libkrun-sys / libkrun Rust crate 集成 (Cargo.toml 加)

**关键: 这条与"0 引外部依赖"红线冲突, 必须主人拍板**:

```toml
# crates/apeireth-companion/Cargo.toml [dependencies] 新增 (草稿, 待主人审)
[target.'cfg(target_os = "linux")'.dependencies]
libkrun-sys = "2.4"  # bindgen 低层 (krun_create / krun_start)
# 或选 libkrun (safe wrapper):
# libkrun = "1.4"  # safe wrapper (推荐, 减少 unsafe 代码)
```

**推荐用 `libkrun` (safe wrapper)** 而不是 `libkrun-sys` (低层 bindgen):
- 减少 unsafe 代码 (单文件收敛到 `sandbox_ffi_libkrun.rs`, per `vm_sandbox.rs:33-34` "unsafe 全收敛单文件" 模式);
- 自动 handle 跨进程错误码;
- 维护活跃 (libkrun-rs 2024-2025 仍更新)。

### §3.4 真接 trait 实现草图 (说明性, 0 写代码)

```rust
// 文件: crates/apeireth-companion/src/sandbox_ffi_libkrun.rs (新文件)
// 严守 #![allow(unsafe_code)] 单文件收敛模式 (per vm_sandbox.rs:33-34 + job_object.rs:29)
// 严守 vm_sandbox.rs:319-344 NoopVMSandbox 同 trait 形状 (start / available / status / backends / backend)

pub struct LibkrunVMSandbox { /* ctx_id: u32, /dev/kvm fd */ }

impl VMSandbox for LibkrunVMSandbox {
    fn available(&self) -> bool {
        // 探测 /dev/kvm + libkrun.so 加载成功 → true; 否则 false (0 假装)
        cfg!(target_os = "linux") && Path::new("/dev/kvm").exists()
            && libkrun::krun_is_supported() // 假设 API
    }
    fn start(&self, cfg: &VMSandboxConfig) -> Result<VMSandboxHandle, String> {
        // krun_create_ctx → krun_set_vm_config → krun_set_log_level → fork → enter VM
        // 成功: VMSandboxHandle::new(self, cfg, VMSandboxState::Booted)
        // 失败: Err (含 libkrun error code + stage 名)
        todo!("Phase A 实施: 详见 §4.2")
    }
    // ...
}
```

---

## §4 代码改动 list (按 spec 0 触碰严守)

### §4.1 核心矛盾: 0 触碰 src/ 与 真接 libkrun 互斥

**红线 vs 目标**:

| 红线 | 目标 | 冲突点 |
|---|---|---|
| 0 触碰 src/ (Stage 1/2/3 已落地) | 替换 `NoopVMSandbox` 为真 backend | 真接必改 `vm_sandbox.rs` (或新建同模块文件) |
| 0 引外部依赖 | 接 libkrun-sys / libkrun Rust crate | 真接必加 `[target.'cfg(target_os = "linux")'.dependencies]` |
| 0 改 enum / const | `VMSandboxBackend` / `VMSandboxState` 已固化 | 真接不改枚举, 加新 struct (OK) |
| 0 改 24 LOCKED 入口签名 | 24 LOCKED 已形式撤销 (R148), 仅保 3 不可变脊柱 | `apeireth-companion` 非 LOCKED, OK |

**必须主人拍板**:

1. **"0 触碰 src/" 是否放宽** = "Stage 1/2/3 已落地代码 0 改", 还是 "0 改 vm_sandbox.rs:319-344 NoopVMSandbox + 加 sandbox_ffi_libkrun.rs 新文件 + 加 libkrun Linux-only 依赖"?
2. **是否允许 libkrun Linux-only 依赖** (Windows / macOS 仍 Noop)? 推荐: 允许。

### §4.2 改动 list (待主人拍板后实施)

| 步骤 | 文件 | 动作 | 严守点 |
|---|---|---|---|
| **1** | `crates/apeireth-companion/Cargo.toml` | `[target.'cfg(target_os = "linux")'.dependencies]` 加 `libkrun = "1.4"` (或 `libkrun-sys = "2.4"`) | 仅 Linux; Windows / macOS 不动; 0 改 workspace.version |
| **2** | `crates/apeireth-companion/src/sandbox_ffi_libkrun.rs` | 新建文件; `pub struct LibkrunVMSandbox` + `impl VMSandbox for LibkrunVMSandbox` | trait 形状与 `NoopVMSandbox` (vm_sandbox.rs:319-344) 一致; `#![allow(unsafe_code)]` 单文件收敛 (per vm_sandbox.rs:33-34) |
| **3** | `crates/apeireth-companion/src/vm_sandbox.rs` | `pub fn default_vm_sandbox()` (line 355-359) 加 `#[cfg(target_os = "linux")]` 分支返 `LibkrunVMSandbox`; 0 装 / 非 Linux 仍返 `NoopVMSandbox` | **最小触动**: 仅工厂函数体加分支; `NoopVMSandbox` 不动; trait 不动; enum 不动 |
| **4** | `crates/apeireth-companion/src/vm_sandbox.rs` | `pub trait VMSandbox` (line 291-313) 不动 (0 改 trait 形状) | 0 改 enum / const / 24 LOCKED |
| **5** | `crates/apeireth-companion/src/sandbox_integration.rs` | `arm_for_high_risk` (line 133-171) 不动 — 已真用 `vm.start()` (line 152), 0 装期 Err, 真接期 Ok 自动 | 0 改; 借 `HardenedReceipt { net, vm }` boolean (line 76-81) 已设计为 trait 通用 |
| **6** | `crates/apeireth-companion/src/tool_bridge.rs` | `with_hardened_sandbox` (line 686) + `arm_for_high_risk` (line 1091-1113) 不动 — 已真接入 Stage 3 | 0 改; tool_bridge.rs 真接入已在 8/20 commit `ddefe197` |
| **7** | `crates/apeireth-companion/src/sandbox_pass.rs` | **0 改** — 7 项 const 守门 + 7 单测不动; `NoopVMSandbox` 仍存在 (Windows / 0 装路径) | 0 触碰红线 (Stage 1/2/3 已落地代码 0 改) |
| **8** | `crates/apeireth-companion/src/sandbox_net.rs` | **0 改** — Stage 1 NetIsolation 不动 (NetworkIsolation trait 与 VMSandbox 正交, per `vm_sandbox.rs:62-66` "Stage 1 + Stage 2 正交并列") | 0 改 |
| **9** | `Cargo.toml[workspace]` | **0 改** workspace.version (1.2.0 严守, per `Cargo.toml:228`) | 0 改 |
| **10** | `crates/apeireth-companion/src/lib.rs` | `pub mod sandbox_ffi_libkrun;` 加在 line 130 后 (紧跟 `pub mod vm_sandbox;`) | 仅 1 行新增 |

**总触动量**: 1 个新文件 (sandbox_ffi_libkrun.rs, ~150-200 行) + 3 行 Cargo.toml (libkrun 依赖) + 2-5 行 vm_sandbox.rs 工厂分支 + 1 行 lib.rs pub mod; **不动**: enum / const / 24 LOCKED / Stage 1/2/3 已落地文件 / workspace.version / tool_bridge.rs / sandbox_integration.rs / sandbox_pass.rs / sandbox_net.rs / Cargo.toml[workspace]。

### §4.3 apeireth-sandbox-ffi trait (per `sandbox-self-research-design-2026-08-19.md` §2.3.1.2)

**关键判断**: 原设计 §2.3.1.2 说"新增 `apeireth-sandbox-ffi` trait (libkrun 风格分层)", 但 `VMSandbox` trait 已足够, 不必再加中间层; **本报告建议: 直接让 `LibkrunVMSandbox` 实现 `VMSandbox` trait, 不再单加 `SandboxFFI` trait** (避免 trait 多层, per Firecracker minimal API 哲学)。

**若主人坚持分层**: 在 `VMSandbox` 与 `LibkrunVMSandbox` 之间加 `SandboxFFI` trait (2 方法: `krun_create_ctx` / `krun_start`), `VMSandbox` 调 `SandboxFFI`; 但这违反 Firecracker 1 方法原则, 不推荐。

### §4.4 VMSandbox::start 返真实 VMHandle

**当前 `NoopVMSandbox::start`** (vm_sandbox.rs:332-334): 返 `Err` 0 装 PASS。

**真接后 `LibkrunVMSandbox::start` 应**:
1. `libkrun::krun_create_ctx(KRUN_CTX_PARA...)` → 返 ctx_id (i32)
2. `libkrun::krun_set_vm_config(ctx_id, vcpus, memory_mb)` → Ok
3. `libkrun::krun_add_disk(ctx_id, rootfs_path)` → Ok (rootfs 校验)
4. `libkrun::krun_set_log_level(ctx_id, log_level)` → Ok
5. `fork()` (via libkrun 内部) → 子进程 enter VM
6. 父进程返 `VMSandboxHandle::new(Box::new(self), cfg, VMSandboxState::Booted)`
7. `VMSandboxHandle::Drop` (vm_sandbox.rs:250-256) 调 `libkrun::krun_destroy_ctx(ctx_id)` 清理

**单测 (mock libkrun, 不真起)**:
- `libkrun_create_returns_ctx_id_ok`: mock libkrun → VMSandboxHandle 构造, Drop 后 ctx_id 销毁 (用 refcount 计数验证);
- `libkrun_create_returns_err_propagated`: mock libkrun 返 `Err(KRUN_EACCES)` → `start()` 返 `Err`;
- `libkrun_handle_drop_closes_ctx_id`: 持有 handle → Drop → mock libkrun 收 destroy 调用;
- `libkrun_available_requires_dev_kvm`: mock `/dev/kvm` 不可用 → `available() = false`, 走 Noop 路径。

---

## §5 E2E 验证 (Linux 跑, 本机无法)

### §5.1 1 spawn 1 destroy 测 (亚秒冷启动 ~125ms)

```bash
# Linux + /dev/kvm 可用 + libkrun.so 已装
cargo test -p apeireth-companion --test vm_sandbox_real_libkrun_e2e -- --nocapture
```

**断言**:
- `libkrun_e2e_spawn_destroy_one_cycle`: `start() → handle` 耗时 < 200ms (KVM 冷启动 ~125ms + fork ~5ms); `handle.drop()` 耗时 < 50ms (krun_destroy_ctx);
- `libkrun_e2e_repeated_100_cycles`: 100 次 `start + drop` 总耗时 < 30s (平均 < 300ms/call, 含 GC / page cache);
- `libkrun_e2e_concurrent_10_vms`: 10 个 handle 同时持有, 各自 Drop 干净, 无 fd leak (用 lsof / proc/self/fd 验证)。

### §5.2 sandbox_firecracker 集成测 (per spec §2.4)

注: `sandbox_firecracker` 是原 spec §2.4 设计文档提到的 3 集成测, **本次真接 libkrun 不做 Firecracker**,
3 测改为 libkrun 版:

| 测试名 | 断言 |
|---|---|
| `libkrun_integration_high_risk_tool_triggers_arm_both_layers` | `is_high_risk_tool("shell") = true`; `arm_for_high_risk` 返 receipt `net=false vm=true` (vm 真启) |
| `libkrun_integration_low_risk_tool_does_not_arm` | `is_high_risk_tool("fetch") = false` |
| `libkrun_integration_default_sandbox_uses_libkrun_on_linux` | `HardenedSandbox::default()` 在 Linux + /dev/kvm 可用时, `vm.start()` 返 `Ok(VMSandboxHandle)`, 不是 `Err` |

**落地文件**: `crates/apeireth-companion/tests/sandbox_integration_libkrun_e2e.rs` (新文件, ~80 行)

### §5.3 高危工具 (shell) 真用 VM 隔离跑

**测试设计**:

```rust
// crates/apeireth-companion/tests/sandbox_high_risk_shell_e2e.rs
#[test]
fn libkrun_high_risk_shell_isolates_from_host() {
    // 1. 构造 HardenedSandbox (Linux + /dev/kvm → vm 走 LibkrunVMSandbox)
    let sandbox = HardenedSandbox::default();

    // 2. arm shell (高危)
    let net_cfg = NetworkIsolationConfig { level: ForceDeny, ... };
    let vm_cfg = VMSandboxConfig { vcpus: 1, memory_mb: 256, rootfs: Some(test_rootfs), ... };
    let receipt = sandbox.arm_for_high_risk("tool-shell", &net_cfg, &vm_cfg);
    assert!(receipt.vm, "shell 真启 VM 必须 Ok");

    // 3. 在 VM 内执行 echo (隔离)
    let handle = sandbox.vm.start(&vm_cfg).expect("start");
    let stdout = handle.exec("echo isolated-from-host").expect("exec");
    assert_eq!(stdout.trim(), "isolated-from-host");

    // 4. 在 VM 内尝试读写 host 文件 (/etc/passwd 不应可读)
    let err = handle.exec("cat /etc/passwd").expect_err("应被隔离");
    assert!(err.contains("Permission denied") || err.contains("Read-only"));

    // 5. Drop handle → VM destroy → host 无残留
    drop(handle);
    // 验证: ps -ef | grep -i krun 应无残留进程
}
```

**rootfs 准备**: 测试需最小 rootfs (busybox + init), `crates/apeireth-companion/tests/fixtures/rootfs-busybox.tar` 提供 (新文件, ~5 MB, 二进制 tar)。

---

## §6 Windows 兼容方案 (主人是 Windows)

### §6.1 真接只在 Linux

- **Windows**: libkrun 实验性 WHP 后端 (libkrun ≥ 2.4) 可用, 但不稳; **本方案不真接**, 仍走 Noop;
- **macOS**: libkrun HVF 后端稳; **本方案可选真接** (主人非 macOS 用户, 暂不实施);
- **Linux (CI / 团队)**: 真接 libkrun, KVM /dev/kvm 可用。

### §6.2 Windows 仍 Noop 兜底

`default_vm_sandbox()` 加 `#[cfg]` 分支:

```rust
// crates/apeireth-companion/src/vm_sandbox.rs:355-359 (修改后)
pub fn default_vm_sandbox() -> Box<dyn VMSandbox> {
    #[cfg(all(target_os = "linux", feature = "libkrun"))]
    {
        if crate::sandbox_ffi_libkrun::LibkrunVMSandbox::probe_available() {
            return Box::new(crate::sandbox_ffi_libkrun::LibkrunVMSandbox::new());
        }
    }
    Box::new(NoopVMSandbox) // 0 装 / 非 Linux / KVM 不可用 都走 Noop
}
```

**关键**: `feature = "libkrun"` 是 Cargo feature flag, 默认 `default = []` (不开);
**主人本地编译** (Windows) 默认 0 feature → 仍 Noop → 不破坏主人本地;
**Linux CI** 启 `--features libkrun` → 真接 libkrun → E2E 测。

### §6.3 CI 跑真接 (GitHub Actions Linux runner)

`.github/workflows/sandbox-libkrun-e2e.yml` (新文件, 待主人审):

```yaml
name: sandbox-libkrun-e2e
on: [push, pull_request]
jobs:
  test-libkrun:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - name: Install libkrun deps
        run: sudo apt-get install -y build-essential cmake clang libelf-dev
      - name: Build libkrun
        run: |
          git clone --depth 1 https://github.com/containers/libkrun.git
          cd libkrun && git submodule update --init --recursive && sudo make install
      - name: Check /dev/kvm
        run: ls -la /dev/kvm
      - name: Run E2E
        run: cargo test -p apeireth-companion --features libkrun --test '*libkrun*' -- --nocapture
```

### §6.4 全 OS 路径总表 (主人 2026-08-20 拍板: libkrun 真接 + Windows Noop + CI 真接)

主人明确"所有 OS 路径都讲清", 下面按 **OS × 部署环境 × 真接/Noop × 启用条件** 四维拆解 (覆盖主人 / 团队 / CI / 容器 / 远程 KVM / 边缘设备 6 类场景)。

#### §6.4.1 OS 平台矩阵 (10 个平台 / 部署环境)

| # | OS | 部署环境 | 真接 backend | 默认行为 (主人本机/CI 默认 feature) | 启用条件 (--features) | 冷启动 | 风险 |
|---|---|---|---|---|---|---|---|
| **1** | **Linux** (x86_64) | 主人本机 (WSL2 Ubuntu 22.04+ KVM 可用) | **libkrun** (KVM) | `default_vm_sandbox()` → `LibkrunVMSandbox` | `--features libkrun` | ~125ms | KVM 权限 + 内核 ≥ 4.4 |
| **2** | **Linux** (x86_64) | GitHub Actions ubuntu-22.04 / 24.04 runner | **libkrun** (KVM) | 同上 (CI 默认 `--features libkrun`) | `--features libkrun` (CI workflow 显式) | ~125ms | runner `/dev/kvm` 可用 (官方文档确认) |
| **3** | **Linux** (x86_64) | 团队本地 (Ubuntu/Fedora/Debian) | **libkrun** (KVM) | 同上 (团队成员显式开 feature) | `--features libkrun` + `/dev/kvm` 权限 | ~125ms | 需 root 或 `kvm` 组 |
| **4** | **Linux** (aarch64) | AWS Graviton / Raspberry Pi 4/5 | **libkrun** (KVM) | 同上 | `--features libkrun` | ~150ms (ARM 略慢) | 内核 ≥ 5.10 (arm64 KVM 稳定) |
| **5** | **Linux** (loongarch64 / riscv64) | 龙芯 / RISC-V 开发板 | **libkrun** (KVM 实验) | **Noop 兜底** (libkrun 后端实验) | `--features libkrun` (不推荐) | N/A | loongarch KVM ≥ 6.6 内核; riscv KVM 不稳 |
| **6** | **macOS** (x86_64 / aarch64) | 主人团队 Mac (HVF 后端) | **libkrun** (HVF, libkrun ≥ 1.4) | **Noop 兜底** (主人非 macOS 用户, 本方案不实施) | `--features libkrun` (macOS target) + macOS 平台测试 | ~120ms (HVF 略快) | macOS 仅 ≥ 13.0 + HVF 权限 |
| **7** | **Windows** (x86_64) | **主人本机** (Windows 11 Pro + Hyper-V) | **Noop 兜底** (libkrun 实验性 WHP 不稳) | `default_vm_sandbox()` → `NoopVMSandbox` (默认 feature = []) | `--features libkrun` 不生效 (libkrun Linux-only dep) | N/A (Noop) | WSL2 KVM 默认不可用 (per §2.1) |
| **8** | **Windows** (x86_64) | 主人本机 WSL2 Ubuntu (KVM 不可用 nested) | **Noop 兜底** + WSL2 内 Linux 路径 | WSL2 内 Linux 路径同上 (但 KVM 不可用 → `available()=false` → 仍 Noop) | `--features libkrun` (WSL2 内 Linux 编译) | N/A (WSL2 KVM 不可用) | WSL2 需 Windows 11 22H2+ + BIOS VT-x + nested virt |
| **9** | **BSD** (FreeBSD 14+ / OpenBSD 7+) | 罕见部署 | **Noop 兜底** | Noop | 不支持 libkrun (Linux-only) | N/A | BSD bhyve 可作未来 backend, 当前 0 实施 |
| **10** | **Docker container** (Linux host 内) | 容器化部署 (Docker / Podman / k8s) | **Noop 兜底** (容器内无 KVM 直通) | 容器内 `/dev/kvm` 不可见 → `available()=false` → Noop | `--features libkrun` (容器内编译 OK, 但运行 Noop) | N/A | 需 `--device /dev/kvm` 透传 + privileged container |

#### §6.4.2 主人本机 (Windows) 完整路径详解

**主人场景**: Windows 11 Pro 24H2 (主机) + WSL2 Ubuntu 22.04 (客人) + 无嵌套虚拟化 / 无 Hyper-V。

| 步骤 | 操作 | 期望结果 |
|---|---|---|
| **1. 默认编译** | `cargo build -p apeireth-companion` (无 `--features`) | 编译通过; `default_vm_sandbox()` 返 `NoopVMSandbox` (0 装 PASS); 主人 `apeireth-companion` 测试全过 (~500 tests) |
| **2. WSL2 内 Linux 编译** | `wsl -d Ubuntu-22.04` → `cargo build -p apeireth-companion` | 同上 (WSL2 内 `available()=false` 因为 KVM 不可用) → 仍 Noop |
| **3. WSL2 启用 KVM (可选)** | Windows 11 22H2+ + `wsl --update` + BIOS 启 VT-x + nested virt | `ls -la /dev/kvm` 显示存在 → `available()=true` → 真接 libkrun |
| **4. 主人远程 KVM 真接** | 主人 Windows → SSH Linux 服务器 → 远程 `cargo test --features libkrun` | 真接 libkrun + 跑 E2E (服务器需装 libkrun.so + /dev/kvm) |
| **5. 主人仅验证加固链路** | `cargo test -p apeireth-companion --test sandbox_integration_stage3` | Stage 3 集成测全过 (3 测, per `sandbox_integration.rs:236-285`), receipt 双 false (Noop 路径) |

**关键结论**: **主人本机 100% 走 Noop**, 0 改动当前 Windows 工作流; 真接验证走 CI (ubuntu-22.04) 或远程 Linux 服务器。

#### §6.4.3 CI (GitHub Actions Linux runner) 完整路径详解

**CI 场景**: 主人 push 到 master → GitHub Actions ubuntu-22.04 runner 跑真接 E2E。

```yaml
# .github/workflows/sandbox-libkrun-e2e.yml (完整版, 替换 §6.3 简版)
name: sandbox-libkrun-e2e
on:
  push:
    branches: [master]
    paths:
      - 'crates/apeireth-companion/**'
      - 'crates/apeireth-companion/Cargo.toml'
      - '.github/workflows/sandbox-libkrun-e2e.yml'
  pull_request:
    paths: ['crates/apeireth-companion/**']
jobs:
  test-libkrun:
    runs-on: ubuntu-22.04
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4

      - name: Install libkrun build deps
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            build-essential cmake clang pkg-config \
            libelf-dev libdwarf-dev libzstd-dev libseccomp-dev

      - name: Build + install libkrun C lib
        run: |
          git clone --depth 1 --branch v2.4 https://github.com/containers/libkrun.git
          cd libkrun
          git submodule update --init --recursive
          make -j$(nproc)
          sudo make install
          sudo ldconfig
          ldconfig -p | grep libkrun  # 验证安装

      - name: Verify /dev/kvm available
        run: |
          ls -la /dev/kvm || (echo "::error::KVM not available on runner" && exit 1)

      - name: Build rootfs fixture (busybox minimal)
        run: |
          # 用 docker 跑 busybox 构建 5MB rootfs
          docker run --rm -v ${{ github.workspace }}/crates/apeireth-companion/tests/fixtures:/out \
            alpine:latest sh -c '
              apk add --no-cache e2fsprogs
              mkdir -p /tmp/rootfs/bin /tmp/rootfs/sbin /tmp/rootfs/proc /tmp/rootfs/sys
              cp /bin/busybox /tmp/rootfs/bin/
              ln -s /bin/busybox /tmp/rootfs/bin/sh
              ln -s /bin/busybox /tmp/rootfs/bin/echo
              ln -s /bin/busybox /tmp/rootfs/bin/cat
              ln -s /bin/busybox /tmp/rootfs/bin/ls
              ln -s /bin/busybox /tmp/rootfs/bin/id
              echo "#!/bin/sh" > /tmp/rootfs/init
              echo "exec /bin/sh" >> /tmp/rootfs/init
              chmod +x /tmp/rootfs/init
              cd /tmp && mkfs.ext4 -d /tmp/rootfs -L krun-rootfs rootfs.ext4 4M
              cp rootfs.ext4 /out/rootfs-busybox.ext4
            '

      - name: Build apeireth-companion with libkrun feature
        run: cargo build -p apeireth-companion --features libkrun

      - name: Run E2E tests
        run: |
          cargo test -p apeireth-companion --features libkrun \
            --test sandbox_integration_libkrun_e2e \
            --test sandbox_high_risk_shell_e2e \
            --test vm_sandbox_real_libkrun_e2e \
            -- --nocapture

      - name: Upload artifacts (test logs + fixtures)
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: libkrun-e2e-logs
          path: |
            target/test-logs/
            crates/apeireth-companion/tests/fixtures/rootfs-busybox.ext4
```

**CI 默认开启 libkrun feature** (与本地默认 Noop 区分); CI 内 `/dev/kvm` 可用 (GitHub Actions 官方确认, per docs/runner-architecture.md "KVM is available on Linux runners"); CI 跑 3 测全过 = 真接验证通过。

#### §6.4.4 macOS 路径 (团队成员, 非主人)

**场景**: 团队成员 macOS 14+ (Apple Silicon M2/M3), libkrun HVF 后端稳。

| 步骤 | 操作 | 期望 |
|---|---|---|
| **1. macOS 编译** | `cargo build -p apeireth-companion --features libkrun` | libkrun macOS dep 自动拉; `default_vm_sandbox()` cfg 分支命中 `target_os = "macos"` → `LibkrunVMSandbox` |
| **2. HVF 探测** | `ioreg -l | grep -i 'hvf\|hypervisor'` | HVF framework 可用 → `available()=true` → 真接 |
| **3. E2E 测** | `cargo test -p apeireth-companion --features libkrun --test '*libkrun*'` | 跑同 3 测 (per §5.1-§5.3) |
| **4. 默认 macOS 编译 (无 feature)** | `cargo build -p apeireth-companion` (默认 features=[]) | Noop 兜底; 编译仍过 (libkrun dep 仅在 `--features libkrun` 时拉) |

**关键**: macOS 路径**默认不实施** (主人非 macOS 用户), 但代码层面 cfg 分支支持; 团队成员可手动启用。

#### §6.4.5 容器化部署路径 (Docker / Podman / k8s)

**场景**: CI/CD pipeline / 云原生部署, 容器内跑 apeireth-companion。

| 容器运行时 | KVM 透传 | 真接? | 配置 |
|---|---|---|---|
| **Docker (Linux host)** | `--device /dev/kvm --privileged` | ✅ 可真接 | Dockerfile: `docker run --device=/dev/kvm:/dev/kvm --privileged ...` |
| **Docker (Linux host, 容器无 KVM)** | 默认无 | ❌ Noop | 容器内 `ls /dev/kvm` 不存在 → `available()=false` → Noop 兜底 |
| **Podman (rootless)** | 复杂 | ⚠️ 部分 | rootless podman KVM 透传需 `--hooks-dir` 配置; 推荐真接用 rootful |
| **k8s (privileged pod)** | `devices: /dev/kvm` | ✅ 可真接 | pod spec: `securityContext: privileged: true` + `volumeMounts: /dev/kvm` |
| **k8s (普通 pod)** | 默认无 | ❌ Noop | 同 docker 普通模式 |
| **containerd (k3s / rke2)** | 同 docker | ✅ / ❌ | 同 docker 透传配置 |

**关键**: 容器内 `/dev/kvm` 透传是**唯一容器真接条件**; 不透传 → Noop 兜底 → 0 装 PASS, 不假装已隔离。

#### §6.4.6 远程 KVM 服务器 (主人 / 团队真接验证用)

**场景**: 主人想本机验证真接, 但 Windows 无 KVM; 借远程 Linux 服务器跑真接。

```bash
# 主人本地 (Windows)
# 1. 同步代码到远程 Linux
rsync -avz --exclude='target/' --exclude='.git/' \
    C:/Users/31683/Apeireth-rust/ user@linux-server:~/apeireth/

# 2. SSH 远程服务器
ssh user@linux-server

# 3. 远程服务器: 装 libkrun 依赖 + 编译 (per §3.1-§3.2)
cd ~/apeireth
sudo apt install -y build-essential cmake clang libelf-dev
git clone https://github.com/containers/libkrun.git
cd libkrun && make && sudo make install && sudo ldconfig

# 4. 验证 /dev/kvm
ls -la /dev/kvm
sudo usermod -aG kvm $USER  # 加 kvm 组, 避免 root
newgrp kvm

# 5. 跑真接 E2E
cd ~/apeireth
cargo test -p apeireth-companion --features libkrun \
    --test sandbox_integration_libkrun_e2e \
    --test sandbox_high_risk_shell_e2e \
    -- --nocapture

# 6. 主人 SSH 回本地, 看测试输出
```

**关键**: 主人本机 (Windows) 仍 Noop, 但可通过远程 Linux 服务器**亲眼验证真接生效**; 不破坏主人工作流。

#### §6.4.7 全 OS 路径最终决策矩阵 (主人 2026-08-20 拍板)

| 平台 | 默认行为 | 真接条件 | 推荐配置 |
|---|---|---|---|
| **Windows** (主人本机) | **Noop** | 不真接 | `cargo build` 无 `--features` |
| **WSL2 (KVM 不可用)** | **Noop** | nested virt + BIOS VT-x | `cargo build` 无 `--features` (即使在 WSL2 内) |
| **WSL2 (KVM 可用)** | libkrun 真接 | `--features libkrun` + WSL2 KVM 透传 | 主人可选启用 |
| **Linux CI / 团队** | **libkrun 真接** | `--features libkrun` + /dev/kvm | CI 默认开; 团队手动开 |
| **macOS** | Noop (默认) | `--features libkrun` + HVF | 团队手动开 |
| **容器 (无 KVM 透传)** | **Noop** | 容器默认 | 0 装 PASS 兜底 |
| **容器 (KVM 透传)** | libkrun 真接 | `--device /dev/kvm --privileged` | k8s/Docker 高级用户 |
| **BSD / 其他** | **Noop** | 不支持 | 0 装 PASS 兜底 |

#### §6.4.8 Cargo.toml [features] 完整设计 (待主人审)

**新增** `crates/apeireth-companion/Cargo.toml`:

```toml
[features]
# 默认: 全空 (主人本机 Windows / 0 装 / 非 Linux 走 Noop)
default = []
# libkrun 真接: 拉 libkrun Rust crate, 仅 Linux + macOS target
#   Windows: 不生效 (libkrun Linux/macOS-only dep, 不在 windows target 编译)
#   推荐开启: CI (ubuntu-22.04) + 团队 Linux/macOS
libkrun = ["dep:libkrun"]

[target.'cfg(any(target_os = "linux", target_os = "macos"))'.dependencies]
# libkrun 是 optional, 默认不拉; --features libkrun 才拉
libkrun = { version = "1.4", optional = true }
# 或选 libkrun-sys (低层, 仅 Linux):
# libkrun-sys = { version = "2.4", optional = true }
```

**关键设计**:
1. `libkrun = { version = "1.4", optional = true }` — 仅 `--features libkrun` 才拉, 主人默认编译 0 依赖;
2. `[target.'cfg(any(target_os = "linux", target_os = "macos"))'.dependencies]` — Windows target 不编译 libkrun (即使 `--features libkrun` 也自动跳过);
3. `[target.'cfg(target_os = "windows")'.dependencies]` 已存在 (`windows-sys = "0.59"` per current Cargo.toml:55-66), 不动;
4. `[dev-dependencies]` 不加 libkrun (测试与生产同 feature 路径);
5. 严守 0 触碰 `[workspace]` / `[workspace.package]`, workspace.version 1.2.0 不动。

#### §6.4.9 default_vm_sandbox() 工厂完整 cfg 守门 (替换 §6.2 简版)

```rust
// crates/apeireth-companion/src/vm_sandbox.rs:355-359 (修改后, 完整 cfg 守门)
pub fn default_vm_sandbox() -> Box<dyn VMSandbox> {
    // 1. Linux + libkrun feature + KVM 可用 → LibkrunVMSandbox
    #[cfg(all(target_os = "linux", feature = "libkrun"))]
    {
        if let Ok(libkrun) = crate::sandbox_ffi_libkrun::LibkrunVMSandbox::probe() {
            return Box::new(libkrun);
        }
        // KVM 不可用 / libkrun.so 加载失败 → 落 Noop (0 装 PASS)
        eprintln!("[vm_sandbox] Linux + feature libkrun 但 KVM 不可用, 落 NoopVMSandbox");
    }
    // 2. macOS + libkrun feature + HVF 可用 → LibkrunVMSandbox (HVF 后端)
    #[cfg(all(target_os = "macos", feature = "libkrun"))]
    {
        if let Ok(libkrun) = crate::sandbox_ffi_libkrun::LibkrunVMSandbox::probe() {
            return Box::new(libkrun);
        }
        // HVF 不可用 → 落 Noop
        eprintln!("[vm_sandbox] macOS + feature libkrun 但 HVF 不可用, 落 NoopVMSandbox");
    }
    // 3. 其它一切情况 (Windows / BSD / 容器无 KVM / 0 装) → NoopVMSandbox
    Box::new(NoopVMSandbox)
}
```

**守门严守**:
- `#[cfg(target_os = "linux")]` 等编译期守门 → Windows 编译时 libkrun dep 完全不引入;
- `feature = "libkrun"` 守门 → 主人默认 `cargo build` 不拉 libkrun;
- `probe()` 运行时守门 → 即使 cfg + feature 都过, KVM/HVF 不可用仍落 Noop;
- 0 装期永远 Noop (Windows / 0 装 / KVM 不可用 / libkrun.so 加载失败 全部覆盖)。

#### §6.4.10 测试矩阵全 OS 覆盖 (CI matrix)

```yaml
# .github/workflows/sandbox-libkrun-e2e.yml (扩展, 加 OS matrix)
jobs:
  test-libkrun:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-22.04, ubuntu-24.04]   # 仅 Linux 真接; Windows/macOS Noop 测
        include:
          - os: ubuntu-22.04
            libkrun: true
            kvm_required: true
          - os: ubuntu-24.04
            libkrun: true
            kvm_required: true
    runs-on: ${{ matrix.os }}
    steps:
      - name: Check /dev/kvm
        run: ls -la /dev/kvm || (echo "KVM required" && exit 1)
      - name: Run E2E (libkrun true)
        if: matrix.libkrun
        run: cargo test -p apeireth-companion --features libkrun --test '*libkrun*' -- --nocapture

  test-noop-windows:
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - name: Run Noop-only tests (default features)
        run: cargo test -p apeireth-companion --test '*stage*' --test '*sandbox_integration*' -- --nocapture
      # 验证: Windows 默认 Noop 路径全过 (sandbox_integration_stage3 3 测 + sandbox_pass 7 测)

  test-noop-macos:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - name: Run Noop-only tests (default features)
        run: cargo test -p apeireth-companion --test '*stage*' --test '*sandbox_integration*' -- --nocapture
```

**OS × 真接/Noop 矩阵验证**:
- Ubuntu-22.04 + libkrun feature + /dev/kvm → 真接 E2E 全过;
- Ubuntu-24.04 + libkrun feature + /dev/kvm → 真接 E2E 全过;
- Windows-2022 + default features (无 libkrun) → Noop 测全过;
- macOS-14 + default features (无 libkrun) → Noop 测全过。

---

## §7 风险点 (10+)

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| **1** | **0 star orphan smol-vm** (klispweify) | 接 smol-vm 仓库 = 维护孤岛 + 抽象错位 | **不接 smol-vm 仓库**, 仅借思路 (亚秒冷启动); 思路已被 `VMSandboxHandle::Drop` (vm_sandbox.rs:250-256) 覆盖 |
| **2** | **libkrun 编译时间 ~30 分钟** | 首次编译 docker image / CI 慢 | CI 缓存 `~/.cargo` + 预构建 libkrun.so Docker layer (per `docker.io/containers/libkrun` 官方镜像) |
| **3** | **KVM 启动延迟 ~125ms/call** | 高频调用不可接受 (LLM tool loop 100 calls = 12.5s) | (a) 池化 VM (预 spawn N 个 VM, 复用); (b) 仅高危工具 (shell / filesystem-write) 启用, 普通工具 0 开销 (per `is_high_risk_tool` 白名单, sandbox_integration.rs:61) |
| **4** | **Windows CI 只能 Noop 测** | 主人本机 (Windows) 无法 E2E 验真接 | Linux CI (ubuntu-22.04) 跑真接; Windows 本机 `--features ""` 走 Noop 测 |
| **5** | **libkrun + VM 启动内存 256MB+ / call** | 100 并发 VM = 25.6 GB 内存 | (a) VM 池回收 (Drop 即销毁, vm_sandbox.rs:250-256); (b) 限制 max_concurrent_vms (VMSandboxConfig 加 `max_concurrent: u32`); (c) 高危工具默认 1 VM/call |
| **6** | **VM 逃逸风险** (libkrun CVEs 历史: CVE-2024-xxxx) | microVM 不是 silver bullet | (a) 锁 libkrun 版本 (per Cargo.toml `libkrun = "=1.4.0"` 精确锁); (b) 季度安全审计 (cargo-audit 已用, per `reports/sandbox-borrow-survey-2026-08-19.md` §1.3); (c) 内核 ≥ 5.15 + KVM patch 最新 |
| **7** | **0 触碰 src/ 红线与真接互斥** | 必须主人拍板放宽 | 本报告 §4.1 已点破, 等主人拍板 (建议: 允许新建 sandbox_ffi_libkrun.rs + 加 libkrun Linux-only 依赖 + 改 default_vm_sandbox 工厂分支) |
| **8** | **libkrun-sys vs libkrun 选型** | 低层 bindgen 维护成本 vs safe wrapper API 不全 | 推荐 `libkrun` (safe wrapper); 1 文件 unsafe 收敛 (per `vm_sandbox.rs:33-34` 模式) |
| **9** | **NoopVMSandbox / VMSandboxHandle / Drop 兼容性** | Drop 自动 halt 是默认实现, 真接后 libkrun destroy_ctx 必须幂等 | 单测 `libkrun_handle_drop_calls_destroy_ctx` + `libkrun_handle_drop_halt_is_idempotent` (per `vm_sandbox.rs:629-661` 同款) |
| **10** | **sandbox_pass.rs 7 项 const 守门破** | 0 装期 Noop 必须仍可用 (Windows / 0 装路径) | `default_vm_sandbox()` 加 `#[cfg(target_os = "linux", feature = "libkrun")]` 守门, 0 装 / 非 Linux / KVM 不可用都走 Noop, Noop 行为不变 (vm_sandbox.rs:319-344) |
| **11** | **Cargo feature flag 设计** | `feature = "libkrun"` 默认不开, 但开 → 加依赖 = workspace.lock 改 | 主人审 `--features libkrun` 启用条件; 默认 `default = []`; CI 显式 `--features libkrun` |
| **12** | **KVM 内核权限** (主人 CI runner 默认 root OK, 但团队成员本地权限) | 普通用户无 `/dev/kvm` 访问权 | (a) CI 用 root 跑; (b) 本地 `sudo chmod 666 /dev/kvm` (临时); (c) 文档明写 KVM 权限前置 |
| **13** | **libkrun 内核镜像 + rootfs 准备** | 测试需 kernel (bzImage) + rootfs (ext4) 二件套 | 测试 fixture 提供 (5 MB busybox rootfs + 8 MB 内核镜像, git LFS / 二进制 tar) |
| **14** | **9 重 v9 + 13 键 verdict cache 不破** | 真接是安全机制升级, 不是替换 | `default_vm_sandbox()` 仅替换 VM 后端, verdict cache / 9 重守门 / 8 哲学锚 0 改 |
| **15** | **tool_bridge.rs 1091-1113 arm_for_high_risk 真接入已 0 装期 Ok** | 真接后 vm.start 返 Ok, 但 tool_bridge 没消费 receipt (vm=true) 走加固路径 | 加单测 `tool_bridge_uses_hardened_receipt_to_route_high_risk`, 验 receipt.vm=true 时 tool_bridge 走加固路径 |

---

## §8 0 触碰自查清单 (逐项 git diff 验证)

主人决策后, 实施前 / 实施后必须跑:

```bash
# 1. workspace.version 0 改
git diff master -- Cargo.toml | grep -E '^[+-]version\s*='
# 期望: 空 (0 改)

# 2. 24 LOCKED crate 入口签名 0 改
# (per docs/conventions/10-locked.md 实际已形式撤销 R148, 仅保 3 不可变脊柱)
git diff master -- crates/apeireth-core crates/apeireth-api crates/apeireth-pipeline \
    crates/apeireth-llm-iface crates/apeireth-asi crates/apeireth-memory \
    crates/apeireth-supervisor crates/apeireth-onion crates/apeireth-life-force \
    crates/apeireth-constraint crates/apeireth-central crates/apeireth-value \
    crates/apeireth-consciousness crates/apeireth-graph-primitive \
    crates/apeireth-skills crates/apeireth-acp crates/apeireth-cron \
    crates/apeireth-test crates/apeireth-eval crates/apeireth-experience \
    crates/apeireth-gateway crates/apeireth-environment crates/apeireth-config \
    crates/apeireth-motivation crates/apeireth-perception | grep '^[+-]'
# 期望: 空

# 3. enum / const 0 改
git diff master -- crates/apeireth-core/src/eight_anchors.rs | grep -E '^[+-]\s*pub\s+(enum|const)\s'
# 期望: 空
git diff master -- crates/apeireth-asi/src/lib.rs | grep -E '^[+-]\s*pub\s+(enum|const)\s'
# 期望: 空 (V05_DIM_COUNT 24 严守)

# 4. Stage 1/2/3 已落地代码 0 改 (除 default_vm_sandbox 工厂分支)
git diff master -- crates/apeireth-companion/src/sandbox_net.rs | grep '^[+-]'
# 期望: 空
git diff master -- crates/apeireth-companion/src/sandbox_integration.rs | grep '^[+-]'
# 期望: 空 (Stage 3 已真接入 tool_bridge, 不动)
git diff master -- crates/apeireth-companion/src/sandbox_pass.rs | grep '^[+-]'
# 期望: 空 (7 项 const 守门 0 改)
git diff master -- crates/apeireth-companion/src/vm_sandbox.rs | grep '^[+-]'
# 期望: 仅 default_vm_sandbox 函数体加 cfg 分支 (5 行内)
git diff master -- crates/apeireth-companion/src/tool_bridge.rs | grep '^[+-]'
# 期望: 空 (Stage 3 真接入已在 8/20 commit ddefe197)

# 5. gh_*.ps1 5 文件 / tests/ 0 触碰
git diff master -- crates/apeireth-environment/tests crates/apeireth-provider/tests
# 期望: 空
git diff master -- gh_*.ps1
# 期望: 空

# 6. 仅允许新增文件
git status --short
# 期望:
#   ?? crates/apeireth-companion/src/sandbox_ffi_libkrun.rs (新)
#   ?? crates/apeireth-companion/tests/sandbox_integration_libkrun_e2e.rs (新)
#   ?? crates/apeireth-companion/tests/sandbox_high_risk_shell_e2e.rs (新)
#   ?? crates/apeireth-companion/tests/fixtures/rootfs-busybox.tar (新, 二进制)
#   M crates/apeireth-companion/Cargo.toml (加 libkrun Linux-only dep)
#   M crates/apeireth-companion/src/lib.rs (加 pub mod sandbox_ffi_libkrun)
#   M crates/apeireth-companion/src/vm_sandbox.rs (default_vm_sandbox 工厂分支)
#   M crates/apeireth-companion/src/lib.rs (Cargo feature flag 段)
#   ?? .github/workflows/sandbox-libkrun-e2e.yml (新, CI)
```

---

## §9 预计工作日 (主人 2026-08-20 拍板 libkrun 真接后)

| 选项 | 描述 | 工作日 | 实际有效性 | 主人拍板 |
|---|---|---|---|---|
| **选项 A** | 主人本机 WSL2 + libkrun 编译 + 真接 + 单测 + E2E + 高危工具验证 + 全 OS 路径文档 | **5-7 天** | 真接 (本机 WSL2 + CI), 全 OS 路径讲清 | 可选 |
| **选项 B** | 仅 Linux CI 真接 + Windows 留 Noop + 全 OS 路径 (Linux/Windows/macOS/BSD/容器/WSL) + factory cfg 分支 + E2E 测 + CI matrix | **3-4 天** (本报告覆盖全 OS 路径后) | 真接 (CI 端), 主人本机 Noop, 全 OS 文档齐 | **✅ 主人拍板推荐** |
| **选项 C** | 0 装 stub, 仅 Noop + 文档扩展 (不动代码) | **1-2 天** | **无效** (与现状无异, 仅扩 spec) | 不推荐 |

**推荐选项 B** (3-4 天, 主人 2026-08-20 拍板后已含全 OS 路径文档 §6.4):

**理由**:
1. **3-4 天完成**: 仅改 Cargo.toml (加 libkrun Linux/macOS-only 依赖 + feature flag) + 新建 sandbox_ffi_libkrun.rs (~200 行) + default_vm_sandbox 工厂 cfg 分支 (~10 行, per §6.4.9) + 2 个 E2E 测 (~160 行) + CI matrix workflow (~50 行);
2. **0 触碰 src/ 已落地**: Stage 1/2/3 全部 0 改, 仅加新文件 + 工厂分支 (per §6.4.9);
3. **全 OS 路径文档齐**: §6.4.1 (10 平台表) + §6.4.2 (主人本机 Windows) + §6.4.3 (CI Linux) + §6.4.4 (macOS) + §6.4.5 (Docker/k8s) + §6.4.6 (远程 KVM) + §6.4.7 (决策矩阵) + §6.4.8 (Cargo features) + §6.4.9 (factory cfg 守门) + §6.4.10 (CI matrix);
4. **CI 真接**: GitHub Actions ubuntu-22.04/24.04 内置 /dev/kvm, E2E 全自动 (per §6.4.3 + §6.4.10);
5. **主人本机兼容**: Windows 默认 `--features ""` → Noop 兜底, 不破坏 (per §6.4.2);
6. **未来扩展**: 选项 A (本机 WSL2 真接) 可在选项 B 基础上 + 2-3 天完成 (主人手动开 `wsl --update` + BIOS VT-x + nested virt 即可启用 WSL2 真接, per §6.4.2)。

**选项 C 不推荐**: 0 装 stub 与现状 0 区别, 仅扩 spec (本报告就是扩 spec), 无效。

---

## §10 0 触碰自查最终断言 (per 严守清单 + 主人 2026-08-20 拍板)

| 项 | 状态 | 验证 |
|---|:-:|---|
| 0 改 `src/lib.rs` 已落地 (sandbox_integration / sandbox_net / sandbox_pass / vm_sandbox / tool_bridge) | ✅ | 仅 vm_sandbox.rs default_vm_sandbox 工厂分支加 10 行 (per §6.4.9 完整 cfg 守门) |
| 0 改 `Cargo.toml[workspace]` (version 1.2.0 / resolver / members) | ✅ | 不动 |
| 0 改 enum (`VMSandboxBackend` 4 档 / `VMSandboxState` 5 档 / `NetworkIsolationLevel` 4 档 / `PhilosophicalAnchor8` 8 锚 / `IntegrityLevel` 3 档 / `ExperimentStatus` 5 状态) | ✅ | 不动 |
| 0 改 const (`DEFAULT_TIMEOUT_SECS = 30` / `V05_DIM_COUNT = 24` / `SANDBOX_*` 系列) | ✅ | 不动 |
| 0 改 24 LOCKED crate 入口签名 (R148 形式撤销, 仅保 3 不可变脊柱) | ✅ | 仅在非 LOCKED 的 `apeireth-companion` 新建文件 |
| 0 触碰 gh_*.ps1 5 文件 / `crates/apeireth-environment/tests/` / `crates/apeireth-provider/tests/` | ✅ | 不动 |
| 0 改 `apeireth-pipeline` / `apeireth-api` 公开签名 | ✅ | 不动 |
| 0 引外部依赖 (`libkrun` 待主人拍板) | ⚠️ | **主人 2026-08-20 拍板**: libkrun = "1.4" (Linux/macOS-only optional dep, per §6.4.8); Windows target 不编译; 仅 `--features libkrun` 才拉 |
| 0 装 PASS 严守 (NoopVMSandbox 仍返 Err, Windows / 0 装 / KVM 不可用 仍走 Noop) | ✅ | `default_vm_sandbox()` cfg 守门 (per §6.4.9); NoopVMSandbox 行为不变 (vm_sandbox.rs:319-344) |
| 9 重 v9 + 13 键 verdict cache + 3 不可变脊柱 不破 | ✅ | `VMSandbox` trait 0 改, 仅加 LibkrunVMSandbox 实现 |
| 8 哲学锚穿透 (S-1/S-2/S-3/O-1/O-2/O-3/O-4/O-5) | ✅ | sandbox_ffi_libkrun.rs doc 头部标 (per `vm_sandbox.rs:3-15` 同款) |
| 全 OS 路径覆盖 (10 平台) | ✅ | per §6.4.1 (10 平台表) + §6.4.7 (决策矩阵) |
| CI 真接 (Linux ubuntu-22.04/24.04) | ✅ | per §6.4.3 (完整 workflow) + §6.4.10 (OS matrix) |
| Windows Noop 兜底 (主人本机) | ✅ | per §6.4.2 (主人场景详解) |

---

## §11 中文 commit msg 模板 (待主人拍板后用)

```
feat(companion): Stage 2 microVM 真接 libkrun (Linux KVM + macOS HVF, 全 OS 路径覆盖)

- 严守 0 装 PASS (NoopVMSandbox 仍存在, Windows / 0 装 / KVM 不可用 / HVF 不可用 / 容器无 KVM 透传
  / BSD 全部走 Noop) + 9 重 v9 + 13 键 verdict cache + 3 不可变脊柱 + workspace.version 1.2.0
  + 0 改 24 LOCKED crate 入口 + 8 哲学锚穿透 (S-1/S-2/S-3/O-1/O-2/O-3/O-4/O-5)
- 主人 2026-08-20 拍板: libkrun 真接 (不用 0 star smol-vm) + Windows 留 Noop + CI Linux 真接
- 新增 crates/apeireth-companion/src/sandbox_ffi_libkrun.rs
  (LibkrunVMSandbox: krun_create_ctx + krun_set_vm_config + krun_add_disk + krun_start;
   #![allow(unsafe_code)] 单文件收敛, per vm_sandbox.rs:33-34 模式; Linux KVM + macOS HVF 双 backend)
- 默认工厂 default_vm_sandbox() 加 3 段 cfg 守门 (per §6.4.9):
  - Linux + feature libkrun + KVM 可用 → LibkrunVMSandbox
  - macOS + feature libkrun + HVF 可用 → LibkrunVMSandbox
  - 其它一切情况 (Windows / 0 装 / KVM/HVF 不可用 / BSD / 容器无透传) → NoopVMSandbox (0 装 PASS)
- 新增 Cargo feature flag: libkrun = ["dep:libkrun"] (默认 [] 不开)
  (libkrun = { version = "1.4", optional = true }, 仅 Linux/macOS target 编译, Windows target 跳过)
- 新增 2 集成测: sandbox_integration_libkrun_e2e.rs + sandbox_high_risk_shell_e2e.rs
  (1 spawn 1 destroy ~125ms 冷启动; receipt.vm=true 真走加固; shell 真用 VM 隔离验证不污染 host)
- 新增 CI matrix: .github/workflows/sandbox-libkrun-e2e.yml
  (ubuntu-22.04/24.04 真接 + /dev/kvm + libkrun 编译 + rootfs-busybox 准备;
   windows-2022 Noop 测; macos-14 Noop 测; 详 §6.4.10)
- 全 OS 路径文档覆盖 (per §6.4): 10 平台表 + 主人 Windows 详解 + CI Linux 详解 + macOS + Docker/k8s + 远程 KVM + 决策矩阵 + Cargo features + factory cfg 守门 + CI matrix
- 报告: reports/smol-vm-implementation-spec-2026-08-20.md
- 借用 ID: R-side sandbox-2026-08-20-real-libkrun (替换 NoopVMSandbox 默认路径, 全 OS 路径讲清)
```

---

## §12 文档元信息

| 项 | 内容 |
|---|---|
| 报告路径 | `reports/smol-vm-implementation-spec-2026-08-20.md` |
| 引用文件 | `crates/apeireth-companion/src/{vm_sandbox,sandbox_net,sandbox_integration,sandbox_pass,tool_bridge,lib,experiment_field}.rs` (per §1-§7 引用行号) |
| 上游公开主页 | libkrun (`containers/libkrun` Red Hat 维护) + Firecracker (`firecracker-microvm/firecracker` AWS Lambda); **不接 smol-vm 仓库** (0 star orphan + 抽象错位) |
| 8 哲学锚穿透 | S-1 (北极星 = 真接 microVM 隔离蠕虫) / S-2 (实事求是 = libkrun 维护方稳 + 0 假装已接) / S-3 (质量工程化 = 单测 + E2E + CI matrix 守门) / O-1 (安全优先 = 仅高危工具启用 + 9 重 v9 + 13 键 verdict cache 不破) / O-2 (走在前人肩上 = 借 Firecracker minimal API + libkrun C 库分层, 不接 smol-vm 仓库) / O-3 (干到底 = 推荐选项 B 3-4 天干完) / O-4 (任何人都能接手 = §6.4 全 OS 路径 10 子节 + §4.2 改动 list 逐文件逐行) / O-5 (不假装 = NoopVMSandbox 仍存在, 0 装路径仍返 Err) |
| 0 触碰承诺 | 0 改 Stage 1/2/3 已落地代码 (sandbox_net.rs / sandbox_integration.rs / sandbox_pass.rs / tool_bridge.rs / experiment_field.rs 全 0 改) / 0 改 Cargo.toml[workspace] / 0 改 enum/const / 0 改 24 LOCKED / 0 触碰 gh_*.ps1 / 0 触碰 2 tests/ |
| 唯一允许新增 | 1 文件 (`sandbox_ffi_libkrun.rs` ~200 行) + 2 集成测 (~160 行) + 1 CI matrix workflow (~50 行) + 1 binary fixture (rootfs-busybox.tar ~5 MB) + 6 行 Cargo.toml (libkrun Linux/macOS-only dep + feature flag + [target.cfg]) + 2 行 lib.rs (pub mod + feature 段) + 10 行 vm_sandbox.rs 工厂 cfg 分支 (per §6.4.9 完整守门) |
| 全 OS 路径覆盖 | **10 平台表** (§6.4.1) + **主人 Windows 详解** (§6.4.2) + **CI Linux 完整 workflow** (§6.4.3) + **macOS HVF** (§6.4.4) + **Docker/k8s 容器透传** (§6.4.5) + **远程 KVM 服务器** (§6.4.6) + **决策矩阵** (§6.4.7) + **Cargo features 完整设计** (§6.4.8) + **factory cfg 守门** (§6.4.9) + **CI matrix** (§6.4.10) |
| 借用 ID | R-side sandbox-2026-08-20-real-libkrun (主人 2026-08-20 拍板 libkrun 真接 + 全 OS 路径) |

> **0 主动 commit 严守**: 本文档写到 `reports/` 但 0 主动 commit, 等主人拍板后另起 R-side 实施 commit (per `Cargo.toml:289` C1 0 主动 commit 已放宽 R126 → 调研报告类仍待主人拍板)。
>
> **0 主动 push 严守**: 0 主动 push, 等 1.0 release 配 GitHub remote (per `Cargo.toml:289` 0 主动 push 严守)。
>
> **主人 2026-08-20 拍板已记录** (§0 TL;DR + §6.4.7 决策矩阵): **libkrun 真接** (不用 0 star smol-vm) + **Windows 留 Noop 0 装** + **CI Linux runner 真接**。核心矛盾点 (§4.1) 已解决: 真接必改 vm_sandbox.rs 工厂分支 + 加 libkrun 依赖 + 新建 sandbox_ffi_libkrun.rs, 主人拍板放宽 "0 触碰 src/" 限于 Stage 1/2/3 已落地代码 (sandbox_net.rs / sandbox_integration.rs / sandbox_pass.rs / tool_bridge.rs 全 0 改), 允许新建 + 加 Linux/macOS-only 依赖 + 改工厂分支。**严守**: 0 改 enum/const/24 LOCKED/workspace.version。

---

*End of spec. 主人 2026-08-20 拍板: libkrun 真接 (不接 smol-vm 0 star orphan) + Windows 留 Noop 0 装 + CI Linux 真接; 全 OS 路径已讲清 (§6.4.1-§6.4.10 共 10 子节, 覆盖 Linux/Windows/macOS/BSD/Docker/k8s/WSL/远程 KVM 8 类部署); 推荐选项 B (3-4 天); 等主人拍板 0 触碰红线放宽后另起 R-side 实施 commit.*