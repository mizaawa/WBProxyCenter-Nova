use crate::{
    error::{ApiError, Args},
    App,
};
use axum::{
    extract::{multipart::MultipartRejection, Multipart, Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use wbpc_store::{accounts::Filter, uploads::Receipt};
fn invalid() -> ApiError {
    ApiError::input("需要 multipart 的 document 文件字段")
}
pub(crate) async fn accept(
    State(a): State<App>,
    Args(f): Args<Filter>,
    input: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let mut data = input.map_err(|_| invalid())?;
    let realm = f.realm.filter(|r| r != "auto" && !r.is_empty());
    if realm
        .as_deref()
        .is_some_and(|r| !matches!(r, "cn" | "intl"))
    {
        return Err(ApiError::input("号池无效"));
    }
    let mut batch = a.uploads.reserve().await?;
    let mut bytes = 0_u64;
    while let Some(mut field) = data.next_field().await.map_err(|_| invalid())? {
        if !matches!(field.name(), Some("document" | "file" | "documents")) {
            return Err(invalid());
        }
        let mut out = batch.next_file().await?;
        while let Some(part) = field.chunk().await.map_err(|_| invalid())? {
            bytes += part.len() as u64;
            if bytes > 256 * 1024 * 1024 {
                return Err(ApiError(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "invalid_request_error",
                    "每批文件总量最多 256 MiB",
                ));
            }
            out.write_all(&part)
                .await
                .map_err(|_| ApiError::service())?;
        }
        out.sync_all().await.map_err(|_| ApiError::service())?;
    }
    let id = a.uploads.submit(batch, realm).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"batch_id":id,"id":id}))))
}
pub(crate) async fn status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Receipt>, ApiError> {
    a.uploads.receipt(&id).await?.map(Json).ok_or(ApiError(
        StatusCode::NOT_FOUND,
        "not_found",
        "找不到此批次或回执已过期",
    ))
}
