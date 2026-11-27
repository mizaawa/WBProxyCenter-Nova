#![forbid(unsafe_code)]
//! 管理面与网关 HTTP 接口。
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use wbpc_store::Runtime;
/// 所有客户端和二进制从同一 workspace 版本派生。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
async fn health(State(db): State<Runtime>) -> Response {
    let ready = db.ping().await.is_ok();
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({"status":if ready{"ok"}else{"unavailable"},"version":VERSION})),
    )
        .into_response()
}
async fn version() -> Json<serde_json::Value> {
    Json(json!({"version":VERSION}))
}
async fn headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if h.get(header::CONTENT_TYPE) == Some(&HeaderValue::from_static("application/json")) {
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
    }
    res
}
/// 只有显式注册的路径可访问，绝不将项目目录作为静态站点提供。
pub fn router(db: Runtime) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/version", get(version))
        .with_state(db)
        .layer(middleware::from_fn(headers))
}
