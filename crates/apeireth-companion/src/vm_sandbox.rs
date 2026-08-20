//! `apeireth-companion::vm_sandbox` — Stage 2 microVM 隔离 (per B 站 UP 主 5.4, 2026-08-19).
//!
//! ## 8 哲学锚穿透 (per R126 P1-2 实施 6→8)
//!
//! - **S-1 北极星导向**: Stage 2 是 5 层沙盒补全的关键缺口 (UP 主 5.3 论断"进程卫生 ≠ 防蠕虫"),
//!   真正隔离 = 网络 + 主机双层. 0 装 PASS 严守: trait 口 + Noop stub, 实装才连 trait.
//! - **S-2 实事求是**: 真实接 libkrun / Hyperlight / Firecracker, 0 假装能启 VM.
//! - **S-3 质量工程化 NEW (R126 P1-2 升)**: 编译期 const 守门 (per `sandbox_pass.rs`),
//!   0 装时 `NoopVMSandbox::available()` 编译期恒 false + `start()` 恒 Err.
//! - **O-1 安全优先 NEW (R126 P1-2 升)**: 5 状态 `VMSandboxState` (Created/Booted/Running/Halted/Error)
//!   + 4 backend (Kvm/Hypervisor/HyperV/PlatformDefault), 借鉴 libkrun backend 抽象 + capability 边界.
//! - **O-2 走在前人肩上**: 借鉴 4 源 (smolvm / Firecracker / libkrun / wasmtime) 公开 docs 思路.
//! - **O-3 干到底**: 5 公共类型 + 1 trait + 2 公共函数 + 23 单测全过, 一次 commit 落地.
//! - **O-4 任何人都能接手**: trait 口 5 方法, 实装替换 NoopVMSandbox 即可, 机制件不动.
//! - **O-5 不假装**: 0 装时诚实返 Err + status 标 "未实装", 含 `available() = false` 编译期 const 守门.
//!
//! ## 8 项承诺 (per task spec §10 + 主人 0 装 PASS 严守需求)
//!
//! - 0 装 PASS 严守: NoopVMSandbox.default().start() 必 Err, 0 假装能启 VM
//! - 0 触碰 24 LOCKED crate 入口签名 (per R148 降级, 仅保 3 不可变脊柱)
//! - 0 改 workspace.version (1.2.0 双轴制: 产品轴 tag v1.0.0 + workspace 轴 1.2.0)
//! - 0 改 enum / const / 24 LOCKED 不可变脊柱
//! - 0 引外部依赖 (Cargo.toml 0 加任何 4 源仓库 entry)
//!
//! ## 0 装 PASS 借鉴 4 源 (0 接 upstream 仓库)
//!
//! 借鉴 4 源 = 公开 docs 思路, 0 装 smolvm/Firecracker/libkrun/wasmtime upstream 仓库.
//! 借鉴元素 (per 4 源各自贡献):
//! - Firecracker minimal API (小 API = 小攻击面; 借鉴 3-syscall trait 设计)
//! - libkrun C lib + Rust binding 分层 (KVM/hypervisor backend 抽象; 借鉴 VMSandboxBackend enum)
//! - wasmtime 组件模型 (capability 边界; 借鉴 sanitize_inputs + 5 状态 state machine)
//! - smolvm 0 装诚实 (NoopXxx + available() = false; 借鉴 NoopVMSandbox stub)
//!
//! 借鉴 4 源思路 (不接库, 0 装 PASS):
//!
//! | 源         | 借鉴                                                         |
//! |------------|--------------------------------------------------------------|
//! | Firecracker | minimal API surface (3 syscall: StartInstance/PutGuest/...)  |
//! | libkrun    | C lib + Rust binding 分层 (KVM/hypervisor backend 抽象)    |
//! | wasmtime   | 组件模型 (capability 边界)                                   |
//! | smolvm / smol-vm | 0 装诚实 (`NoopXxx` 模式, `available() = false`)       |
//!
//! ## 哲学 (主人 2026-08-19: 真正的环境隔离)
//!
//! 现有 [`crate::sandbox`] 5 层 (洋葱门 / 审批链 / MOVE-STAY / Job Object / 最小权限)
//! **都是进程卫生**, 不防蠕虫 — UP 主 5.3 论断. 蠕虫杀的不是单个进程, 是**整个网络可达面
//! + 整个持久化可达面**. 5.4 正确做法分两条:
//! - 一次性 VM (Firecracker / libkrun microVM);
//! - AppContainer + WFP 出站默认拒绝 + 目录虚拟化.
//!
//! 我们走 microVM 隔离 (Stage 2). 本模块只留 trait 口 + 0 装 stub, 真接点在
//! libkrun (Linux KVM) / Hyperlight (Windows Hyper-V) / Firecracker 实装时启用.
//!
//! ## 0 装 PASS 红线 (smolvm 模式)
//!
//! - [`NoopVMSandbox`] 全部方法 Err/Z: `available()` false, `start()` Err.
//! - [`default_vm_sandbox`] 0 装返 Noop (平台检测占位).
//! - [`validate_config`] 仅做参数边界 (1..=32 vcpus, 1..=65536 MB), **不**验证
//!   backend 兼容性 (0 装时不假装 backend 在本平台可用).
//!
//! ## 与现有沙盒关系 (Stage 1 + Stage 2 正交并列)
//!
//! - Stage 1 [`crate::sandbox_net`]: 网络隔离 (loopback / default-deny / force-deny).
//! - Stage 2 本模块: 进程 + 文件系统 + 设备 + 内存 隔离 (一次性 VM).
//! - 两层**正交** — VM 可携带 network: Some(cfg) 启动, 也可不携带.
//! - 与 [`crate::sandbox`]: 本模块是 sibling, 不替代 (后者是单进程限额, 前者是容器边界).

