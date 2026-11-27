use crate::{account_format::validate, Error};
use fs2::FileExt;
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use tokio::{fs, io::AsyncReadExt, task};
pub(crate) const FILE_MAX: u64 = 8 * 1024 * 1024;
pub(crate) async fn read(path: &Path) -> Result<Value, Error> {
    let m = fs::symlink_metadata(path).await?;
    if !m.is_file() || m.len() > FILE_MAX {
        return Err(Error::Invalid("账号文件无效或超过 8 MiB"));
    }
    let mut buf = Vec::with_capacity(m.len() as usize);
    fs::File::open(path)
        .await?
        .take(FILE_MAX + 1)
        .read_to_end(&mut buf)
        .await?;
    if buf.len() as u64 > FILE_MAX {
        return Err(Error::Invalid("账号文件过大"));
    }
    let v: Value = serde_json::from_slice(&buf)?;
    validate(&v)?;
    Ok(v)
}
fn lock(path: &Path) -> Result<File, Error> {
    let lp = path.with_extension("json.lock");
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    if std::fs::symlink_metadata(&lp).is_ok_and(|m| !m.is_file()) {
        return Err(Error::Conflict);
    }
    let f = opts.open(lp)?;
    FileExt::lock_exclusive(&f)?;
    Ok(f)
}
fn parent(path: &Path) -> Result<&Path, Error> {
    path.parent().ok_or(Error::Invalid("账号路径无效"))
}
fn sync_dir(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
/// 调用者从索引取得路径，预期旧值的比较在文件锁内进行，失败绝不更新索引。
pub(crate) async fn replace(
    path: PathBuf,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<(), Error> {
    task::spawn_blocking(move || {
        let _guard = lock(&path)?;
        match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() && m.len() <= FILE_MAX => {
                let old: Value = serde_json::from_reader(File::open(&path)?)?;
                if before.as_ref() != Some(&old) {
                    return Err(Error::Conflict);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && before.is_none() => {}
            _ => return Err(Error::Conflict),
        }
        let dir = parent(&path)?;
        if let Some(value) = after {
            let bytes = serde_json::to_vec_pretty(&value)?;
            if bytes.len() as u64 + 1 > FILE_MAX {
                return Err(Error::Invalid("账号文件过大"));
            }
            let mut tmp = tempfile::Builder::new()
                .prefix(".account-")
                .suffix(".tmp")
                .tempfile_in(dir)?;
            tmp.write_all(&bytes)?;
            tmp.write_all(b"\n")?;
            tmp.as_file().sync_all()?;
            tmp.persist(&path).map_err(|e| e.error)?;
        } else {
            std::fs::remove_file(&path)?;
        }
        sync_dir(dir)
    })
    .await?
}
