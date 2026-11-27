//! 上传先写到私有 inbox，返回回执后处理不依赖 HTTP 请求的生命周期。
use crate::{accounts::Vault, upload_parser, Error, Runtime};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tokio::{
    fs,
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
    task,
};
use ulid::Ulid;
/// Redis 只保存计数和可公开诊断，不保存导入内容或客户端文件名。
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub batch_id: String,
    pub status: String,
    pub uploaded: usize,
    pub imported: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
}
/// 单进程最多接受四个上传，限制磁盘 IO 与解析占用。
#[derive(Clone)]
pub struct Uploads {
    db: Runtime,
    vault: Vault,
    inbox: PathBuf,
    workers: Arc<Semaphore>,
    boot: String,
}
/// 临时目录和许可随批次释放；客户端名绝不作为文件名。
pub struct Pending {
    temp: TempDir,
    _permit: OwnedSemaphorePermit,
    count: usize,
}
impl Pending {
    /// 流式写入下一份文档，不把 multipart 整包读进内存。
    pub async fn next_file(&mut self) -> Result<fs::File, Error> {
        self.count += 1;
        Ok(fs::File::create(self.temp.path().join(self.count.to_string())).await?)
    }
}
impl Uploads {
    /// 收件箱由启动阶段初始化，和可下载的前端资源完全分离。
    pub fn new(db: Runtime, vault: Vault, inbox: PathBuf) -> Self {
        Self {
            db,
            vault,
            inbox,
            workers: Arc::new(Semaphore::new(4)),
            boot: Ulid::new().to_string(),
        }
    }
    /// 队列满时不接收新文档。
    pub async fn reserve(&self) -> Result<Pending, Error> {
        let permit = self
            .workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        let dir = self.inbox.clone();
        let temp =
            task::spawn_blocking(move || tempfile::Builder::new().prefix("batch-").tempdir_in(dir))
                .await??;
        Ok(Pending {
            temp,
            _permit: permit,
            count: 0,
        })
    }
    async fn save(&self, r: &Receipt) -> Result<(), Error> {
        redis::cmd("SET")
            .arg(self.db.key(&format!("upload:{}", r.id)))
            .arg(serde_json::to_string(r)?)
            .arg("EX")
            .arg(86400)
            .query_async::<()>(&mut self.db.conn.clone())
            .await?;
        Ok(())
    }
    /// 提交时才将批次移交后台；上传中断则自动清掉暂存文件。
    pub async fn submit(&self, pending: Pending, realm: Option<String>) -> Result<String, Error> {
        if pending.count == 0 {
            return Err(Error::Invalid("未收到账号文件"));
        }
        let id = Ulid::new().to_string();
        let receipt = Receipt {
            id: id.clone(),
            batch_id: id.clone(),
            status: "processing".into(),
            uploaded: pending.count,
            imported: 0,
            failed: 0,
            errors: vec![],
            boot: Some(self.boot.clone()),
        };
        self.save(&receipt).await?;
        let service = self.clone();
        tokio::spawn(async move {
            service.process(pending, realm, receipt).await;
        });
        Ok(id)
    }
    async fn process(&self, p: Pending, realm: Option<String>, mut r: Receipt) {
        for i in 1..=p.count {
            let file = p.temp.path().join(i.to_string());
            let (tx, mut rx) = mpsc::channel(4);
            let parser = task::spawn_blocking(move || upload_parser::parse(&file, tx));
            let mut records = 0;
            while let Some(v) = rx.recv().await {
                records += 1;
                match self.vault.import(v, realm.as_deref()).await {
                    Ok(ids) => r.imported += ids.len(),
                    Err(_) => {
                        r.failed += 1;
                        if r.errors.len() < 32 {
                            r.errors.push(format!("文件 {i} 第 {records} 条无法导入"));
                        }
                    }
                }
                if records % 64 == 0 {
                    let _ = self.save(&r).await;
                }
            }
            if !matches!(parser.await, Ok(Ok(()))) || records == 0 {
                r.failed += 1;
                if r.errors.len() < 32 {
                    r.errors.push(format!("文件 {i} 内容无效或不完整"));
                }
            }
            let _ = self.save(&r).await;
        }
        r.status = if r.failed == 0 { "completed" } else { "failed" }.into();
        if self.save(&r).await.is_err() {
            tracing::warn!(batch_id=%r.id,"写入导入回执失败");
        }
        let _ = task::spawn_blocking(move || drop(p)).await;
    }
    /// 只允许 ULID 定位回执；重启前尚未结束的批次不能永远显示为进行中。
    pub async fn receipt(&self, id: &str) -> Result<Option<Receipt>, Error> {
        if Ulid::from_string(id).is_err() {
            return Ok(None);
        }
        let raw: Option<String> = redis::cmd("GET")
            .arg(self.db.key(&format!("upload:{id}")))
            .query_async(&mut self.db.conn.clone())
            .await?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let mut r: Receipt = serde_json::from_str(&raw)?;
        if r.status == "processing" && r.boot.as_deref() != Some(&self.boot) {
            r.status = "failed".into();
            r.errors.push("服务重启，请核对已经导入的账号后重试".into());
        }
        r.boot = None;
        Ok(Some(r))
    }
}