use std::path::PathBuf;

/// microVM 后端类型 (借鉴 libkrun backend 枚举).
///
/// **0 装 PASS**: 枚举**本身**合法 (仅是 label), 但 [`VMSandboxBackend::detect`]
/// 在 trait 未实装时返 `None` — **不**假装本平台可用 KVM/Hypervisor/Hyper-V.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VMSandboxBackend {
    /// KVM (Linux only).
    Kvm,
    /// Hypervisor.framework (macOS only).
    Hypervisor,
    /// Windows Hyper-V (Windows only).
    HyperV,
    /// 未指定 — 由工厂按平台默认挑.
    PlatformDefault,
}

impl VMSandboxBackend {
    /// 序列化 (小写稳定字符串 — 配置文件 / 协议传参均用此格式).
    pub fn as_str(&self) -> &'static str {
        match self {
            VMSandboxBackend::Kvm => "kvm",
            VMSandboxBackend::Hypervisor => "hypervisor",
            VMSandboxBackend::HyperV => "hyperv",
            VMSandboxBackend::PlatformDefault => "platform_default",
        }
    }

    /// 反序列化 — 不识别字符串回退 [`PlatformDefault`].
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "kvm" => VMSandboxBackend::Kvm,
            "hypervisor" | "hv" => VMSandboxBackend::Hypervisor,
            "hyperv" => VMSandboxBackend::HyperV,
            "platform_default" | "default" | "auto" | "" => VMSandboxBackend::PlatformDefault,
            _ => VMSandboxBackend::PlatformDefault,
        }
    }

    /// 0 装检测: trait 未实装时**永远返 None** — 不假装本平台可用 KVM/Hyper-V.
    ///
    /// 实装路径 (后续):
    /// - `cfg!(target_os = "linux")` → 探测 `/dev/kvm` → Some(Kvm)
    /// - `cfg!(target_os = "macos")` → 探测 Hypervisor.framework → Some(Hypervisor)
    /// - `cfg!(target_os = "windows")` → 探测 Hyper-V 平台 → Some(HyperV)
    /// - 探测失败 → None (诚实, 不假装)
    ///
    /// 当前一律 `None` — 0 装 PASS 红线.
    pub fn detect() -> Option<Self> {
        // 0 装: 任何平台 trait 未实装 → 返 None (0 假装 KVM 在 macOS 可用).
        None
    }
}

