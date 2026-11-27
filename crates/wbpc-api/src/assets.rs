use crate::error::ApiError;
use axum::{
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
};
pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path();
    #[cfg(feature = "web-ui")]
    {
        use axum::http::header;
        static WEB: include_dir::Dir<'_> =
            include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../web/dist");
        let asset = if path == "/" {
            "index.html"
        } else if let Some(relative) = path.strip_prefix("/assets/") {
            if relative.is_empty() || relative.contains('/') || relative.contains("..") {
                ""
            } else {
                &path[1..]
            }
        } else {
            ""
        };
        if let Some(f) = WEB.get_file(asset) {
            let mime = if asset.ends_with(".js") {
                "text/javascript; charset=utf-8"
            } else if asset.ends_with(".css") {
                "text/css; charset=utf-8"
            } else if asset.ends_with(".woff2") {
                "font/woff2"
            } else {
                "text/html; charset=utf-8"
            };
            return ([(header::CONTENT_TYPE, mime)], f.contents()).into_response();
        }
    }
    #[cfg(not(feature = "web-ui"))]
    if path == "/" {
        return ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "web_unavailable",
            "该构建未包含控制台",
        )
        .into_response();
    }
    ApiError(StatusCode::NOT_FOUND, "not_found", "找不到这个地址").into_response()
}
