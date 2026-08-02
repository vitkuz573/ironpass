use axum::body::Body;
use axum::http::{Request, StatusCode};
use ironpass_api::db::DbPool;
use ironpass_api::models::{AddSubscriptionRequest, StartProxyRequest};
use ironpass_api::state::AppState;
use ironpass_config::ConfigManager;
use ironpass_core::traits::HwidProvider;
use std::sync::Arc;
use tower::ServiceExt;

struct MockHwidProvider;

impl HwidProvider for MockHwidProvider {
    fn generate(&self) -> ironpass_core::Result<String> {
        Ok("mock-hwid".into())
    }

    fn get_device_info(&self) -> ironpass_core::Result<ironpass_core::models::HwidInfo> {
        Ok(ironpass_core::models::HwidInfo {
            hwid: "mock-hwid".into(),
            device_model: "Mock".into(),
            os: "Linux".into(),
            hostname: "mock".into(),
            username: "mock".into(),
            machine_id: "mock".into(),
        })
    }
}

fn test_state() -> Arc<AppState> {
    let config_manager = ConfigManager::with_dirs(
        std::env::temp_dir().join("ironpass-test-cfg"),
        std::env::temp_dir().join("ironpass-test-data"),
    );
    let db = DbPool::open_in_memory().unwrap();
    let hwid: Arc<dyn HwidProvider + Send + Sync> = Arc::new(MockHwidProvider);
    Arc::new(AppState::new(config_manager, db, hwid))
}

fn app(state: Arc<AppState>) -> axum::Router {
    ironpass_api::app(state)
}

#[tokio::test]
async fn health_returns_ok() {
    let state = test_state();
    let response = app(state)
        .oneshot(Request::builder().uri("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn config_round_trip() {
    let state = test_state();
    let router = app(state);

    let get = router
        .clone()
        .oneshot(Request::builder().uri("/api/v1/config").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);

    let default_config = ironpass_config::AppConfig::default();
    let body = serde_json::to_string(&ironpass_api::models::ConfigResponse { config: default_config }).unwrap();
    let put = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
}

#[tokio::test]
async fn subscription_crud() {
    let state = test_state();
    let router = app(state);

    let add_body = serde_json::to_string(&AddSubscriptionRequest {
        url: "https://example.com/sub".into(),
        name: Some("test".into()),
        hwid: None,
    })
    .unwrap();

    let add = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/subscriptions")
                .header("content-type", "application/json")
                .body(Body::from(add_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::OK);

    let list = router
        .clone()
        .oneshot(Request::builder().uri("/api/v1/subscriptions").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(list.into_body(), usize::MAX).await.unwrap();
    let subs: Vec<ironpass_api::models::StoredSubscription> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(subs.len(), 1);
    let id = subs[0].id;

    let get = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/subscriptions/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);

    let delete = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/subscriptions/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
}

#[tokio::test]
async fn proxy_status_without_node_returns_ok() {
    let state = test_state();
    let response = app(state)
        .oneshot(Request::builder().uri("/api/v1/proxy/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn start_proxy_without_selection_is_bad_request() {
    let state = test_state();
    let router = app(state);
    let body = serde_json::to_string(&StartProxyRequest {
        node_id: None,
        socks_port: Some(11080),
        http_port: Some(11080),
        mixed_port: None,
    })
    .unwrap();
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy/start")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
