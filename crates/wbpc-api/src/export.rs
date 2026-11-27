use super::{
    error::{ApiError, Input},
    App,
};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{io, path::Path};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    task,
};
use wbpc_store::accounts::Filter;
#[derive(Deserialize)]
pub(crate) struct Selection {
    ids: Option<Vec<String>>,
    #[serde(flatten)]
    filter: Filter,
}
const PART: usize = 6 * 1024 * 1024;
async fn start(dir: &Path, n: usize) -> io::Result<BufWriter<fs::File>> {
    let mut f = BufWriter::new(fs::File::create(dir.join(format!("part-{n}.json"))).await?);
    f.write_all(br#"{"accounts":["#).await?;
    Ok(f)
}
async fn end(mut f: BufWriter<fs::File>) -> io::Result<()> {
    f.write_all(b"]}").await?;
    f.flush().await
}
fn fail<T>(_: T) -> ApiError {
    ApiError::service()
}
pub(crate) async fn download(
    State(a): State<App>,
    Input(sel): Input<Selection>,
) -> Result<Response, ApiError> {
    let ids = match sel.ids {
        Some(xs) if xs.is_empty() => Vec::new(),
        xs => {
            a.vault
                .export_ids(xs.as_deref().unwrap_or(&[]), &sel.filter)
                .await?
        }
    };
    let base = a.staging.clone();
    let tmp = task::spawn_blocking(move || {
        tempfile::Builder::new()
            .prefix("export-")
            .tempdir_in(base.as_ref())
    })
    .await
    .map_err(fail)?
    .map_err(fail)?;
    let mut n = 1;
    let mut len = 14;
    let mut used = false;
    let mut f = start(tmp.path(), n).await.map_err(fail)?;
    for id in ids {
        let data = serde_json::to_vec(&a.vault.export_one(&id).await?).map_err(fail)?;
        if used && len + data.len() + 3 > PART {
            end(f).await.map_err(fail)?;
            n += 1;
            len = 14;
            used = false;
            f = start(tmp.path(), n).await.map_err(fail)?;
        }
        if used {
            f.write_all(b",").await.map_err(fail)?;
            len += 1;
        }
        f.write_all(&data).await.map_err(fail)?;
        len += data.len();
        used = true;
    }
    end(f).await.map_err(fail)?;
    let (path, name, mime) = if n == 1 {
        (
            tmp.path().join("part-1.json"),
            "accounts.json",
            "application/json; charset=utf-8",
        )
    } else {
        let dir = tmp.path().to_owned();
        let file = task::spawn_blocking(move || -> Result<_, io::Error> {
            let path = dir.join("accounts.zip");
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&path)?);
            for i in 1..=n {
                let item = format!("part-{i}.json");
                writer.start_file(
                    &item,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )?;
                io::copy(&mut std::fs::File::open(dir.join(item))?, &mut writer)?;
            }
            writer.finish()?;
            Ok(path)
        })
        .await
        .map_err(fail)?
        .map_err(fail)?;
        (file, "accounts.zip", "application/zip")
    };
    let file = fs::File::open(path).await.map_err(fail)?;
    let stream = futures::stream::try_unfold((file, tmp), |(mut file, tmp)| async move {
        let mut buf = vec![0; 65536];
        let n = file.read(&mut buf).await?;
        buf.truncate(n);
        Ok::<_, io::Error>((n > 0).then(|| (Bytes::from(buf), (file, tmp))))
    });
    let mut r = Body::from_stream(stream).into_response();
    r.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    r.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(r#"attachment; filename="{name}""#)).map_err(fail)?,
    );
    Ok(r)
}
