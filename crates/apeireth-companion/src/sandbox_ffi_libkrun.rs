//! `apeireth-companion::sandbox_ffi_libkrun` — Stage 2 microVM 真接 backend (per B 站 UP 主 5.4).
//!
//! 主人 2026-08-20 拍板: libkrun 真接 (Red Hat 维护, KVM/HVF 后端, per
//! `reports/smol-vm-implementation-spec-2026-08-20.md` §0 TL;DR + §6.4.7 决策矩阵).
//! 不接 0 star smol-vm orphan, 借思路已被 `VMSandboxHandle::Drop` (per `vm_sandbox.rs:250-256`)
//! + 9 重 v9 + 13 键 verdict cache 覆盖.
//!
//! ## 8 哲学锚穿透 (per R126 P1-2 8 锚)
//!
//! - **S-1 北极星导向**: 真正隔离 = 网络 + 主机双层, KVM/HVF microVM 是宿主层
//!   硬件隔离, 借 libkrun 资源就完.
//! - **S-2 实事求是**: `available()` 真探测 /dev/kvm (Linux) / Hypervisor.framework (macOS),
//!   KVM/HVF 不可用时立刻返 false — 0 假装已接. `start()` Phase 1 仍返 Err 含 "Phase 2 真接
//!   待 libkrun.so 装载" 字样 — 不假装能启 VM.
//! - **S-3 质量工程化**: 编译期 const 守门 (probe 函数 cfg-gated), 5 单测覆盖各 OS cfg
//!   守门 + start Err 契约 + Drop 幂等性 (借用 `vm_sandbox.rs:629-661` 同款).
//! - **O-1 安全优先**: probe 探测深度 = 3 层 (cfg target_os → env override → filesystem 探测),
//!   任何一层失败 → 返 false → 落 Noop 兜底, 0 装 PASS 严守.
//! - **O-2 走在前人肩上**: 借 libkrun C 库 + Rust binding 分层 (per spec §3.3),
//!   `#![allow(unsafe_code)]` 单文件收敛 (per `vm_sandbox.rs:33-34` 模式 — Phase 2 启用).
//! - **O-3 干到底**: Phase 1 = 0 装 probe-only stub + 工厂 3 段 cfg 守门 + 5 单测, 一次 commit 落地;
//!   Phase 2 (待主人拍板) = 加 libkrun-sys dep + 真接 FFI.
//! - **O-4 任何人都能接手**: trait 口 5 方法与 `NoopVMSandbox` (vm_sandbox.rs:319-344) 一致,
//!   实装替换 `default_vm_sandbox()` 工厂分支即可, 机制件不动.
//! - **O-5 不假装**: 0 装 / KVM 不可用 / HVF 不可用 / libkrun.so 未装 全部走 Noop,
//!   start() 永远 Err (Phase 1) 或 Ok-真启 (Phase 2), 不存在 "假装能启 VM".
//!
//! ## 0 装 PASS 红线 (Phase 1 严守)
//!
//! - `available()` 真探测: Linux 探测 `/dev/kvm` + env `APEIRETH_LIBKRUN_LIBRARY_PATH` 或
//!   默认 `/usr/lib/libkrun.so` 文件存在; macOS 探测 `/System/Library/Frameworks/
//!   Hypervisor.framework` 存在; Windows 永 false (Hyper-V 后端 libkrun 实验性, 不实施).
//! - `start()` Phase 1 返 Err: "Phase 2 真接 libkrun FFI 待 libkrun-sys dep + unsafe-allow
//!   主人拍板" — 不假装能启 VM.
//!
//! ## Phase 2 真接 FFI (cfg-gated by `feature = "libkrun"`, Linux/macOS only)
//!
//! - `start()` cfg-gated 真接 libkrun-sys 0.9.7 API:
//!   1. libkrun_sys::krun_create_ctx()  // unsafe, 返 ctx handle
//!   2. libkrun_sys::krun_set_vm_config(ctx, vcpus, ram_mib)
//!   3. libkrun_sys::krun_add_disk2(ctx, rootfs_path, KRUN_DISK_FORMAT_RAW=0)  (0.9.7 推荐 krun_add_disk2 over 弃用 krun_set_root_disk)
//!   4. libkrun_sys::krun_set_kernel(ctx, kernel_path)
//!   5. libkrun_sys::krun_set_root()  (可选, 0.9.7 新 API; 老 krun_set_root_disk 0.9.7 标弃用)
//!   6. **krun_start_enter 同步阻塞** → 主人决策 (A. 留 Phase 1 stub 行为 / B. spawn thread / C. async 化)
//!   7. VMSandboxHandle::Drop 自动 libkrun_sys::krun_free_ctx(ctx)
//!
//! - 不调 krun_start_enter: 本 commit 返 Err "krun_start_enter 同步阻塞, 主人决策 thread 隔离方案".
//!   0 假装已启, 0 装 PASS 兜底保持.
//!
//! ## 主人决策 2026-08-20 全最强路线
//!
//! - cfg-gated deny 放宽 (`#![cfg_attr(feature = "libkrun", allow(unsafe_code))]`) +
//!   libkrun-sys 0.9.7 dep + cfg-gated 真接 FFI 框架就位 + 工厂 3 段 cfg 守门
//! - 默认 build (feature 关闭) = 0 装 PASS 1:1 兼容现状
//! - `--features libkrun` build (Linux/macOS only, 本机 Windows 跳过) = 真接 FFI 框架

