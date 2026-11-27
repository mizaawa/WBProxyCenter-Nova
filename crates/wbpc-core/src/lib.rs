#![forbid(unsafe_code)]
//! 共享配置与领域类型。
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use thiserror::Error;
use url::Url;

/// 启动错误仅包含变量名，不回显密码或连接串。
#[derive(Debug, Error)]
#[error("配置项 {0} 无效")]
pub struct ConfigError(&'static str);

/// 服务所需配置；故意不实现 Debug，避免连接串进入日志。
pub struct Config {
    pub addr: SocketAddr,
    pub access_dir: PathBuf,
    pub data_dir: PathBuf,
    pub redis_url: Url,
    pub log: String,
    pub admin_password: Option<String>,
    pub secure_cookie: bool,
}
impl Config {
    /// 相对默认值始终基于程序目录，而不是服务的工作目录。
    pub fn from_pairs<'a>(
        pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
        home: &Path,
    ) -> Result<Self, ConfigError> {
        let env: HashMap<_, _> = pairs.into_iter().collect();
        let get = |key| env.get(key).copied().filter(|s| !s.is_empty());
        let addr = get("WBP_ADDR")
            .unwrap_or("127.0.0.1:3040")
            .parse()
            .map_err(|_| ConfigError("WBP_ADDR"))?;
        let mut redis_url = Url::parse(get("WBP_REDIS_URL").unwrap_or("redis://127.0.0.1:6379/0"))
            .map_err(|_| ConfigError("WBP_REDIS_URL"))?;
        if redis_url.scheme() != "redis" || redis_url.host().is_none() {
            return Err(ConfigError("WBP_REDIS_URL"));
        }
        if let Some(pw) = get("WBP_REDIS_PASSWORD") {
            redis_url
                .set_password(Some(pw))
                .map_err(|_| ConfigError("WBP_REDIS_PASSWORD"))?;
        }
        let access_dir = get("WBP_ACCESS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("access_account"));
        let data_dir = get("WBP_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("data"));
        let secure_cookie = match get("WBP_SECURE_COOKIE") {
            None | Some("0") => false,
            Some("1") => true,
            _ => return Err(ConfigError("WBP_SECURE_COOKIE")),
        };
        Ok(Self {
            addr,
            access_dir,
            data_dir,
            redis_url,
            log: get("WBP_LOG").unwrap_or("info").to_owned(),
            admin_password: get("WBP_ADMIN_PASSWORD").map(str::to_owned),
            secure_cookie,
        })
    }
}
