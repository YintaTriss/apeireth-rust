//! Integration tests for apeireth-environment (post-1.0.0 增量)
//!
//! src/lib.rs mod tests 已覆盖 13 cases (mod tests) + 1 Kani proof.
//! 这里 (tests/) 加 per-行为样板: 配置 builder + BackendKind 分类 + BackendRegistry.
//! 0 触碰 src/, 0 编造 "已实现"。

#![allow(missing_docs)]

use apeireth_environment::{
    BackendKind, BackendRegistry, DaytonaBackend, DaytonaConfig, DockerBackend, DockerConfig,
    EnvironmentError, ModalBackend, ModalConfig, SingularityBackend, SingularityConfig, SshConfig,
    TerminalBackend,
};

// =============================================================================
// 1. BackendKind 6 variant 端到端
// =============================================================================

#[test]
fn backend_kind_all_returns_6() {
    assert_eq!(BackendKind::ALL.len(), 6, "6 种 terminal backend");
}

#[test]
fn backend_kind_as_str_6_distinct() {
    let strs: Vec<&str> = BackendKind::ALL.iter().map(|k| k.as_str()).collect();
    let unique: std::collections::HashSet<&str> = strs.iter().copied().collect();
    assert_eq!(unique.len(), 6, "6 个 as_str 互不相同");
}

#[test]
fn backend_kind_as_str_returns_lowercase() {
    for kind in BackendKind::ALL {
        let s = kind.as_str();
        assert!(
            s.chars().all(|c| c.is_ascii_lowercase()),
            "as_str 应小写: {kind:?} -> {s}"
        );
    }
}

// =============================================================================
// 2. BackendKind is_* 分类
// =============================================================================

#[test]
fn backend_kind_is_local_only_local() {
    assert!(BackendKind::Local.is_local());
    assert!(!BackendKind::Docker.is_local());
    assert!(!BackendKind::Ssh.is_local());
    assert!(!BackendKind::Daytona.is_local());
    assert!(!BackendKind::Modal.is_local());
    assert!(!BackendKind::Singularity.is_local());
}

#[test]
fn backend_kind_is_container() {
    assert!(BackendKind::Docker.is_container());
    assert!(BackendKind::Singularity.is_container());
    // 其他 4 个不是 container
    assert!(!BackendKind::Local.is_container());
    assert!(!BackendKind::Ssh.is_container());
    assert!(!BackendKind::Daytona.is_container());
    assert!(!BackendKind::Modal.is_container());
}

#[test]
fn backend_kind_is_remote() {
    assert!(BackendKind::Ssh.is_remote());
    assert!(BackendKind::Daytona.is_remote());
    assert!(BackendKind::Modal.is_remote());
    // 其他 3 个不是 remote
    assert!(!BackendKind::Local.is_remote());
    assert!(!BackendKind::Docker.is_remote());
    assert!(!BackendKind::Singularity.is_remote());
}

#[test]
fn backend_kind_classification_6() {
    // 互斥且穷尽: 6 backend 各属一个类别
    for kind in BackendKind::ALL {
        let count =
            (kind.is_local() as u32) + (kind.is_container() as u32) + (kind.is_remote() as u32);
        assert_eq!(count, 1, "每个 kind 恰属 1 类别: {kind:?}");
    }
}

// =============================================================================
// 3. ExecRequest builder 端到端
// =============================================================================

#[test]
fn exec_request_new_uses_sensible_defaults() {
    let r = apeireth_environment::ExecRequest::new("ls");
    assert_eq!(r.command, "ls");
    assert!(r.working_dir.is_none());
    assert!(r.env.is_empty());
    assert_eq!(r.timeout_secs, 30, "默认 30s");
    assert!(r.stdin.is_none());
}

#[test]
fn exec_request_with_cwd() {
    let r = apeireth_environment::ExecRequest::new("ls").with_cwd("/tmp");
    assert_eq!(r.working_dir, Some("/tmp".to_string()));
}

#[test]
fn exec_request_with_env_single_pair() {
    let r = apeireth_environment::ExecRequest::new("env").with_env("FAKE_KEY", "FAKE_VALUE");
    assert_eq!(
        r.env,
        vec![("FAKE_KEY".to_string(), "FAKE_VALUE".to_string())]
    );
}