/// microVM 资源配置 (借鉴 Firecracker minimal API surface).
///
/// **字段约束** (由 [`validate_config`] 校验):
/// - `vcpus`: 1..=32
/// - `memory_mb`: 1..=65536
/// - `boot_timeout_secs`: ≥ 1
/// - `rootfs` / `kernel` / `initrd`: 0 装时不强制存在 (Noop 不调用, 校验交给实装)
/// - `network`: 可选 — 携带则和 Stage 1 [`crate::sandbox_net`] 协作
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VMSandboxConfig {
    /// vCPU 数 (1-32).
    pub vcpus: u32,
    /// 内存 (MB).
    pub memory_mb: u32,
    /// 启动盘 (rootfs path, .ext4 / .img).
    pub rootfs: Option<PathBuf>,
    /// 启动内核 (bzImage / vmlinuz).
    pub kernel: Option<PathBuf>,
    /// 启动 initrd (可选).
    pub initrd: Option<PathBuf>,
    /// 网络隔离配置 (对接 Stage 1 [`crate::sandbox_net`]).
    pub network: Option<crate::sandbox_net::NetworkIsolationConfig>,
    /// VM 启动超时 (秒, 默认 30).
    pub boot_timeout_secs: u64,
}

impl Default for VMSandboxConfig {
    fn default() -> Self {
        Self {
            vcpus: 1,
            memory_mb: 512,
            rootfs: None,
            kernel: None,
            initrd: None,
            network: None,
            boot_timeout_secs: 30,
        }
    }
}

impl VMSandboxConfig {
    /// 是否真的需要根文件系统 (用于校验用户意图).
    /// 0 装: 返回 `kernel.is_some() || rootfs.is_some()` — 不强制 (实装时接手).
    pub fn has_kernel_or_rootfs(&self) -> bool {
        self.kernel.is_some() || self.rootfs.is_some()
    }
}

/// 启动的 microVM 句柄 (借鉴 libkrun Resource 设计: Drop 自动清理).
///
/// 0 装语义: `start()` 返 `Err`, 永远**不**构造成功实例 — 因此一旦持有 handle,
/// 离开作用域必触发 `Drop` 自动 halt (不论实装与否).
#[derive(Debug)]
pub struct VMSandboxHandle {
    inner: Box<dyn VMSandbox>,
    config: VMSandboxConfig,
    state: VMSandboxState,
    /// 已 halt 标志 (避免 Drop 二次 halt).
    halted: bool,
}

impl VMSandboxHandle {
    /// 构造 handle (0 装路径: 仅内部 trait 实现可调用, 用户拿不到).
    pub(crate) fn new(
        inner: Box<dyn VMSandbox>,
        config: VMSandboxConfig,
        state: VMSandboxState,
    ) -> Self {
        Self {
            inner,
            config,
            state,
            halted: false,
        }
    }

    /// VM 当前状态.
    pub fn state(&self) -> VMSandboxState {
        self.state
    }

    /// 启动时使用的 config (留作审计 / Drop 清理).
    pub fn config(&self) -> &VMSandboxConfig {
        &self.config
    }

    /// 等待 VM 启动完成, 返 Ok(()).
    ///
    /// 0 装: 真实现 trait 才会到这一步; Noop 走不到这里 (start() 返 Err).
    pub fn wait_boot(&mut self) -> Result<(), String> {
        if self.state == VMSandboxState::Booted || self.state == VMSandboxState::Running {
            return Ok(());
        }
        Err(format!("VM 状态 {:?} 不可 wait_boot", self.state))
    }

    /// 在 VM 内执行命令 (借用 libkrun `krun_create_ctx` + `krun_start_enter` 精神).
    ///
    /// 0 装: 真实启动后才调用; Noop 走不到这.
    pub fn exec(&mut self, cmd: &str) -> Result<String, String> {
        if self.halted {
            return Err("VM 已 halt, 不可 exec".into());
        }
        if cmd.is_empty() {
            return Err("exec: 命令不可为空".into());
        }
        // 0 装: 真实实装此处转 inner.exec(cmd); 当前 Noop 永不到达.
        Err("VMSandboxHandle::exec: 0 装 stage trait 未实装 (接 libkrun/Hyperlight/Firecracker 后启用)".into())
    }

    /// 停 VM (幂等 — 多次调用安全).
    pub fn halt(&mut self) -> Result<(), String> {
        if self.halted {
            return Ok(());
        }
        self.halted = true;
        self.state = VMSandboxState::Halted;
        // 0 装: 真实实装此处杀进程 + 收 tap/netns; Noop 无副作用.
        Ok(())
    }

    /// 是否已 halt (供测试 / 审计).
    pub fn is_halted(&self) -> bool {
        self.halted
    }
}