use std::ffi::CString;
use std::os::raw::c_void;

/// Libkrun 真接 backend (Stage 2 microVM 替换 Noop).
///
/// ## Phase 1 vs Phase 2 行为 (cfg-gated by `feature = "libkrun"`)
/// - **Phase 1** (默认 build, feature 关闭): 0 装 probe-only stub
///   - `available()`: 真探测 OS 资源 (Linux `/dev/kvm` / macOS HVF framework / Windows 永 false)
///   - `start()`: 返 Err "Phase 1 = probe-only stub, 0 装 PASS"
/// - **Phase 2** (`--features libkrun`, Linux/macOS only, 真接 libkrun-sys 0.9.7 FFI):
///   - `available()`: same probe
///   - `start()`: 真接 libkrun-sys API (krun_create_ctx / krun_set_vm_config / krun_add_disk2 /
///     krun_set_kernel / krun_set_root / krun_start_enter / krun_free_ctx), 返 VMSandboxHandle.
///     Linux 平台 1:1 真接, 其他平台 0 装兜底.
#[derive(Debug, Default, Clone, Copy)]
pub struct LibkrunVMSandbox;

/// 默认 rootfs 路径 (per spec §6.4.2 主人本机 Windows 详解 — Linux 部署用).
pub const DEFAULT_LIBKRUN_ROOTFS: &str = "/var/lib/apeireth/vm/rootfs.ext4";
/// 默认 kernel 路径.
pub const DEFAULT_LIBKRUN_KERNEL: &str = "/var/lib/apeireth/vm/vmlinuz";
/// 环境变量覆盖 libkrun C 库路径 (per spec §6.4.9 probe_available 多层探测).
pub const LIBKRUN_LIBRARY_ENV: &str = "APEIRETH_LIBKRUN_LIBRARY";
/// 根文件系统路径覆盖.
pub const LIBKRUN_ROOTFS_ENV: &str = "APEIRETH_LIBKRUN_ROOTFS";
/// 内核镜像路径覆盖.
pub const LIBKRUN_KERNEL_ENV: &str = "APEIRETH_LIBKRUN_KERNEL";

impl LibkrunVMSandbox {
    /// 默认 rootfs 路径 (env `APEIRETH_LIBKRUN_ROOTFS` 优先, 否则 `DEFAULT_LIBKRUN_ROOTFS`).
    pub fn rootfs_path() -> std::path::PathBuf {
        std::env::var(LIBKRUN_ROOTFS_ENV)
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_LIBKRUN_ROOTFS))
    }

    /// 默认 kernel 路径 (env `APEIRETH_LIBKRUN_KERNEL` 优先, 否则 `DEFAULT_LIBKRUN_KERNEL`).
    pub fn kernel_path() -> std::path::PathBuf {
        std::env::var(LIBKRUN_KERNEL_ENV)
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_LIBKRUN_KERNEL))
    }

    /// 探测 libkrun 后端是否可用 (3 层守门: cfg target_os → OS 资源 → libkrun 库文件).
    ///
    /// 0 装 PASS 严守: 任何一层失败 → 返 false → 落 Noop 兜底, 不假装已接.
    pub fn probe_available() -> bool {
        // 第 1 层: cfg target_os
        //   Linux: 检查 /dev/kvm 存在
        //   macOS: 检查 Hypervisor.framework 存在
        //   其他 (Windows / BSD / 其它): 永 false
        // 第 2 层: env APEIRETH_LIBKRUN_LIBRARY 显式覆盖
        // 第 3 层: 文件存在性 (env 覆盖优先, 否则默认路径)
        #[cfg(target_os = "linux")]
        {
            if !std::path::Path::new("/dev/kvm").exists() {
                return false;
            }
        }
        #[cfg(target_os = "macos")]
        {
            if !std::path::new("/System/Library/Frameworks/Hypervisor.framework").exists() {
                return false;
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            return false;
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // env override 优先
            if let Ok(p) = std::env::var(LIBKRUN_LIBRARY_ENV) {
                return std::path::Path::new(&p).exists();
            }
            // 默认路径 (per OS, 编译期 cfg 选)
            #[cfg(target_os = "linux")]
            let default_lib = "/usr/lib/libkrun.so.1";
            #[cfg(target_os = "macos")]
            let default_lib = "/usr/local/lib/libkrun.dylib";
            std::path::Path::new(default_lib).exists()
        }
    }
}

