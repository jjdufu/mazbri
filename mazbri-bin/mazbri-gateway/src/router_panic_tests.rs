//! 请求级 panic 隔离层的测试：处理链处于可恢复范围，panic 转换为 500 JSON。

use super::isolate_request_panics;
use crate::gateway::metrics::GATEWAY_PANIC_TOTAL;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use mazbri_core::panic_policy::in_recoverable_scope;
use tower::ServiceExt;

async fn scope_probe() -> String {
    in_recoverable_scope().to_string()
}

async fn exploding_handler() -> &'static str {
    panic!("synthetic gateway handler panic");
}

fn app() -> Router {
    isolate_request_panics(
        Router::new()
            .route("/probe", get(scope_probe))
            .route("/boom", get(exploding_handler)),
    )
}

async fn call(path: &str) -> (StatusCode, String) {
    let response = app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn handlers_run_inside_recoverable_scope() {
    assert!(!in_recoverable_scope());
    let (status, body) = call("/probe").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "true");
    assert!(!in_recoverable_scope());
}

#[tokio::test]
async fn handler_panic_becomes_500_json_and_is_counted() {
    let before = GATEWAY_PANIC_TOTAL.get();
    let (status, body) = call("/boom").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json.is_object(),
        "panic response must be a JSON envelope: {body}"
    );
    assert!(
        !body.contains("synthetic gateway handler panic"),
        "panic message must not leak to clients"
    );
    assert!(GATEWAY_PANIC_TOTAL.get() > before);
    // 隔离层在 unwind 后恢复深度计数，不会把可恢复标记泄漏到当前线程。
    assert!(!in_recoverable_scope());
}

#[test]
fn build_router_applies_isolation_outside_base_middleware() {
    let source = include_str!("router.rs");
    let base = source
        .find("let base_middleware = ServiceBuilder::new()")
        .unwrap();
    let base_end = base + source[base..].find(';').unwrap();
    assert!(
        !source[base..base_end].contains("CatchPanicLayer"),
        "CatchPanicLayer must only be applied by isolate_request_panics"
    );
    assert!(source.contains(
        ".layer(base_middleware);\n    Ok(isolate_request_panics(router).with_state(app_state))"
    ));
    let helper = source.find("fn isolate_request_panics").unwrap();
    let marker = helper
        + source[helper..]
            .find("recoverable_request_middleware")
            .unwrap();
    let catcher = helper
        + source[helper..]
            .find("CatchPanicLayer::custom(handle_panic)")
            .unwrap();
    assert!(
        marker < catcher,
        "marker layer must sit directly inside CatchPanicLayer"
    );
}

#[test]
fn release_profile_unwinds_so_catch_panic_is_effective() {
    // panic = "abort" 会让 CatchPanicLayer 成为死代码，任何请求 panic 都直接杀进程。
    let manifest = include_str!("../../Cargo.toml");
    let release = &manifest[manifest.find("[profile.release]").unwrap()..];
    let release = release[1..]
        .find("\n[")
        .map_or(release, |end| &release[..=end]);
    assert!(release.contains("panic = \"unwind\""), "{release}");
    assert!(!release.contains("panic = \"abort\""));
}