#[test]
fn exec_request_with_env_multiple_pairs() {
    let r = apeireth_environment::ExecRequest::new("env")
        .with_env("FAKE_A", "v1")
        .with_env("FAKE_B", "v2")
        .with_env("FAKE_C", "v3");
    assert_eq!(r.env.len(), 3, "应 3 对 env");
}

#[test]
fn exec_request_with_timeout() {
    let r = apeireth_environment::ExecRequest::new("ls").with_timeout(60);
    assert_eq!(r.timeout_secs, 60);
}

#[test]
fn exec_request_builder_chain() {
    let r = apeireth_environment::ExecRequest::new("ls")
        .with_cwd("/home")
        .with_env("X", "1")
        .with_env("Y", "2")
        .with_timeout(120);
    assert_eq!(r.command, "ls");
    assert_eq!(r.working_dir, Some("/home".to_string()));
    assert_eq!(r.env.len(), 2);
    assert_eq!(r.timeout_secs, 120);
}

// =============================================================================
// 4. DockerConfig builder
// =============================================================================

#[test]
fn docker_config_new_uses_sensible_defaults() {
    let c = DockerConfig::new("ubuntu:22.04");
    assert_eq!(c.image, "ubuntu:22.04");
    assert!(c.container_name.is_none());
    assert!(c.volumes.is_empty());
    assert!(c.network.is_none());
    assert!(c.memory.is_none());
    assert!(c.cpus.is_none());
}

#[test]
fn docker_config_with_volume() {
    let c = DockerConfig::new("ubuntu:22.04").with_volume("/host/data", "/data");
    assert_eq!(c.volumes.len(), 1);
    assert_eq!(
        c.volumes[0],
        ("/host/data".to_string(), "/data".to_string())
    );
}

#[test]
fn docker_config_with_multiple_volumes() {
    let c = DockerConfig::new("ubuntu:22.04")
        .with_volume("/h1", "/c1")
        .with_volume("/h2", "/c2")
        .with_volume("/h3", "/c3");
    assert_eq!(c.volumes.len(), 3);
}

// =============================================================================
// 5. SshConfig builder
// =============================================================================

#[test]
fn ssh_config_new_uses_sensible_defaults() {
    let c = SshConfig::new("server.example.com", 22, "user");
    assert_eq!(c.host, "server.example.com");
    assert_eq!(c.port, 22);
    assert_eq!(c.user, "user");
    assert!(c.key_path.is_none());
    assert!(c.password.is_none());
}

#[test]
fn ssh_config_with_key() {
    let c = SshConfig::new("server.com", 22, "user").with_key("/fake/path/key");
    assert_eq!(c.key_path, Some("/fake/path/key".to_string()));
}

#[test]
fn ssh_config_custom_port() {
    let c = SshConfig::new("server.com", 2222, "user");
    assert_eq!(c.port, 2222);
}

// =============================================================================
// 6. DaytonaConfig / ModalConfig / SingularityConfig
// =============================================================================

#[test]
fn daytona_config_required_fields() {
    let c = DaytonaConfig {
        api_url: "https://api.daytona.io".to_string(),
        api_key: "fake-key".to_string(),
    };
    assert!(!c.api_url.is_empty());
    assert!(!c.api_key.is_empty());
}

#[test]
fn modal_config_required_fields() {
    let c = ModalConfig {
        api_url: "https://api.modal.com".to_string(),
        api_key: "fake-key".to_string(),
    };
    assert!(!c.api_url.is_empty());
    assert!(!c.api_key.is_empty());
}

#[test]
fn singularity_config_defaults() {
    let c = SingularityConfig {
        image: "ubuntu.sif".to_string(),
        singularity_path: None,
    };
    assert_eq!(c.image, "ubuntu.sif");
    assert!(c.singularity_path.is_none());
}

#[test]
fn singularity_config_with_path() {
    let c = SingularityConfig {
        image: "ubuntu.sif".to_string(),
        singularity_path: Some("/usr/bin/singularity".to_string()),
    };
    assert_eq!(c.singularity_path, Some("/usr/bin/singularity".to_string()));
}