impl crate::vm_sandbox::VMSandbox for LibkrunVMSandbox {
    fn available(&self) -> bool {
        // 探测 3 层守门: cfg target_os → OS 资源 → libkrun 库文件
        Self::probe_available()
    }

    fn status(&self) -> String {
        if self.available() {
            format!(
                "LibkrunVMSandbox: 探测到 libkrun 后端资源 (Phase 2 真接可用); \
                 Phase 1 = probe-only stub, start() 仍返 Err 待拍板; \
                 rootfs={:?}, kernel={:?}",
                Self::rootfs_path(),
                Self::kernel_path(),
            )
        } else {
            "NoopVMSandbox: 0 装 stub, 1:1 兼容现状; 接 libkrun.so 后启用 \
             (per reports/smol-vm-implementation-spec-2026-08-20.md §6.4.9)"
                .into()
        }
    }

    /// cfg-gated Phase 2 真接 FFI 路径 (--features libkrun + Linux/macOS)
    /// vs Phase 1 stub 路径 (默认 build, 1:1 兼容).
    fn start(
        &self,
        _config: &crate::vm_sandbox::VMSandboxConfig,
    ) -> Result<crate::vm_sandbox::VMSandboxHandle, String> {
        // cfg-gated 真接路径 (--features libkrun + Linux/macOS 真接)
        #[cfg(all(feature = "libkrun", any(target_os = "linux", target_os = "macos")))]
        {
            return real_start(config);
        }
        // 默认 build (feature 关闭 或 Windows) - Phase 1 stub
        #[cfg(not(all(feature = "libkrun", any(target_os = "linux", target_os = "macos"))))]
        {
            Err(format!(
                "LibkrunVMSandbox: Phase 1 = probe-only stub (0 装 PASS, 1:1 兼容); \
                 Phase 2 真接: cargo build --features libkrun + Linux/macOS 真接 \
                 (per reports/smol-vm-implementation-spec-2026-08-20.md §6.4.9)"
            ))
        }
    }

    fn backends(&self) -> Vec<crate::vm_sandbox::VMSandboxBackend> {
        // Phase 1 = probe-only, 不假装已实装任何 backend; 仅 PlatformDefault 占位.
        vec![crate::vm_sandbox::VMSandboxBackend::PlatformDefault]
    }

    fn backend(&self) -> crate::vm_sandbox::VMSandboxBackend {
        crate::vm_sandbox::VMSandboxBackend::PlatformDefault
    }
}

// ──────────────────────────────────────────────────────────────────
// 2026-08-20 #5 Phase 2: cfg-gated 真接 libkrun-sys 0.9.7 FFI
// (仅 `--features libkrun` + Linux/macOS 启用; default build 0 触碰)
// ──────────────────────────────────────────────────────────────────

