use crate::{Error, Runtime};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{net::IpAddr, sync::Arc};
use tokio::{sync::Semaphore, task};
/// 密码操作占用阻塞线程；信号量约束不同来源的并发成本。
#[derive(Clone)]
pub struct Sessions {
    db: Runtime,
    workers: Arc<Semaphore>,
}
fn random_secret() -> String {
    let mut b = [0_u8; 32];
    OsRng.fill_bytes(&mut b);
    hex::encode(b)
}
fn hash(v: &str) -> String {
    hex::encode(Sha256::digest(v.as_bytes()))
}
impl Sessions {
    /// 保持账户文件域和会话域独立。
    pub fn new(db: Runtime) -> Self {
        Self {
            db,
            workers: Arc::new(Semaphore::new(4)),
        }
    }
    /// 一次性设置码仅在无管理员时由服务进程生成并显示在私有日志。
    pub fn setup_code() -> String {
        random_secret()
    }
    /// 管理员哈希与其他设置共享 hash，但不进入状态接口。
    pub async fn configured(&self) -> Result<bool, Error> {
        Ok(redis::cmd("HEXISTS")
            .arg(self.db.key("settings"))
            .arg("password_hash")
            .query_async(&mut self.db.conn.clone())
            .await?)
    }
    /// 只允许首个初始化者提交，重启后不覆盖管理员密码。
    pub async fn initialize(&self, password: String) -> Result<(), Error> {
        if password.chars().count() < 10 || password.len() > 72 {
            return Err(Error::Invalid("密码至少 10 个字符，最多 72 字节"));
        }
        let _permit = self
            .workers
            .acquire()
            .await
            .map_err(|_| Error::Invalid("服务正在停止"))?;
        let value =
            task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST)).await??;
        let v = serde_json::to_string(&value)?;
        let created:bool=redis::Script::new("if redis.call('HSETNX',KEYS[1],'password_hash',ARGV[1]) == 1 then redis.call('INCR',KEYS[2]); return 1 end; return 0").key(self.db.key("settings")).key(self.db.key("settings:rev")).arg(v).invoke_async(&mut self.db.conn.clone()).await?;
        if !created {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    /// 未完成的验证也占用计数，避免同时抵达的请求绕过失败上限。
    pub async fn reserve_attempt(&self, ip: IpAddr) -> Result<bool, Error> {
        Ok(redis::Script::new("local n=redis.call('INCR',KEYS[1]); if n==1 then redis.call('EXPIRE',KEYS[1],300) end; if n>10 then return 0 end; return 1").key(self.db.key(&format!("login:attempt:{ip}"))).invoke_async(&mut self.db.conn.clone()).await?)
    }
    /// 非法凭证与未设置密码均不泄露认证细节。
    pub async fn verify(&self, password: String) -> Result<bool, Error> {
        if password.len() > 72 {
            return Ok(false);
        }
        let raw: Option<String> = redis::cmd("HGET")
            .arg(self.db.key("settings"))
            .arg("password_hash")
            .query_async(&mut self.db.conn.clone())
            .await?;
        let Some(raw) = raw else {
            return Ok(false);
        };
        let digest: String = serde_json::from_str(&raw)?;
        let _permit = self
            .workers
            .acquire()
            .await
            .map_err(|_| Error::Invalid("服务正在停止"))?;
        Ok(task::spawn_blocking(move || bcrypt::verify(password, &digest)).await??)
    }
    /// 只有客户端保留随机 cookie 原文。Redis 只存摘要和过期时间。
    pub async fn issue(&self, ip: IpAddr) -> Result<String, Error> {
        let secret = random_secret();
        redis::pipe()
            .atomic()
            .cmd("SET")
            .arg(self.db.key(&format!("session:{}", hash(&secret))))
            .arg(1)
            .arg("EX")
            .arg(86400)
            .ignore()
            .cmd("DEL")
            .arg(self.db.key(&format!("login:attempt:{ip}")))
            .ignore()
            .query_async::<()>(&mut self.db.conn.clone())
            .await?;
        Ok(secret)
    }
    /// 会话的存活性由 Redis 的 TTL 决定。
    pub async fn is_valid(&self, token: &str) -> Result<bool, Error> {
        if token.len() != 64 || !token.bytes().all(|x| x.is_ascii_hexdigit()) {
            return Ok(false);
        }
        Ok(redis::cmd("EXISTS")
            .arg(self.db.key(&format!("session:{}", hash(token))))
            .query_async(&mut self.db.conn.clone())
            .await?)
    }
    /// 主动退出删除服务端凭据，使复制过的 cookie 也同时失效。
    pub async fn revoke(&self, token: &str) -> Result<(), Error> {
        redis::cmd("DEL")
            .arg(self.db.key(&format!("session:{}", hash(token))))
            .query_async::<()>(&mut self.db.conn.clone())
            .await?;
        Ok(())
    }
}
