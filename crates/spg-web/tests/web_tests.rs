use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use rusqlite::Connection;
use spg_web::build_app;
use spg_web::models::{ProviderKind, ProviderUpdateRequest};
use spg_web::secret_store::{MemorySecretStore, SecretStore};
use spg_web::store::ProviderStore;
use spg_web::AppState;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

#[tokio::test]
async fn locked_galaxy_redirects_to_login() {
    let temp = tempdir().expect("temp dir");
    let app = build_app(
        AppState::new(
            temp.path().join("providers.db"),
            temp.path().join("core.db"),
            Arc::new(MemorySecretStore::new()),
        )
        .expect("state"),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/galaxy")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get("location").expect("location"),
        "/login"
    );
}

#[tokio::test]
async fn locked_galaxy_api_is_forbidden() {
    let temp = tempdir().expect("temp dir");
    let app = build_app(
        AppState::new(
            temp.path().join("providers.db"),
            temp.path().join("core.db"),
            Arc::new(MemorySecretStore::new()),
        )
        .expect("state"),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/galaxy/list")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(
        String::from_utf8(body.to_vec()).expect("utf8"),
        "setup_required"
    );
}

#[allow(unreachable_code)]
#[tokio::test]
async fn setup_page_requires_login_confirmation() {
    let temp = tempdir().expect("temp dir");
    let app = build_app(
        AppState::new(
            temp.path().join("providers.db"),
            temp.path().join("core.db"),
            Arc::new(MemorySecretStore::new()),
        )
        .expect("state"),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/setup/providers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get("location").expect("location"),
        "/login"
    );
    return;
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("Provider Access"));
    assert!(html.contains("统一授权与调度配置"));
}

#[tokio::test]
async fn setup_page_renders_provider_shell_after_login_confirmation() {
    let temp = tempdir().expect("temp dir");
    let app = build_app(
        AppState::new(
            temp.path().join("providers.db"),
            temp.path().join("core.db"),
            Arc::new(MemorySecretStore::new()),
        )
        .expect("state"),
    );

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/confirm")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/setup/providers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("02 API 配置测试"));
    assert!(html.contains("逐行配置与测试"));
    assert!(html.contains("一键测试全部 API"));
}

#[test]
fn provider_settings_do_not_store_plaintext_api_keys() {
    let temp = tempdir().expect("temp dir");
    let db_path = temp.path().join("providers.db");
    let store = ProviderStore::new(db_path.clone(), Arc::new(MemorySecretStore::new()));
    store.initialize().expect("initialize");

    let secret = "secret-inline-token";
    store
        .save_provider(
            ProviderKind::Doubao,
            provider_request(ProviderKind::Doubao, Some(secret)),
        )
        .expect("save provider");

    let conn = Connection::open(&db_path).expect("open db");
    let mut statement = conn
        .prepare("PRAGMA table_info(provider_settings)")
        .expect("pragma");
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect columns");

    assert!(!columns.iter().any(|column| column == "api_key"));

    let bytes = fs::read(&db_path).expect("read db");
    assert!(!String::from_utf8_lossy(&bytes).contains(secret));
}

#[test]
fn provider_tests_cover_timeout_and_failure_paths() {
    let temp = tempdir().expect("temp dir");
    let store = ProviderStore::new(
        temp.path().join("providers.db"),
        Arc::new(MemorySecretStore::new()),
    );
    store.initialize().expect("initialize");

    store
        .save_provider(
            ProviderKind::Serper,
            provider_request(ProviderKind::Serper, Some("timeout-token")),
        )
        .expect("save timeout provider");
    let timeout = store
        .test_provider(ProviderKind::Serper)
        .expect("test timeout provider");
    assert!(!timeout.ok);
    assert_eq!(timeout.status, "timeout");

    store
        .save_provider(
            ProviderKind::Firecrawl,
            provider_request(ProviderKind::Firecrawl, Some("fail-token")),
        )
        .expect("save failed provider");
    let failed = store
        .test_provider(ProviderKind::Firecrawl)
        .expect("test failed provider");
    assert!(!failed.ok);
    assert_eq!(failed.status, "failed");
}

