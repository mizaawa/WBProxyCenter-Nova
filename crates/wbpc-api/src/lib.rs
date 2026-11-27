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
mod accounts;
mod assets;
mod auth;
mod error;
mod export;
use axum::{extract::DefaultBodyLimit, routing::post};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use wbpc_store::{accounts::Vault, session::Sessions};
#[derive(Clone)]
/// 共享的状态只暴露领域仓储；handler 不直接操作 Redis。
pub struct App {
    db: Runtime,
    vault: Vault,
    sessions: Sessions,
    setup_code: Arc<String>,
    secure: bool,
    staging: Arc<PathBuf>,
}
impl App {
    /// 日志读取一次性验证码后，只能用它完成首次管理员初始化。
    pub fn new(
        db: Runtime,
        vault: Vault,
        sessions: Sessions,
        setup_code: String,
        secure: bool,
        staging: PathBuf,
    ) -> Self {
        Self {
            db,
            vault,
            sessions,
            setup_code: Arc::new(setup_code),
            secure,
            staging: Arc::new(staging),
        }
    }
}
use wbpc_store::Runtime;
/// 所有客户端和二进制从同一 workspace 版本派生。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
async fn health(State(a): State<App>) -> Response {
    let ready = a.db.ping().await.is_ok();
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
    h.insert(header::CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
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
pub fn router(a: App) -> Router {
    let secured = Router::new()
        .route("/api/accounts", get(accounts::list).post(accounts::import))
        .route("/api/accounts/import", post(accounts::import))
        .route("/api/accounts/export", post(export::download))
        .route(
            "/api/accounts/{id}",
            get(accounts::detail)
                .patch(accounts::patch)
                .delete(accounts::delete),
        )
        .route("/api/auth/logout", post(auth::logout))
        .route_layer(middleware::from_fn_with_state(a.clone(), auth::require));
    let management = Router::new()
        .merge(secured)
        .route("/api/auth/status", get(auth::status))
        .route("/api/auth/setup", post(auth::setup))
        .route("/api/auth/login", post(auth::login))
        .route_layer(middleware::from_fn_with_state(a.clone(), auth::origin));
    Router::new()
        .merge(management)
        .route("/health", get(health))
        .route("/api/version", get(version))
        .fallback(get(assets::serve))
        .with_state(a)
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(middleware::from_fn(headers))
}
