use crate::Error;
use chrono::Utc;
use redis::{aio::ConnectionManager, Client, RedisResult};
use std::time::Duration;
use ulid::Ulid;
/// 共享连接管理器；所有 Redis 操作都在仓储内定义。
#[derive(Clone)]
pub struct Runtime {
    pub(crate) conn: ConnectionManager,
    pub(crate) prefix: String,
}
impl Runtime {
    /// 在接受后台流量前确认 Redis 可用；连接失败时不能退化为临时内存账号库。
    pub async fn connect(url: &str) -> Result<Self, Error> {
        Self::open(url, "wbp:").await
    }
    pub(crate) async fn open(url: &str, prefix: &str) -> Result<Self, Error> {
        let client = Client::open(url)?;
        let cfg = redis::aio::ConnectionManagerConfig::new()
            .set_number_of_retries(2)
            .set_connection_timeout(Duration::from_secs(3))
            .set_response_timeout(Duration::from_secs(3));
        let conn = client.get_connection_manager_with_config(cfg).await?;
        Ok(Self {
            conn,
            prefix: prefix.into(),
        })
    }
    pub(crate) fn key(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.prefix)
    }
    /// readiness 走一次真实 PING，不能返回缓存的上线状态。
    pub async fn ping(&self) -> Result<(), Error> {
        let mut c = self.conn.clone();
        let reply: String = redis::cmd("PING").query_async(&mut c).await?;
        if reply != "PONG" {
            return Err(Error::Schema);
        }
        Ok(())
    }
    /// 先检查版本，再发布本次进程元信息；绝不重写较新版本的数据。
    pub async fn initialize(&self, version: &str, addr: &str) -> Result<(), Error> {
        let mut c = self.conn.clone();
        let info: String = redis::cmd("INFO").arg("server").query_async(&mut c).await?;
        let major = info
            .lines()
            .find_map(|s| s.strip_prefix("redis_version:"))
            .and_then(|v| v.split('.').next())
            .and_then(|v| v.parse::<u32>().ok());
        if major.is_none_or(|v| v < 7) {
            return Err(Error::Schema);
        }
        let schema = self.key("meta:schema");
        let _: Option<String> = redis::cmd("SET")
            .arg(&schema)
            .arg(1)
            .arg("NX")
            .query_async(&mut c)
            .await?;
        let actual: RedisResult<i64> = redis::cmd("GET").arg(&schema).query_async(&mut c).await;
        if actual? != 1 {
            return Err(Error::Schema);
        }
        let now = Utc::now();
        redis::pipe()
            .atomic()
            .cmd("SET")
            .arg(self.key("meta:boot"))
            .arg(Ulid::new().to_string())
            .ignore()
            .cmd("HSET")
            .arg(self.key("meta:instance"))
            .arg("version")
            .arg(version)
            .arg("started_at")
            .arg(now.timestamp_millis())
            .arg("started_iso")
            .arg(now.to_rfc3339())
            .arg("addr")
            .arg(addr)
            .ignore()
            .query_async::<()>(&mut c)
            .await?;
        Ok(())
    }
}
