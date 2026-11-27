use super::{
    error::{ApiError, Args, Input},
    App,
};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use wbpc_store::accounts::{Filter, Page};
pub(crate) async fn list(State(a): State<App>, Args(f): Args<Filter>) -> Json<Page> {
    Json(a.vault.list(&f).await)
}
pub(crate) async fn import(
    State(a): State<App>,
    Args(f): Args<Filter>,
    Input(raw): Input<Value>,
) -> Result<Json<Value>, ApiError> {
    let ids = a.vault.import(raw, f.realm.as_deref()).await?;
    Ok(Json(json!({"imported":ids.len(),"ids":ids})))
}
pub(crate) async fn detail(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(a.vault.detail(&id).await?))
}
pub(crate) async fn patch(
    State(a): State<App>,
    Path(id): Path<String>,
    Input(v): Input<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(a.vault.patch(&id, v).await?))
}
pub(crate) async fn delete(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    a.vault.delete(&id).await?;
    Ok(Json(json!({"deleted":1})))
}