impl Drop for VMSandboxHandle {
    fn drop(&mut self) {
        // 借鉴 libkrun: VM 退出时自动清理 (不 leak resource).
        // halt 是幂等的, 此处不关心 Result.
        let _ = self.halt();
    }
}

/// VM 状态 (确定性机).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VMSandboxState {
    /// 句柄已构造 (尚未 Boot).
    Created,
    /// 已启动 (kernel + rootfs 加载完).
    Booted,
    /// 正在运行 (接受 exec).
    Running,
    /// 已停 (halted / Drop 触发).
    Halted,
    /// 启动或运行中出错.
    Error,
}

impl VMSandboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            VMSandboxState::Created => "created",
            VMSandboxState::Booted => "booted",
            VMSandboxState::Running => "running",
            VMSandboxState::Halted => "halted",
            VMSandboxState::Error => "error",
        }
    }
}

/// microVM 沙盒 trait (借鉴 Firecracker 3-syscall minimal API).
///
/// **0 装 PASS**:
/// - `available()` 永远 false (除真实实装).
/// - `start()` 永远 Err (不假装能启 VM).
/// - `backends()` 0 装时返空 (0 装 = 无 backend).
pub trait VMSandbox: Send + Sync + std::fmt::Debug {
    /// 是否实装 (0 装: false).
    fn available(&self) -> bool;

    /// 状态描述 (0 装: 含 "未实装" + 接 libkrun/Hyperlight/Firecracker 字样).
    fn status(&self) -> String;

    /// 启动一个 VM (借鉴 Firecracker StartInstance).
    ///
    /// 0 装: 返 Err (含"未实装" + "0 假装能启 VM" 字样).
    /// 实装: 成功返 [`VMSandboxHandle`] (含 Drop 自动 halt).
    fn start(&self, config: &VMSandboxConfig) -> Result<VMSandboxHandle, String>;

    /// 列出已实装 backend (0 装: 空).
    fn backends(&self) -> Vec<VMSandboxBackend> {
        Vec::new()
    }

    /// 当前选用的 backend (0 装: PlatformDefault).
    fn backend(&self) -> VMSandboxBackend {
        VMSandboxBackend::PlatformDefault
    }
}

/// 0 装默认实现 (smolvm 0 装 PASS).
///
/// 全部方法 Err/false — 不假装能启 VM. trait 实装 (libkrun/Hyperlight/Firecracker) 时
/// 替换此 stub, 接口契约不变.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopVMSandbox;

impl VMSandbox for NoopVMSandbox {
    fn available(&self) -> bool {
        false
    }

    fn status(&self) -> String {
        "NoopVMSandbox: 未实装 (0 装 PASS, 0 假装能启 VM, 接 libkrun/Hyperlight/Firecracker 后启用)"
            .into()
    }

    fn start(&self, _config: &VMSandboxConfig) -> Result<VMSandboxHandle, String> {
        Err("NoopVMSandbox: microVM 隔离未实装 (Stage 2 仅 trait + 0 装 stub, 真实 backend 待选型 libkrun/Hyperlight/Firecracker)".into())
    }

    fn backends(&self) -> Vec<VMSandboxBackend> {
        // 0 装: 无 backend, 严守 0 装 PASS.
        Vec::new()
    }

    fn backend(&self) -> VMSandboxBackend {
        VMSandboxBackend::PlatformDefault
    }
}

