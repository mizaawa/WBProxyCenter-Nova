#![forbid(unsafe_code)]
use anyhow::{Context, Result};
use std::{env, path::Path};
use tokio::{net::TcpListener, signal};
use tracing_subscriber::EnvFilter;
use wbpc_core::Config;
use wbpc_store::{private_dir, Runtime};
#[tokio::main]
async fn main() -> Result<()> {
    let exe = env::current_exe()?;
    let home = exe.parent().context("无法确定程序目录")?;
    let vars: Vec<_> = env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    let cfg = Config::from_pairs(vars.iter().map(|(k, v)| (k.as_str(), v.as_str())), home)?;
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_new(&cfg.log)?)
        .init();
    initialize_dirs(&cfg.data_dir, &cfg.access_dir).await?;
    let db = Runtime::connect(cfg.redis_url.as_str()).await?;
    db.initialize(wbpc_api::VERSION, &cfg.addr.to_string())
        .await?;
    let listener = TcpListener::bind(cfg.addr)
        .await
        .context("监听地址不可用")?;
    tracing::info!(addr=%listener.local_addr()?,version=wbpc_api::VERSION,"服务已启动");
    axum::serve(
        listener,
        wbpc_api::router(db).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    Ok(())
}
async fn initialize_dirs(data: &Path, access: &Path) -> Result<()> {
    private_dir(access).await?;
    private_dir(data).await?;
    for name in ["inbox", "staging", "updates", "logs"] {
        private_dir(&data.join(name)).await?;
    }
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut term) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            tokio::select! {_ = signal::ctrl_c()=>{},_=term.recv()=>{}};
            return;
        }
    }
    let _ = signal::ctrl_c().await;
}
