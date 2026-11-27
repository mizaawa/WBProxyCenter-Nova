use thiserror::Error;
/// 存储错误只在服务端保留诊断信息；凭证与连接串不进入响应。
#[derive(Debug, Error)]
pub enum Error {
    #[error("Redis 操作失败")]
    Redis(#[from] redis::RedisError),
    #[error("文件操作失败")]
    Io(#[from] std::io::Error),
    #[error("后台文件任务失败")]
    Join(#[from] tokio::task::JoinError),
    #[error("账号 JSON 无效")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("找不到此账号")]
    NotFound,
    #[error("账号或设置发生冲突")]
    Conflict,
    #[error("密码处理失败")]
    Hash(#[from] bcrypt::BcryptError),
    #[error("Redis 版本或数据版本不兼容")]
    Schema,
}