// =============================================================================
// 7. BackendRegistry 端到端
// =============================================================================

#[test]
fn backend_registry_default_is_empty() {
    let r = BackendRegistry::default();
    assert_eq!(r.kinds().len(), 0, "默认 registry 应空");
}

#[test]
fn backend_registry_new_is_empty() {
    let r = BackendRegistry::new();
    assert_eq!(r.kinds().len(), 0);
}

#[test]
fn backend_registry_with_local_has_local() {
    let r = BackendRegistry::with_local();
    assert!(r.get(BackendKind::Local).is_some());
    assert!(!r.get(BackendKind::Docker).is_some());
    assert_eq!(r.kinds(), vec![BackendKind::Local]);
}

#[test]
fn backend_registry_register_returns_self() {
    let r = BackendRegistry::new().register(Box::new(apeireth_environment::LocalBackend));
    assert_eq!(r.kinds().len(), 1);
}

#[test]
fn backend_registry_register_multiple_backends() {
    let r = BackendRegistry::new()
        .register(Box::new(apeireth_environment::LocalBackend))
        .register(Box::new(DockerBackend::new(DockerConfig::new(
            "ubuntu:22.04",
        ))));
    assert_eq!(r.kinds().len(), 2);
    assert!(r.get(BackendKind::Local).is_some());
    assert!(r.get(BackendKind::Docker).is_some());
}

#[test]
fn backend_registry_get_returns_correct_backend() {
    let r = BackendRegistry::new()
        .register(Box::new(apeireth_environment::LocalBackend))
        .register(Box::new(DockerBackend::new(DockerConfig::new("alpine:3"))));
    let local = r.get(BackendKind::Local).expect("Local 应注册");
    assert_eq!(local.kind(), BackendKind::Local);
    let docker = r.get(BackendKind::Docker).expect("Docker 应注册");
    assert_eq!(docker.kind(), BackendKind::Docker);
}

#[test]
fn backend_registry_get_missing_returns_none() {
    let r = BackendRegistry::new();
    assert!(r.get(BackendKind::Local).is_none());
    assert!(r.get(BackendKind::Docker).is_none());
    assert!(r.get(BackendKind::Ssh).is_none());
}

#[test]
fn backend_registry_register_replaces() {
    let r = BackendRegistry::new()
        .register(Box::new(apeireth_environment::LocalBackend))
        .register(Box::new(DockerBackend::new(DockerConfig::new("img1"))))
        .register(Box::new(DockerBackend::new(DockerConfig::new("img2"))));
    // 同 kind 重复 register 覆盖
    assert_eq!(r.kinds().len(), 2);
    let docker = r.get(BackendKind::Docker).unwrap();
    assert_eq!(docker.name(), "docker"); // 后端 name
                                         // Docker 覆盖后, 第二次 register 的 config 不易直接测 (无 getter), 仅 kinds 数
}

#[test]
fn backend_registry_kinds_returns_all_registered() {
    let r = BackendRegistry::new()
        .register(Box::new(apeireth_environment::LocalBackend))
        .register(Box::new(DockerBackend::new(DockerConfig::new("img"))))
        .register(Box::new(SingularityBackend::new(SingularityConfig {
            image: "img.sif".to_string(),
            singularity_path: None,
        })));
    let kinds = r.kinds();
    assert_eq!(kinds.len(), 3);
    assert!(kinds.contains(&BackendKind::Local));
    assert!(kinds.contains(&BackendKind::Docker));
    assert!(kinds.contains(&BackendKind::Singularity));
}

// =============================================================================
// 8. EnvironmentError 6 variant Display
// =============================================================================