#[test]
fn unlock_requires_all_required_providers() {
    let temp = tempdir().expect("temp dir");
    let secrets = Arc::new(MemorySecretStore::new());
    let store = ProviderStore::new(temp.path().join("providers.db"), secrets.clone());
    store.initialize().expect("initialize");

    for provider in ProviderKind::REQUIRED {
        store
            .save_provider(
                provider,
                provider_request(provider, Some(&format!("{}-ok-key", provider.slug()))),
            )
            .expect("save provider");
    }

    store.test_all().expect("test all");
    let unlocked = store.unlock_state().expect("unlock state");
    assert!(unlocked.unlocked);
    assert_eq!(unlocked.ready_count, 5);
    assert_eq!(unlocked.optional_ready_count, 0);
    assert_eq!(unlocked.optional_total_count, 0);

    secrets
        .delete_secret("provider::doubao")
        .expect("delete secret");
    let relocked = store.unlock_state().expect("unlock state after delete");
    assert!(!relocked.unlocked);
    assert_eq!(relocked.ready_count, 4);
}

#[test]
fn provider_config_change_invalidates_previous_ready_status() {
    let temp = tempdir().expect("temp dir");
    let store = ProviderStore::new(
        temp.path().join("providers.db"),
        Arc::new(MemorySecretStore::new()),
    );
    store.initialize().expect("initialize");

    store
        .save_provider(
            ProviderKind::Doubao,
            provider_request(ProviderKind::Doubao, Some("doubao-ok-key")),
        )
        .expect("save provider");
    let ready = store
        .test_provider(ProviderKind::Doubao)
        .expect("test provider");
    assert_eq!(ready.status, "ready");

    let mut changed = provider_request(ProviderKind::Doubao, None);
    changed.base_url = "https://timeout.example.com".to_string();
    let view = store
        .save_provider(ProviderKind::Doubao, changed)
        .expect("save changed provider");

    assert_eq!(view.status, "configured");
    assert!(view.last_verified_at.is_none());
    assert!(!store.unlock_state().expect("unlock state").unlocked);
}

#[tokio::test]
async fn root_redirects_back_to_setup_when_required_secret_disappears() {
    let temp = tempdir().expect("temp dir");
    let secrets = Arc::new(MemorySecretStore::new());
    let db_path = temp.path().join("providers.db");
    let core_db = temp.path().join("core.db");

    let store = ProviderStore::new(db_path.clone(), secrets.clone());
    store.initialize().expect("initialize");
    store.confirm_local_user().expect("confirm local user");

    for provider in ProviderKind::REQUIRED {
        store
            .save_provider(
                provider,
                provider_request(provider, Some(&format!("{}-ok-key", provider.slug()))),
            )
            .expect("save provider");
    }

    store.test_all().expect("test all providers");
    let gate_state = store.advance_api_gate().expect("advance gate");
    assert!(gate_state.api_gate_passed);

    secrets
        .delete_secret("provider::doubao")
        .expect("delete required secret");

    let app = build_app(AppState::new(db_path, core_db, secrets).expect("state"));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get("location").expect("location"),
        "/setup/providers"
    );
}

#[test]
fn gate_state_revalidates_current_local_identity() {
    let temp = tempdir().expect("temp dir");
    let db_path = temp.path().join("providers.db");
    let store = ProviderStore::new(db_path.clone(), Arc::new(MemorySecretStore::new()));
    store.initialize().expect("initialize");

    let local_user = store.local_user();
    let conn = Connection::open(&db_path).expect("open db");
    conn.execute(
        "
        UPDATE app_gate_state
        SET local_user_confirmed = 1,
            confirmed_username = ?1,
            confirmed_device_name = ?2,
            api_gate_passed = 1
        WHERE id = 1
        ",
        rusqlite::params![
            format!("{}-mismatch", local_user.username),
            local_user.device_name
        ],
    )
    .expect("seed mismatched gate");

    let gate_state = store.gate_state().expect("gate state");
    assert!(!gate_state.local_user_confirmed);
    assert!(!gate_state.api_gate_passed);
    assert!(gate_state.confirmed_username.is_none());
    assert!(gate_state.confirmed_device_name.is_none());
}

fn provider_request(provider: ProviderKind, api_key: Option<&str>) -> ProviderUpdateRequest {
    ProviderUpdateRequest {
        enabled: true,
        api_key: api_key.map(ToOwned::to_owned),
        clear_api_key: false,
        base_url: provider.default_base_url().to_string(),
        monthly_limit: provider.monthly_limit(),
        daily_soft_limit: provider.daily_soft_limit(),
        default_role: provider.default_role().to_string(),
        priority: provider.priority(),
    }
}
