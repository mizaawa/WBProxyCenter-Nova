#![forbid(unsafe_code)]
//! 账号文件库与 Redis 运行时存储。
mod dirs;
mod error;
mod runtime;
pub use dirs::private_dir;
pub use error::Error;
pub use runtime::Runtime;