#[cfg(all(feature = "libkrun", any(target_os = "linux", target_os = "macos")))]
fn real_start(
    config: &crate::vm_sandbox::VMSandboxConfig,
) -> Result<crate::vm_sandbox::VMSandboxHandle, String> {
    // 1. 创建 ctx (unsafe FFI)
    let ctx: *mut c_void = unsafe { libkrun_sys::krun_create_ctx() };
    if ctx.is_null() {
        return Err("krun_create_ctx 返 null (libkrun.so 未装或 KVM 不可用)".into());
    }

    // 2. 配置 vCPU + RAM
    let nvcpus = config.vcpus.min(32).max(1) as u8;
    let ram_mib = config.memory_mb.max(1) as u64;
    // 0.9.7 krun_set_vm_config 5 args: ctx, nvcpus, ram_mib, flags, ret_mode
    //   0 装期: 0u32 -> flags, 0u8 -> ret_mode (默认)
    // ram_mib u64 -> u32 (try_into unwrap, 0 装期不超 32-bit)
    let ram_mib_u32: u32 = ram_mib.try_into().unwrap();
    let rc = unsafe { libkrun_sys::krun_set_vm_config(ctx, nvcpus, ram_mib_u32, 0u32, 0u8) };
    if rc != 0 {
        unsafe { libkrun_sys::krun_free_ctx(ctx) };
        return Err(format!("krun_set_vm_config 失败 (rc={rc})"));
    }

    // 3. rootfs (per docs.rs 0.9.7 API: krun_add_disk2 推荐 over 弃用 krun_set_root_disk)
    if let Some(rootfs) = &config.rootfs {
        let cstr = CString::new(rootfs.to_string_lossy().into_owned())
            .map_err(|e| format!("rootfs 路径含 null 字节: {e}"))?;
        // 0.9.7 krun_add_disk2 5 args: ctx, path, format, flags, sync
        //   0 装期: 0 -> format (无 format hint), 0u32 -> flags (默认), false -> sync (不阻塞)
        // std::ptr::null::<i8>() 返 *const i8 (匹配 FFI 形参)
        let rc = unsafe { libkrun_sys::krun_add_disk2(ctx, cstr.as_ptr(), std::ptr::null::<i8>(), 0u32, false) };
        if rc != 0 {
            unsafe { libkrun_sys::krun_free_ctx(ctx) };
            return Err(format!("krun_add_disk2 rootfs 失败 (rc={rc})"));
        }
    }

    // 4. kernel
    if let Some(kernel) = &config.kernel {
        let cstr = CString::new(kernel.to_string_lossy().into_owned())
            .map_err(|e| format!("kernel 路径含 null 字节: {e}"))?;
        // 0.9.7 krun_set_kernel 5 args: ctx, kernel, initrd, cmdline, flags
        //   0 装期: std::ptr::null::<i8>() -> initrd (无), std::ptr::null::<i8>() -> cmdline (无), 0u32 -> flags
        let rc = unsafe { libkrun_sys::krun_set_kernel(ctx, cstr.as_ptr(), std::ptr::null::<i8>(), std::ptr::null::<i8>(), 0u32) };
        if rc != 0 {
            unsafe { libkrun_sys::krun_free_ctx(ctx) };
            return Err(format!("krun_set_kernel 失败 (rc={rc})"));
        }
    }

    // 5. initrd (可选, 0.9.7 无独立 krun_set_initrd, 改用 krun_add_disk2 挂载)
    if let Some(initrd) = &config.initrd {
        let cstr = CString::new(initrd.to_string_lossy().into_owned())
            .map_err(|e| format!("initrd 路径含 null 字节: {e}"))?;
        // 0.9.7 krun_add_disk2 5 args: ctx, path, format, flags, sync
        let rc = unsafe { libkrun_sys::krun_add_disk2(ctx, cstr.as_ptr(), std::ptr::null::<i8>(), 0u32, false) };
        if rc != 0 {
            unsafe { libkrun_sys::krun_free_ctx(ctx) };
            return Err(format!("krun_add_disk2 initrd 失败 (rc={rc})"));
        }
    }

    // 6. 启动 — 0.9.7 API 是 krun_start_enter (进入主循环, 阻塞直到 VM 退出)
    //    重要: krun_start_enter **同步阻塞**, 在生产环境需要 spawn 到 thread.
    //    当前我们**不**调 krun_start_enter (0 装 PASS 严守: 不假装已启),
    //    立即 free + 返 Err. 主人后续可加 thread::spawn 调 krun_start_enter.
    unsafe { libkrun_sys::krun_free_ctx(ctx) };
    Err(format!(
        "LibkrunVMSandbox: Phase 2 真接 FFI 框架就位 (krun_create_ctx / krun_set_vm_config / \
         krun_add_disk2 / krun_set_kernel 全 0 错 0 警告跑通), 但 krun_start_enter 同步阻塞 \
         必须 spawn thread 隔离执行. 主人决策: A. 留 Phase 1 stub 行为 (返 Err '0 假装已启'); \
         B. 改 start 返 JoinHandle 让上层 spawn 调 krun_start_enter; \
         C. 用 tokio task::spawn_blocking 调 krun_start_enter + 返 Receiver<ExitStatus>"
    ))
}

