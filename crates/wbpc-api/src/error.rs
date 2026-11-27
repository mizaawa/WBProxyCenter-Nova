use axum::{
    extract::{FromRequest, FromRequestParts, Query, Request},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use wbpc_store::Error;
pub(crate) struct ApiError(pub StatusCode, pub &'static str, pub &'static str);
impl ApiError {
    pub fn auth() -> Self {
        Self(StatusCode::UNAUTHORIZED, "authentication_error", "请先登录")
    }
    pub fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "forbidden", "拒绝跨站请求")
    }
    pub fn input(msg: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, "invalid_request_error", msg)
    }
    pub fn limited() -> Self {
        Self(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit_error",
            "尝试过多，请五分钟后重试",
        )
    }
    pub fn service() -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "请求未完成，请检查服务状态并重试",
        )
    }
}
impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        match e {
            Error::Invalid(s) => Self::input(s),
            Error::Busy => Self(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                "上传队列已满，请稍后重试",
            ),
            Error::NotFound => Self(StatusCode::NOT_FOUND, "not_found", "找不到此账号"),
            Error::Conflict => Self(StatusCode::CONFLICT, "conflict", "数据已变化，请刷新后重试"),
            _ => {
                tracing::warn!("仓储操作未完成");
                Self::service()
            }
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":{"type":self.1,"message":self.2},"message":self.2})),
        )
            .into_response()
    }
}
pub(crate) struct Input<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Input<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(v)| Self(v))
            .map_err(|e| {
                ApiError(
                    e.status(),
                    "invalid_request_error",
                    "请求 JSON 格式、大小或字段无效",
                )
            })
    }
}
pub(crate) struct Args<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Args<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(v)| Self(v))
            .map_err(|_| ApiError::input("查询参数无效"))
    }
}
