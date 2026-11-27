use crate::Error;
use std::path::Path;
use tokio::fs;
/// 仅初始化明确指定的应用私有目录；拒绝将符号链接当作凭证目录。
pub async fn private_dir(path: &Path) -> Result<(), Error> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path).await?;
    let meta = fs::symlink_metadata(path).await?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Error::Invalid("数据目录必须是真实目录"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}
