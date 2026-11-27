use crate::{
    error::{ApiError, Input},
    App, VERSION,
};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, uri::Authority, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};
fn cookie(h: &HeaderMap) -> &str {
    h.get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|s| s.trim().strip_prefix("wbp_session="))
        })
        .unwrap_or("")
}
fn authority(h: &HeaderMap) -> Option<Authority> {
    h.get(header::HOST)?.to_str().ok()?.parse().ok()
}
pub(crate) async fn origin(
    State(app): State<App>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if req.method() != Method::GET
        && req.method() != Method::HEAD
        && req.method() != Method::OPTIONS
    {
        let host = authority(req.headers()).ok_or_else(ApiError::forbidden)?;
        let scheme = if app.secure { "https" } else { "http" };
        let expected = url::Url::parse(&format!("{scheme}://{host}"))
            .map_err(|_| ApiError::forbidden())?
            .origin()
            .ascii_serialization();
        let actual = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if actual != Some(expected.as_str()) {
            return Err(ApiError::forbidden());
        }
    }
    Ok(next.run(req).await)
}
pub(crate) async fn require(
    State(app): State<App>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if !app.sessions.is_valid(cookie(req.headers())).await? {
        return Err(ApiError::auth());
    }
    Ok(next.run(req).await)
}
fn set_cookie(app: &App, secret: &str, live: bool) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(&format!(
        "wbp_session={secret}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        if live { 86400 } else { 0 },
        if app.secure { "; Secure" } else { "" }
    ))
    .map_err(|_| ApiError::service())
}
async fn success(app: &App, ip: IpAddr) -> Result<Response, ApiError> {
    let token = app.sessions.issue(ip).await?;
    let mut r = Json(json!({"authenticated":true})).into_response();
    r.headers_mut()
        .insert(header::SET_COOKIE, set_cookie(app, &token, true)?);
    Ok(r)
}
pub(crate) async fn status(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "authenticated": a.sessions.is_valid(cookie(&h)).await?,
        "needs_setup": !a.sessions.configured().await?,
        "setup_code_required": false, // Kept for status clients predating the two-field form.
        "version": VERSION,
        "site_name": "WBProxy Center"
    })))
}
#[derive(Deserialize)]
pub(crate) struct Password {
    password: String,
}
#[derive(Deserialize)]
pub(crate) struct SetupPassword {
    password: String,
    password_confirm: String,
}
pub(crate) async fn setup(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Input(p): Input<SetupPassword>,
) -> Result<Response, ApiError> {
    if a.sessions.configured().await? {
        return Err(wbpc_store::Error::Conflict.into());
    }
    if !a.sessions.reserve_attempt(peer.ip()).await? {
        return Err(ApiError::limited());
    }
    if p.password != p.password_confirm {
        return Err(ApiError::input("两次输入的管理员密钥不一致"));
    }
    a.sessions.initialize(p.password).await?;
    success(&a, peer.ip()).await
}
pub(crate) async fn login(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Input(p): Input<Password>,
) -> Result<Response, ApiError> {
    if !a.sessions.reserve_attempt(peer.ip()).await? {
        return Err(ApiError::limited());
    }
    if !a.sessions.verify(p.password).await? {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "管理员密钥错误",
        ));
    }
    success(&a, peer.ip()).await
}
pub(crate) async fn logout(State(a): State<App>, h: HeaderMap) -> Result<Response, ApiError> {
    a.sessions.revoke(cookie(&h)).await?;
    let mut r = Json(json!({"authenticated":false})).into_response();
    r.headers_mut()
        .insert(header::SET_COOKIE, set_cookie(&a, "", false)?);
    Ok(r)
}