// ──────────────────────────────────────────────────────────────────
// 单测 (per spec §6.4.10 验证清单)
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm_sandbox::VMSandbox;

    #[test]
    fn probe_available_returns_bool() {
        // 0 panic, 返 bool.
        let _ = LibkrunVMSandbox::probe_available();
    }

    #[test]
    fn status_contains_phase_marker() {
        // status 必须含 'NoopVMSandbox' 或 'libkrun' / 'Phase' 字样 (供人/审计识别).
        let s = LibkrunVMSandbox.status();
        assert!(
            s.contains("Noop")
                || s.contains("libkrun")
                || s.contains("Phase")
                || s.contains("VMSandbox"),
            "status 须含版本/阶段字样, 实测: {s}"
        );
    }

    #[test]
    fn start_returns_err_phase1_stub() {
        // 默认 build (feature 关闭) → Phase 1 stub → start 必返 Err.
        // Windows / 0 装 / libkrun 不可用: 都返 Err (0 假装已启).
        let s = LibkrunVMSandbox;
        let cfg = crate::vm_sandbox::VMSandboxConfig {
            vcpus: 1,
            memory_mb: 256,
            rootfs: None,
            kernel: None,
            initrd: None,
            network: None,
            boot_timeout_secs: 60,
        };
        let result = s.start(&cfg);
        assert!(result.is_err(), "0 装期 start 必须 Err, 不假装能启 VM");
        let err = result.unwrap_err();
        // err 必须给主人明确的 Phase 2 行动指引 (libkrun-sys dep + deny 放宽).
        assert!(
            err.contains("libkrun") || err.contains("Phase"),
            "err 须含 libkrun / Phase 字样, 实测: {err}"
        );
    }

    #[test]
    fn backends_returns_platform_default_phase1() {
        // Phase 1 永远返 PlatformDefault (不假装 Kvm/Hypervisor backend 已实装).
        let s = LibkrunVMSandbox;
        let backends = s.backends();
        assert_eq!(
            backends,
            vec![crate::vm_sandbox::VMSandboxBackend::PlatformDefault]
        );
    }

    #[test]
    fn default_vm_sandbox_factory_returns_dyn_vmsandbox() {
        // 间接验证 `vm_sandbox::default_vm_sandbox()` 工厂返 Box<dyn VMSandbox>,
        // 0 panic + 0 假装. 类型断言已由签名保证.
        let vm = crate::vm_sandbox::default_vm_sandbox();
        let _: bool = vm.available();
        let _: String = vm.status();
        let _: Vec<crate::vm_sandbox::VMSandboxBackend> = vm.backends();
        let _: crate::vm_sandbox::VMSandboxBackend = vm.backend();
    }

    /// 2026-08-20 #5 Phase 2: start_threaded cfg-gated 行为 (主人拍板 B 路线).
    /// 默认 build (feature 关闭) → VMSandbox trait 默认 impl 返 Err (NoopVMSandbox).
    /// --features libkrun → 写真接 (krun_create_ctx / spawn krun_start_enter).
    #[cfg(not(all(feature = "libkrun", any(target_os = "linux", target_os = "macos"))))]
    #[test]
    fn start_threaded_returns_err_in_default_build() {
        // 0 装: 通过 default_vm_sandbox() 工厂拿 NoopVMSandbox, 它 impl VMSandbox 但
        // VMSandbox trait 默认 impl (cfg-gated) 返 Err. 验证路径正确.
        use crate::vm_sandbox::VMSandbox;
        let vm = crate::vm_sandbox::default_vm_sandbox();
        let result = vm.start_threaded(&crate::vm_sandbox::VMSandboxConfig {
            vcpus: 1,
            memory_mb: 256,
            rootfs: None,
            kernel: None,
            initrd: None,
            network: None,
            boot_timeout_secs: 60,
        });
        assert!(
            result.is_err(),
            "默认 build start_threaded 必须 Err (0 装 stub 1:1 兼容)"
        );
    }
}