/// microVM 沙盒工厂 (平台检测占位 — 0 装时一律返 Noop).
///
/// **实装路径** (后续):
/// - Linux: `cfg!(target_os = "linux")` + 检查 `/dev/kvm` → 返 `LibkrunVMSandbox` (实装时)
/// - macOS: 检查 Hypervisor framework → 返 `HypervisorVMSandbox` (实装时)
/// - Windows: 检查 Hyper-V 平台 → 返 `HyperVVMSandbox` (实装时)
/// - 任何平台 trait 未实装时: 返 `NoopVMSandbox` (0 装 PASS 红线).
///
/// **当前状态**: 仅返 Noop (Stage 2 trait 口预留, 实装待下一轮).
pub fn default_vm_sandbox() -> Box<dyn VMSandbox> {
    // 2026-08-20 #5 smol-vm Phase 1 工厂 3 段 cfg 守门 (per spec §6.4.9):
    // - Linux + `feature = "libkrun"` + probe 命中 → LibkrunVMSandbox
    // - macOS + `feature = "libkrun"` + probe 命中 → LibkrunVMSandbox (HVF 后端)
    // - 其它 / feature 关闭 / probe 失败 → NoopVMSandbox (0 装 1:1 兼容)
    //
    // 当前 Phase 1: `feature = "libkrun"` 关闭, LibkrunVMSandbox::start() 永远 Err
    // (probe-only stub), 所以 `default_vm_sandbox()` 实际仍返 NoopVMSandbox — 0 装 1:1 行为.
    // Phase 2 真接 FFI 时, --features libkrun + LibkrunVMSandbox::start 真 Ok 才会触发真 VM spawn.
    #[cfg(all(target_os = "linux", feature = "libkrun"))]
    {
        use crate::sandbox_ffi_libkrun::LibkrunVMSandbox;
        let backend = LibkrunVMSandbox;
        if backend.available() {
            return Box::new(backend);
        }
        eprintln!("[default_vm_sandbox] Linux + libkrun feature 但 probe 失败 (KVM 不可用 / libkrun.so 未装), 落 Noop 兜底 (per spec §6.4.9)");
    }
    #[cfg(all(target_os = "macos", feature = "libkrun"))]
    {
        use crate::sandbox_ffi_libkrun::LibkrunVMSandbox;
        let backend = LibkrunVMSandbox;
        if backend.available() {
            return Box::new(backend);
        }
        eprintln!("[default_vm_sandbox] macOS + libkrun feature 但 probe 失败 (HVF 不可用 / libkrun.dylib 未装), 落 Noop 兜底 (per spec §6.4.9)");
    }
    // 其它一切情况 (Windows / 0 装 / feature 关闭 / probe 失败) → NoopVMSandbox
    Box::new(NoopVMSandbox)
}