#[test]
fn env_error_displays_distinctly() {
    let errors = vec![
        EnvironmentError::LocalFailed("x".into()),
        EnvironmentError::DockerUnavailable("x".into()),
        EnvironmentError::DockerFailed("x".into()),
        EnvironmentError::SshUnavailable("x".into()),
        EnvironmentError::SshFailed("x".into()),
        EnvironmentError::DaytonaUnconfigured("x".into()),
        EnvironmentError::ModalUnconfigured("x".into()),
        EnvironmentError::SingularityUnconfigured("x".into()),
        EnvironmentError::Timeout(60),
        EnvironmentError::Denied("x".into()),
    ];
    let displays: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    let unique: std::collections::HashSet<&String> = displays.iter().collect();
    assert_eq!(unique.len(), displays.len(), "10 variant Display 互不相同");
}

#[test]
fn env_error_specific_messages() {
    assert!(EnvironmentError::Timeout(30).to_string().contains("30"));
    assert!(EnvironmentError::LocalFailed("disk full".into())
        .to_string()
        .contains("disk full"));
    assert!(EnvironmentError::DockerUnavailable("no docker".into())
        .to_string()
        .contains("no docker"));
}

// =============================================================================
// 9. 集成: 真实 LocalBackend execute 端到端
// =============================================================================

#[tokio::test]
async fn local_backend_executes_echo() {
    // 跨平台 echo (Windows cmd /c, Unix 避免 -c 歧义用 printf)
    let backend = apeireth_environment::LocalBackend;
    let cmd = if cfg!(target_os = "windows") {
        "cmd /c echo hello-apeireth"
    } else {
        "printf hello-apeireth"
    };
    let req = apeireth_environment::ExecRequest::new(cmd).with_timeout(5);
    let result = backend.execute(&req).await;
    let r = match result {
        Ok(r) => r,
        Err(e) => panic!("应能执行, 实际 Err: {e}"),
    };
    if cfg!(target_os = "windows") {
        // Windows cmd /c echo 可能 normalize 引号, 只 check exit code
        assert_eq!(r.exit_code, 0, "cmd /c echo 应 exit 0");
    } else {
        // Unix 严格 check stdout
        assert_eq!(r.exit_code, 0);
        assert!(
            r.stdout.contains("hello-apeireth"),
            "应含 stdout: {:?}",
            r.stdout
        );
    }
    assert_eq!(r.backend, "local");
}

#[tokio::test]
async fn local_backend_executes_invalid_command() {
    let backend = apeireth_environment::LocalBackend;
    let req =
        apeireth_environment::ExecRequest::new("definitely-not-a-real-command-xyz").with_timeout(5);
    let result = backend.execute(&req).await;
    assert!(result.is_err(), "无效命令应返 Err");
}

#[tokio::test]
async fn local_backend_availability_is_true() {
    let backend = apeireth_environment::LocalBackend;
    assert!(backend.availability().await);
}

// =============================================================================
// 10. 集成: 6 backend execute 各路径
// =============================================================================

#[tokio::test]
async fn daytona_backend_execute_returns_unconfigured_error() {
    let backend = DaytonaBackend::new(DaytonaConfig {
        api_url: "https://api.daytona.io".to_string(),
        api_key: "fake".to_string(),
    });
    let req = apeireth_environment::ExecRequest::new("echo hi");
    let result = backend.execute(&req).await;
    assert!(matches!(
        result,
        Err(EnvironmentError::DaytonaUnconfigured(_))
    ));
}

#[tokio::test]
async fn daytona_backend_availability_is_false() {
    let backend = DaytonaBackend::new(DaytonaConfig {
        api_url: "https://api.daytona.io".to_string(),
        api_key: "fake".to_string(),
    });
    assert!(!backend.availability().await, "stub backend 应 unavailable");
}

#[tokio::test]
async fn modal_backend_execute_returns_unconfigured_error() {
    let backend = ModalBackend::new(ModalConfig {
        api_url: "https://api.modal.com".to_string(),
        api_key: "fake".to_string(),
    });
    let req = apeireth_environment::ExecRequest::new("echo hi");
    let result = backend.execute(&req).await;
    assert!(matches!(
        result,
        Err(EnvironmentError::ModalUnconfigured(_))
    ));
}

#[tokio::test]
async fn modal_backend_availability_is_false() {
    let backend = ModalBackend::new(ModalConfig {
        api_url: "https://api.modal.com".to_string(),
        api_key: "fake".to_string(),
    });
    assert!(!backend.availability().await);
}
