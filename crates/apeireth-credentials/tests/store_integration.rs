//! Integration tests for apeireth-credentials (post-1.0.0 增量)
//!
//! src/ 各 mod tests 已覆盖. 这里 (tests/) 加 per-行为样板.
//! 0 触碰 src/, 0 编造"已实现"。

#![allow(missing_docs)]

use apeireth_credentials::secret::SecretString;
use apeireth_credentials::{
    validate_service_name, CredentialsError, CredentialsStore, FileCredentialsStore,
};

fn tmp_path(name: &str) -> std::path::PathBuf {
    // 用 pid + name 隔离, 0 真凭据, 0 假数据
    let dir = std::env::temp_dir().join(format!(
        "apeireth-creds-int-{}-{}-{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.join("creds.json")
}

fn fake_value() -> String {
    // 0 真凭据, 0 真 key 模式 (避开 GH scanner)
    format!("FAKE-VALUE-{}", std::process::id())
}

// =============================================================================
// 1. validate_service_name 边界
// =============================================================================

#[test]
fn validate_service_name_accepts_normal() {
    assert!(validate_service_name("service-a").is_ok());
    assert!(validate_service_name("svc_1").is_ok());
    assert!(validate_service_name("api.server").is_ok());
    assert!(validate_service_name("my-service_1.0").is_ok());
}

#[test]
fn validate_service_name_rejects_empty() {
    assert!(validate_service_name("").is_err());
}

#[test]
fn validate_service_name_rejects_too_long() {
    let long = "a".repeat(129);
    assert!(validate_service_name(&long).is_err());
    let max = "a".repeat(128);
    assert!(validate_service_name(&max).is_ok());
}

#[test]
fn validate_service_name_rejects_dot_only() {
    assert!(validate_service_name(".").is_err());
    assert!(validate_service_name("..").is_err());
}

#[test]
fn validate_service_name_rejects_leading_dot() {
    assert!(validate_service_name(".hidden").is_err());
    assert!(validate_service_name(".ssh").is_err());
}

#[test]
fn validate_service_name_rejects_path_separator() {
    assert!(validate_service_name("path/traversal").is_err());
    assert!(validate_service_name("path\\traversal").is_err());
    // "/" → is_ascii_alphanumeric false → 拒
    assert!(validate_service_name("path/to").is_err());
    // "\\" → is_ascii_alphanumeric false → 拒
    assert!(validate_service_name("path\\to").is_err());
}

#[test]
fn validate_service_name_rejects_special_chars() {
    assert!(validate_service_name("svc!").is_err());
    assert!(validate_service_name("svc@home").is_err());
    assert!(validate_service_name("svc#1").is_err());
    assert!(validate_service_name("svc$").is_err());
    assert!(validate_service_name("svc%").is_err());
    assert!(validate_service_name("svc&").is_err());
    assert!(validate_service_name("svc*").is_err());
    assert!(validate_service_name("svc|").is_err());
    assert!(validate_service_name("svc<test>").is_err());
}

#[test]
fn validate_service_name_rejects_whitespace() {
    assert!(validate_service_name("svc with space").is_err());
    assert!(validate_service_name("svc\ttab").is_err());
    assert!(validate_service_name("svc\nnewline").is_err());
}

#[test]
fn validate_service_name_accepts_max_length() {
    let max = "a".repeat(128);
    assert!(validate_service_name(&max).is_ok());
}

// =============================================================================
// 2. FileCredentialsStore 基本 CRUD
// =============================================================================

#[test]
fn file_store_get_unknown_returns_error() {
    let p = tmp_path("get-unknown");
    let store = FileCredentialsStore::new(&p).unwrap();
    let r = store.get("nonexistent-service");
    assert!(matches!(r, Err(CredentialsError::UnknownService(_))));
}

#[test]
fn file_store_set_then_get_round_trip() {
    let p = tmp_path("set-get");
    let store = FileCredentialsStore::new(&p).unwrap();
    let val = fake_value();
    store.set("svc-1", SecretString::new(val.clone())).unwrap();
    let got = store.get("svc-1").unwrap();
    assert_eq!(got.expose(), val);
}

#[test]
fn file_store_set_overwrites_existing() {
    let p = tmp_path("overwrite");
    let store = FileCredentialsStore::new(&p).unwrap();
    let v1 = format!("FAKE-A-{}", std::process::id());
    let v2 = format!("FAKE-B-{}", std::process::id());
    store.set("svc", SecretString::new(v1.clone())).unwrap();
    store.set("svc", SecretString::new(v2.clone())).unwrap();
    let got = store.get("svc").unwrap();
    assert_eq!(got.expose(), v2);
    assert_ne!(got.expose(), v1);
}

#[test]
fn file_store_set_multiple_services() {
    let p = tmp_path("multi");
    let store = FileCredentialsStore::new(&p).unwrap();
    store.set("svc-a", SecretString::new(fake_value())).unwrap();
    store.set("svc-b", SecretString::new(fake_value())).unwrap();
    store.set("svc-c", SecretString::new(fake_value())).unwrap();
    assert!(store.contains("svc-a").unwrap());
    assert!(store.contains("svc-b").unwrap());
    assert!(store.contains("svc-c").unwrap());
}

#[test]
fn file_store_delete_existing_returns_ok() {
    let p = tmp_path("delete");
    let store = FileCredentialsStore::new(&p).unwrap();
    store.set("svc", SecretString::new(fake_value())).unwrap();
    assert!(store.delete("svc").is_ok());
    assert!(!store.contains("svc").unwrap());
}

#[test]
fn file_store_delete_unknown_returns_error() {
    let p = tmp_path("delete-unknown");
    let store = FileCredentialsStore::new(&p).unwrap();
    assert!(matches!(
        store.delete("nonexistent"),
        Err(CredentialsError::UnknownService(_))
    ));
}

#[test]
fn file_store_list_empty_returns_empty_vec() {
    let p = tmp_path("list-empty");
    let store = FileCredentialsStore::new(&p).unwrap();
    let list = store.list().unwrap();
    assert!(list.is_empty());
}

#[test]
fn file_store_list_returns_all_services() {
    let p = tmp_path("list-all");
    let store = FileCredentialsStore::new(&p).unwrap();
    store.set("alpha", SecretString::new(fake_value())).unwrap();
    store.set("beta", SecretString::new(fake_value())).unwrap();
    store.set("gamma", SecretString::new(fake_value())).unwrap();
    let mut list = store.list().unwrap();
    list.sort();
    assert_eq!(list, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn file_store_contains_returns_correct_state() {
    let p = tmp_path("contains");
    let store = FileCredentialsStore::new(&p).unwrap();
    assert!(!store.contains("missing").unwrap());
    store
        .set("present", SecretString::new(fake_value()))
        .unwrap();
    assert!(store.contains("present").unwrap());
}

#[test]
fn file_store_path_returns_storage_path() {
    let p = tmp_path("path");
    let store = FileCredentialsStore::new(&p).unwrap();
    assert_eq!(store.path(), p.as_path());
}

#[test]
fn file_store_persists_across_instances() {
    let p = tmp_path("persist");
    let val = fake_value();
    {
        let store = FileCredentialsStore::new(&p).unwrap();
        store.set("svc", SecretString::new(val.clone())).unwrap();
    }
    // 新实例应能读到
    let store2 = FileCredentialsStore::new(&p).unwrap();
    let got = store2.get("svc").unwrap();
    assert_eq!(got.expose(), val);
}

// =============================================================================
// 3. FileCredentialsStore 验证错误处理
// =============================================================================

#[test]
fn file_store_set_invalid_name_returns_error() {
    let p = tmp_path("invalid-name");
    let store = FileCredentialsStore::new(&p).unwrap();
    let r = store.set("", SecretString::new(fake_value()));
    assert!(matches!(r, Err(CredentialsError::InvalidServiceName(_))));
    let r = store.set(".", SecretString::new(fake_value()));
    assert!(matches!(r, Err(CredentialsError::InvalidServiceName(_))));
    let r = store.set("path/traversal", SecretString::new(fake_value()));
    assert!(matches!(r, Err(CredentialsError::InvalidServiceName(_))));
}

#[test]
fn file_store_get_invalid_name_returns_error() {
    let p = tmp_path("get-invalid");
    let store = FileCredentialsStore::new(&p).unwrap();
    let r = store.get("");
    assert!(matches!(r, Err(CredentialsError::InvalidServiceName(_))));
    let r = store.get("../escape");
    assert!(matches!(r, Err(CredentialsError::InvalidServiceName(_))));
}

#[test]
fn file_store_delete_invalid_name_returns_error() {
    let p = tmp_path("del-invalid");
    let store = FileCredentialsStore::new(&p).unwrap();
    let r = store.delete("path/with/slash");
    assert!(matches!(r, Err(CredentialsError::InvalidServiceName(_))));
}

#[test]
fn file_store_new_creates_parent_directory() {
    let parent = tmp_path("parent-dir").parent().unwrap().to_path_buf();
    let nested = parent.join(format!(
        "nested-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let creds_path = nested.join("deeper").join("creds.json");
    assert!(!nested.exists());
    let _store = FileCredentialsStore::new(&creds_path).unwrap();
    assert!(nested.exists());
    assert!(creds_path.parent().unwrap().exists());
}

// =============================================================================
// 4. CredentialsError Display
// =============================================================================

#[test]
fn credentials_error_displays_distinctly() {
    let errors = vec![
        CredentialsError::UnknownService("svc".into()),
        CredentialsError::InvalidServiceName("svc!@#".into()),
        CredentialsError::Io {
            service: "svc".into(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "fake"),
        },
        CredentialsError::Format {
            service: "svc".into(),
            message: "bad json".into(),
        },
    ];
    let displays: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    let unique: std::collections::HashSet<&String> = displays.iter().collect();
    assert_eq!(unique.len(), displays.len(), "4 variant Display 互不相同");
}

#[test]
fn credentials_error_specific_messages() {
    assert!(CredentialsError::UnknownService("alpha".into())
        .to_string()
        .contains("alpha"));
    assert!(CredentialsError::InvalidServiceName("bad!".into())
        .to_string()
        .contains("bad!"));
}

// =============================================================================
// 5. SecretString 集成 (from secret.rs)
// =============================================================================

#[test]
fn secret_string_new_preserves_value() {
    let v = "my-fake-secret-001";
    let s = SecretString::new(v.to_string());
    assert_eq!(s.expose(), v);
}

#[test]
fn secret_string_empty() {
    let s = SecretString::new(String::new());
    assert_eq!(s.expose(), "");
}

#[test]
fn secret_string_clone_independent() {
    let s1 = SecretString::new("value-1".to_string());
    let s2 = s1.clone();
    assert_eq!(s1.expose(), s2.expose());
}