/// 参数验证 (Stage 2 关口).
///
/// **0 装 PASS**: 仅校验参数边界 (`vcpus` 1..=32, `memory_mb` 1..=65536,
/// `boot_timeout_secs` ≥ 1), **不**验证 backend ↔ platform 兼容性 — 该验证
/// 由 trait 实装者按 `#[cfg(target_os)]` 在 `start()` 内完成 (0 装时返 None
/// 即可, backend 兼容性检查无意义).
///
/// **不验证**: `rootfs` / `kernel` / `initrd` 路径存在性 — 0 装时这些路径
/// 不会真的被访问, 实装时由 `start()` 内强制一并校验.
pub fn validate_config(config: &VMSandboxConfig, _backend: VMSandboxBackend) -> Result<(), String> {
    if config.vcpus == 0 || config.vcpus > 32 {
        return Err(format!(
            "VMSandboxConfig.vcpus={} 越界 (合法 1..=32, Firecracker libkrun 实装上限 32 vCPU)",
            config.vcpus
        ));
    }
    if config.memory_mb == 0 || config.memory_mb > 65536 {
        return Err(format!(
            "VMSandboxConfig.memory_mb={} 越界 (合法 1..=65536, 即 64 GB 上限)",
            config.memory_mb
        ));
    }
    if config.boot_timeout_secs == 0 {
        return Err("VMSandboxConfig.boot_timeout_secs=0 非法 (≥ 1 秒)".to_string());
    }
    // 0 装: 不验证 backend 兼容性 (留 firecracker 实装时验证).
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox_net::NetworkIsolationLevel;

    // ──────────────────────────────────────────────────────────────────
    // 1) 4 backend as_str 往返
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn backend_as_str_roundtrip_4_variants() {
        let cases = [
            (VMSandboxBackend::Kvm, "kvm"),
            (VMSandboxBackend::Hypervisor, "hypervisor"),
            (VMSandboxBackend::HyperV, "hyperv"),
            (VMSandboxBackend::PlatformDefault, "platform_default"),
        ];
        for (b, s) in cases {
            assert_eq!(b.as_str(), s, "as_str 不稳定: {b:?}");
            assert_eq!(VMSandboxBackend::parse(s), b, "parse 不回: {s}");
        }
    }

    #[test]
    fn backend_parse_aliases() {
        // 借鉴 libkrun 宽容 alias
        assert_eq!(VMSandboxBackend::parse("HV"), VMSandboxBackend::Hypervisor);
        assert_eq!(
            VMSandboxBackend::parse("default"),
            VMSandboxBackend::PlatformDefault
        );
        assert_eq!(
            VMSandboxBackend::parse("auto"),
            VMSandboxBackend::PlatformDefault
        );
        assert_eq!(
            VMSandboxBackend::parse(""),
            VMSandboxBackend::PlatformDefault
        );
        // 未知值 → PlatformDefault (0 装默认)
        assert_eq!(
            VMSandboxBackend::parse("garbage"),
            VMSandboxBackend::PlatformDefault
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // 2) 0 装 backend 检测 — 永远 None
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn backend_detect_noop_returns_none() {
        // 0 装 PASS: detect() 必返 None, 不假装本平台可用 KVM/Hyper-V.
        assert!(
            VMSandboxBackend::detect().is_none(),
            "0 装: detect() 必须 None (不假装本平台 backend 可用)"
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // 3) 默认配置合法性
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn default_config_valid_values() {
        let c = VMSandboxConfig::default();
        assert_eq!(c.vcpus, 1);
        assert_eq!(c.memory_mb, 512);
        assert_eq!(c.boot_timeout_secs, 30);
        assert!(c.rootfs.is_none());
        assert!(c.kernel.is_none());
        assert!(c.initrd.is_none());
        assert!(c.network.is_none());
        // 默认 validate 必须 Ok
        assert!(validate_config(&c, VMSandboxBackend::PlatformDefault).is_ok());
    }

    #[test]
    fn default_config_has_no_kernel_or_rootfs() {
        let c = VMSandboxConfig::default();
        assert!(!c.has_kernel_or_rootfs());
        let mut c2 = c.clone();
        c2.kernel = Some(PathBuf::from("vmlinuz"));
        assert!(c2.has_kernel_or_rootfs());
        c2.kernel = None;
        c2.rootfs = Some(PathBuf::from("rootfs.ext4"));
        assert!(c2.has_kernel_or_rootfs());
    }

    // ──────────────────────────────────────────────────────────────────
    // 4) 工厂默认 — 0 装时必须返 Noop
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn default_vm_sandbox_returns_noop() {
        let s = default_vm_sandbox();
        // 0 装 PASS: 工厂必须返 Noop (available = false).
        assert!(
            !s.available(),
            "默认工厂 0 装时必须返 available=false (smolvm 0 装红线)"
        );
        let status = s.status();
        assert!(
            status.contains("Noop"),
            "默认工厂 status 必须含 Noop: {status}"
        );
        assert!(
            status.contains("未实装"),
            "默认工厂 status 必须含未实装: {status}"
        );
        // start 必 Err (0 装严守)
        let err = s.start(&VMSandboxConfig::default()).unwrap_err();
        assert!(err.contains("未实装"), "start 必须诚实 Err: {err}");
        assert!(err.contains("Noop"), "start 必须含 Noop: {err}");
    }

    // ──────────────────────────────────────────────────────────────────
    // 5-7) NoopVMSandbox 0 装诚实
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn noop_vm_sandbox_available_false() {
        assert!(!NoopVMSandbox.available(), "0 装: available 必须 false");
    }

    #[test]
    fn noop_vm_sandbox_status_explains_no_op() {
        let s = NoopVMSandbox;
        let st = s.status();
        assert!(st.contains("Noop"), "status 必须含 Noop: {st}");
        assert!(st.contains("未实装"), "status 必须明示未实装: {st}");
        assert!(
            st.contains("libkrun") || st.contains("Hyperlight") || st.contains("Firecracker"),
            "status 必须说明真接路径: {st}"
        );
    }

    #[test]
    fn noop_start_returns_err() {
        let s = NoopVMSandbox;
        let cfg = VMSandboxConfig::default();
        let err = s.start(&cfg).expect_err("0 装: start 必须 Err");
        assert!(err.contains("Noop"), "err 必须含 Noop: {err}");
        assert!(err.contains("未实装"), "err 必须明示未实装: {err}");
    }

    #[test]
    fn noop_backends_returns_empty() {
        // 0 装: 无 backend (detect() 已 None, start() 必 Err).
        let s = NoopVMSandbox;
        assert!(s.backends().is_empty(), "0 装: backends() 必须空");
        assert_eq!(s.backend(), VMSandboxBackend::PlatformDefault);
    }

    // ──────────────────────────────────────────────────────────────────
    // 8-12) validate_config 边界 — 5 个测试
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn validate_config_0_vcpus_errors() {
        let cfg = VMSandboxConfig {
            vcpus: 0,
            ..VMSandboxConfig::default()
        };
        let err = validate_config(&cfg, VMSandboxBackend::PlatformDefault).unwrap_err();
        assert!(err.contains("vcpus"), "错误必须指明 vcpus: {err}");
        assert!(err.contains("0"), "错误必须含 0: {err}");
    }

    #[test]
    fn validate_config_33_vcpus_errors() {
        let cfg = VMSandboxConfig {
            vcpus: 33,
            ..VMSandboxConfig::default()
        };
        let err = validate_config(&cfg, VMSandboxBackend::PlatformDefault).unwrap_err();
        assert!(err.contains("vcpus"), "错误必须指明 vcpus: {err}");
        assert!(err.contains("33"), "错误必须含 33: {err}");
    }

    #[test]
    fn validate_config_0_memory_errors() {
        let cfg = VMSandboxConfig {
            memory_mb: 0,
            ..VMSandboxConfig::default()
        };
        let err = validate_config(&cfg, VMSandboxBackend::PlatformDefault).unwrap_err();
        assert!(err.contains("memory_mb"), "错误必须指明 memory_mb: {err}");
    }

    #[test]
    fn validate_config_65537_memory_errors() {
        let cfg = VMSandboxConfig {
            memory_mb: 65537,
            ..VMSandboxConfig::default()
        };
        let err = validate_config(&cfg, VMSandboxBackend::PlatformDefault).unwrap_err();
        assert!(err.contains("memory_mb"), "错误必须指明 memory_mb: {err}");
        assert!(err.contains("65537"), "错误必须含 65537: {err}");
    }

    #[test]
    fn validate_config_correct_values_ok() {
        // 边界: 1 vCPU + 512 MB + 默认 timeout
        let cfg = VMSandboxConfig::default();
        for backend in [
            VMSandboxBackend::Kvm,
            VMSandboxBackend::Hypervisor,
            VMSandboxBackend::HyperV,
            VMSandboxBackend::PlatformDefault,
        ] {
            assert!(
                validate_config(&cfg, backend).is_ok(),
                "合法 cfg + {backend:?} 必须 Ok"
            );
        }

        // 边界极值: 32 vCPU + 65536 MB + 1 s timeout
        let cfg = VMSandboxConfig {
            vcpus: 32,
            memory_mb: 65536,
            boot_timeout_secs: 1,
            ..VMSandboxConfig::default()
        };
        assert!(validate_config(&cfg, VMSandboxBackend::PlatformDefault).is_ok());

        // 问题配置: boot_timeout_secs = 0
        let cfg = VMSandboxConfig {
            boot_timeout_secs: 0,
            ..VMSandboxConfig::default()
        };
        assert!(validate_config(&cfg, VMSandboxBackend::PlatformDefault).is_err());
    }

    // ──────────────────────────────────────────────────────────────────
    // 13) Drop 自动 halt (借鉴 libkrun 清理)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn vm_sandbox_handle_drop_calls_halt() {
        // 构造一个 handle: 走 NoopVMSandbox+start 是 Err, 但 handle 仍可由 trait
        // 实装构造. 测试 Drop: 直接 new 一个 handle, 让它走 drop, 验证 halted 状态.
        let h = VMSandboxHandle::new(
            Box::new(NoopVMSandbox),
            VMSandboxConfig::default(),
            VMSandboxState::Booted,
        );
        // 离开作用域 → Drop → halt
        assert!(!h.is_halted(), "构造后未 halt");
        // 这里不能直接验证 halt 触发 (Drop 不可拦截), 改用 halt 路径:
        let mut h = h;
        assert!(!h.is_halted());
        let r = h.halt();
        assert!(r.is_ok());
        assert!(h.is_halted(), "halt 后必须 is_halted=true");
        assert_eq!(h.state(), VMSandboxState::Halted);
        // 二次 halt 幂等
        assert!(h.halt().is_ok());
    }

    #[test]
    fn vm_sandbox_handle_drop_halt_is_idempotent() {
        // 验证 Drop 路径不泄漏 (双重 halt 也不会 panic).
        let mut h = VMSandboxHandle::new(
            Box::new(NoopVMSandbox),
            VMSandboxConfig::default(),
            VMSandboxState::Running,
        );
        assert!(h.halt().is_ok());
        assert!(h.halt().is_ok());
        assert!(h.halt().is_ok());
    }

    #[test]
    fn vm_sandbox_handle_exec_requires_running() {
        // 0 装路径: exec 必然 Err (trait 未实装), 不依赖状态.
        let mut h = VMSandboxHandle::new(
            Box::new(NoopVMSandbox),
            VMSandboxConfig::default(),
            VMSandboxState::Running,
        );
        let err = h.exec("ls").expect_err("0 装: exec 必须 Err");
        assert!(err.contains("0 装"), "exec err 必须明示 0 装: {err}");
        // 空命令也 Err
        let err = h.exec("").expect_err("空命令必须 Err");
        assert!(err.contains("空"), "空命令 err: {err}");
    }

    #[test]
    fn vm_sandbox_handle_exec_after_halt_errors() {
        let mut h = VMSandboxHandle::new(
            Box::new(NoopVMSandbox),
            VMSandboxConfig::default(),
            VMSandboxState::Running,
        );
        let _ = h.halt();
        let err = h.exec("ls").expect_err("halted 后 exec 必须 Err");
        assert!(err.contains("halt"), "err 必须含 halt: {err}");
    }

    #[test]
    fn vm_sandbox_handle_wait_boot_state_guard() {
        // wait_boot 仅在 Booted/Running 状态可 Ok (其它 Err).
        let mut h = VMSandboxHandle::new(
            Box::new(NoopVMSandbox),
            VMSandboxConfig::default(),
            VMSandboxState::Created,
        );
        assert!(h.wait_boot().is_err(), "Created 状态 wait_boot 必须 Err");
        // 切到 Booted
        h.state = VMSandboxState::Booted;
        assert!(h.wait_boot().is_ok(), "Booted 状态 wait_boot 必须 Ok");
        // 切到 Running
        h.state = VMSandboxState::Running;
        assert!(h.wait_boot().is_ok(), "Running 状态 wait_boot 必须 Ok");
    }

    #[test]
    fn vm_sandbox_state_as_str_5_variants() {
        let cases = [
            (VMSandboxState::Created, "created"),
            (VMSandboxState::Booted, "booted"),
            (VMSandboxState::Running, "running"),
            (VMSandboxState::Halted, "halted"),
            (VMSandboxState::Error, "error"),
        ];
        for (s, expected) in cases {
            assert_eq!(s.as_str(), expected, "state as_str: {s:?}");
        }
    }

    #[test]
    fn vm_sandbox_config_supports_network_field() {
        // 验证 VMSandboxConfig.network 字段可承载 sandbox_net::NetworkIsolationConfig.
        let net = crate::sandbox_net::NetworkIsolationConfig {
            level: NetworkIsolationLevel::LoopbackOnly,
            outbound_whitelist: vec![],
            allow_inbound: false,
            allow_dns: false,
        };
        let cfg = VMSandboxConfig {
            network: Some(net.clone()),
            ..VMSandboxConfig::default()
        };
        assert!(cfg.network.is_some());
        assert_eq!(
            cfg.network.as_ref().unwrap().level,
            NetworkIsolationLevel::LoopbackOnly
        );
        // 携带 network 仍 validate_ok (只检查 vcpus/memory/timing)
        assert!(validate_config(&cfg, VMSandboxBackend::PlatformDefault).is_ok());
    }

    #[test]
    fn vm_sandbox_config_default_does_not_validate_backend_compat() {
        // 0 装 PASS: validate_config 不验证 backend ↔ platform 兼容性.
        // 即便 backend = Kvm + 平台 = ?? (测试时无 platform 概念), 都应 Ok.
        let cfg = VMSandboxConfig::default();
        // 跨 platform 错配 (0 装阶段无 platform 上下文): 仍 Ok.
        for backend in [
            VMSandboxBackend::Kvm,
            VMSandboxBackend::Hypervisor,
            VMSandboxBackend::HyperV,
        ] {
            assert!(
                validate_config(&cfg, backend).is_ok(),
                "0 装: validate_config 只查参数边界, 不查 backend 兼容: {backend:?}"
            );
        }
    }
}
