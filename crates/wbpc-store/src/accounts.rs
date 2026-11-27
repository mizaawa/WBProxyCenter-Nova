//! 文件真源与可丢弃索引。内存中长期保存的只有界面摘要、路径和去重指纹。
use crate::{account_file as file, account_format as fmt, private_dir, Error};
use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    fs,
    sync::{mpsc, watch, Mutex, RwLock},
    time,
};
#[derive(Clone)]
struct Entry {
    path: PathBuf,
    mtime: SystemTime,
    len: u64,
    id: String,
    fp: String,
    meta: Value,
}
#[derive(Default)]
struct Index {
    files: HashMap<PathBuf, Entry>,
    by_id: BTreeMap<String, Entry>,
}
impl Index {
    fn rebuild(&mut self) {
        self.by_id.clear();
        for v in self.files.values() {
            if let Some(old) = self.by_id.get(&v.id) {
                tracing::warn!(account_id=%v.id,"发现重复账号 ID，使用最新文件");
                if (&old.mtime, &old.path) >= (&v.mtime, &v.path) {
                    continue;
                }
            }
            self.by_id.insert(v.id.clone(), v.clone());
        }
    }
}
struct Inner {
    dir: PathBuf,
    write: Mutex<()>,
    index: RwLock<Index>,
}
/// 以文件为真源，Redis 重建不会丢失账号。
#[derive(Clone)]
pub struct Vault(Arc<Inner>);
/// 列表与导出共享筛选语义；仅列表消费分页字段。
#[derive(Deserialize, Default, Clone)]
pub struct Filter {
    pub realm: Option<String>,
    pub health: Option<String>,
    pub q: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub view: Option<String>,
}
impl Filter {
    fn matches(&self, v: &Entry) -> bool {
        self.realm
            .as_deref()
            .is_none_or(|r| r.is_empty() || r == "all" || v.meta["realm"] == r)
            && self
                .health
                .as_deref()
                .is_none_or(|g| g.is_empty() || g == "all" || v.meta["health"] == g)
            && self.q.as_deref().is_none_or(|q| {
                let q = q.trim().to_lowercase();
                v.id.to_lowercase().contains(&q)
                    || v.meta["label"]
                        .as_str()
                        .is_some_and(|s| s.to_lowercase().contains(&q))
            })
    }
}
/// 分页时不复制整库，仅克隆当前页的可展示数据。
#[derive(Serialize)]
pub struct Page {
    pub items: Vec<Value>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}
async fn entry(path: PathBuf, v: &Value) -> Result<Entry, Error> {
    let m = fs::symlink_metadata(&path).await?;
    Ok(Entry {
        path,
        mtime: m.modified()?,
        len: m.len(),
        id: v["id"]
            .as_str()
            .ok_or(Error::Invalid("缺少账号 ID"))?
            .into(),
        fp: fmt::fingerprint(v)?,
        meta: fmt::summary(v),
    })
}
impl Vault {
    /// 先完成初始扫描再对外提供列表。
    pub async fn open(dir: &Path) -> Result<Self, Error> {
        private_dir(dir).await?;
        let v = Self(Arc::new(Inner {
            dir: dir.to_owned(),
            write: Mutex::new(()),
            index: RwLock::new(Index::default()),
        }));
        v.rescan().await?;
        Ok(v)
    }
    /// 测试和文件事件可以强制重读；兜底扫描复用 mtime/size 未变的摘要。
    pub async fn rescan(&self) -> Result<(), Error> {
        self.scan(true).await
    }
    async fn scan(&self, force: bool) -> Result<(), Error> {
        let _guard = self.0.write.lock().await;
        let mut next = Index::default();
        let mut dir = fs::read_dir(&self.0.dir).await?;
        while let Some(item) = dir.next_entry().await? {
            let path = item.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json")
                || item.file_name().to_string_lossy().starts_with('.')
                || !item.file_type().await?.is_file()
            {
                continue;
            }
            if !force {
                let m = item.metadata().await?;
                if let Some(prev) = self.0.index.read().await.files.get(&path) {
                    if prev.len == m.len() && m.modified().ok() == Some(prev.mtime) {
                        next.files.insert(path, prev.clone());
                        continue;
                    }
                }
            }
            match file::read(&path).await {
                Ok(v) => {
                    let e = entry(path.clone(), &v).await?;
                    next.files.insert(path, e);
                }
                Err(_) => tracing::warn!("忽略不可读或不兼容的账号文件"),
            }
        }
        next.rebuild();
        *self.0.index.write().await = next;
        Ok(())
    }
    /// notify 事件合并 300ms，4s 补扫；文件监听不可用时仍然可以导入。
    pub async fn watch(&self, mut stop: watch::Receiver<bool>) {
        let (tx, mut rx) = mpsc::channel(1);
        let dir = self.0.dir.clone();
        let _watcher = tokio::task::spawn_blocking(move || {
            let mut w = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
                if ev.is_ok_and(|e| !matches!(e.kind, EventKind::Access(_))) {
                    let _ = tx.try_send(());
                }
            })?;
            w.watch(&dir, RecursiveMode::NonRecursive)?;
            Ok::<_, notify::Error>(w)
        })
        .await
        .ok()
        .and_then(Result::ok);
        let mut pulse = time::interval(Duration::from_secs(4));
        pulse.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            let force = tokio::select! {_=stop.changed()=>break,_=pulse.tick()=>false,Some(())=rx.recv()=>{time::sleep(Duration::from_millis(300)).await;while rx.try_recv().is_ok(){}true}};
            if self.scan(force).await.is_err() {
                tracing::warn!("账号目录扫描失败");
            }
        }
    }
    /// 根据摘要分页，凭证不会进入列表响应。
    pub async fn list(&self, f: &Filter) -> Page {
        let size = f.page_size.unwrap_or(20).clamp(1, 200);
        let page = f.page.unwrap_or(1).max(1);
        let start = (page - 1).saturating_mul(size);
        let mut items = Vec::with_capacity(size);
        let mut total = 0;
        let ix = self.0.index.read().await;
        for e in ix.by_id.values().rev().filter(|e| f.matches(e)) {
            if total >= start && items.len() < size {
                items.push(if f.view.as_deref() == Some("ids") {
                    json!(e.id)
                } else {
                    e.meta.clone()
                });
            }
            total += 1;
        }
        Page {
            items,
            total,
            page,
            page_size: size,
        }
    }
    async fn get_file(&self, id: &str) -> Result<(PathBuf, Value), Error> {
        let path = self
            .0
            .index
            .read()
            .await
            .by_id
            .get(id)
            .map(|e| e.path.clone())
            .ok_or(Error::NotFound)?;
        let v = file::read(&path).await?;
        if v["id"] != id {
            return Err(Error::Conflict);
        }
        Ok((path, v))
    }
    /// 详情同样不显示凭证或嵌入代理 URL 的密码。
    pub async fn detail(&self, id: &str) -> Result<Value, Error> {
        let (_, v) = self.get_file(id).await?;
        let mut s = fmt::summary(&v);
        for k in ["note", "limits", "models"] {
            s[k] = v[k].clone();
        }
        s["egress"] = json!({"mode":v["egress"]["mode"],"proxy_id":v["egress"]["proxy_id"]});
        Ok(s)
    }
    async fn publish(&self, path: PathBuf, before: Option<Value>, v: Value) -> Result<(), Error> {
        file::replace(path.clone(), before, Some(v.clone())).await?;
        let e = entry(path.clone(), &v).await?;
        let mut ix = self.0.index.write().await;
        ix.files.insert(path, e);
        ix.rebuild();
        Ok(())
    }
    /// 先校验整批，重复凭证只更新原文件。每个文件的提交都是原子的。
    pub async fn import(&self, raw: Value, realm: Option<&str>) -> Result<Vec<String>, Error> {
        let docs = fmt::parts(raw)?;
        if docs.is_empty() || docs.len() > 4096 {
            return Err(Error::Invalid("每批需包含 1 至 4096 个账号"));
        }
        let mut prepared = Vec::with_capacity(docs.len());
        for raw in docs {
            let v = fmt::normalize(raw.clone(), realm)?;
            prepared.push((v, raw));
        }
        let _guard = self.0.write.lock().await;
        let mut ids = Vec::new();
        for (mut v, raw) in prepared {
            let fp = fmt::fingerprint(&v)?;
            let existing = {
                let ix = self.0.index.read().await;
                ix.by_id
                    .values()
                    .find(|e| e.fp == fp)
                    .cloned()
                    .or_else(|| v["id"].as_str().and_then(|id| ix.by_id.get(id).cloned()))
            };
            let (path, before) = if let Some(e) = existing {
                let (path, old) = self.get_file(&e.id).await?;
                if old["id"] != e.id {
                    return Err(Error::Conflict);
                }
                let mut merged = old.clone();
                let mut delta = v.clone();
                if let Some(obj) = delta.as_object_mut() {
                    for k in [
                        "label",
                        "enabled",
                        "tasks_enabled",
                        "product",
                        "egress",
                        "limits",
                        "models",
                        "note",
                        "upstream",
                    ] {
                        if raw.get(k).is_none() {
                            obj.remove(k);
                        }
                    }
                    if raw.get("realm").is_none() && realm.is_none() {
                        obj.remove("realm");
                    }
                    obj.remove("id");
                    obj.remove("created_at");
                }
                fmt::merge(&mut merged, delta);
                v = merged;
                (path, Some(old))
            } else {
                let name = fmt::filename(&v);
                (self.0.dir.join(name), None)
            };
            v["updated_at"] = json!(Utc::now().to_rfc3339());
            let id = v["id"]
                .as_str()
                .ok_or(Error::Invalid("无效账号 ID"))?
                .to_owned();
            self.publish(path, before, v).await?;
            ids.push(id);
        }
        Ok(ids)
    }
    /// 管理面只能修改已知配置项，不能通过 PATCH 替换身份或注入路径。
    pub async fn patch(&self, id: &str, patch: Value) -> Result<Value, Error> {
        let o = patch
            .as_object()
            .ok_or(Error::Invalid("修改内容必须是对象"))?;
        if o.is_empty()
            || !o.iter().all(|(k, v)| match k.as_str() {
                "label" => v
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 128),
                "note" => v.is_string(),
                "enabled" | "tasks_enabled" => v.is_boolean(),
                "limits" => v.as_object().is_some_and(|a| {
                    a.iter().all(|(name, n)| match name.as_str() {
                        "max_concurrent" | "daily_token_limit" | "model_daily_token_limit" => {
                            n.as_u64().is_some()
                        }
                        "reserve_credits" | "daily_credit_limit" => {
                            n.as_f64().is_some_and(|x| x >= 0.0)
                        }
                        _ => false,
                    })
                }),
                "models" => v.as_object().is_some_and(|a| {
                    a.keys().all(|k| k == "disabled")
                        && a.get("disabled")
                            .and_then(Value::as_array)
                            .is_some_and(|xs| xs.iter().all(Value::is_string))
                }),
                "egress" => v.as_object().is_some_and(|a| {
                    a.iter().all(|(k, v)| match k.as_str() {
                        "mode" => matches!(v.as_str(), Some("follow" | "bind" | "direct")),
                        "proxy_id" | "url" | "sticky_id" => v.is_string(),
                        _ => false,
                    })
                }),
                _ => false,
            })
        {
            return Err(Error::Invalid("不支持的修改项或类型"));
        }
        let _guard = self.0.write.lock().await;
        let (path, old) = self.get_file(id).await?;
        let mut v = old.clone();
        fmt::merge(&mut v, patch);
        v["updated_at"] = json!(Utc::now().to_rfc3339());
        let summary = fmt::summary(&v);
        self.publish(path, Some(old), v).await?;
        Ok(summary)
    }
    /// 删除同一 ID 的所有副本，避免下一轮扫描恢复旧文件。
    pub async fn delete(&self, id: &str) -> Result<(), Error> {
        let _guard = self.0.write.lock().await;
        let paths: Vec<_> = self
            .0
            .index
            .read()
            .await
            .files
            .values()
            .filter(|e| e.id == id)
            .map(|e| e.path.clone())
            .collect();
        if paths.is_empty() {
            return Err(Error::NotFound);
        }
        for path in paths {
            let old = file::read(&path).await?;
            if old["id"] != id {
                return Err(Error::Conflict);
            }
            file::replace(path.clone(), Some(old), None).await?;
            let mut ix = self.0.index.write().await;
            ix.files.remove(&path);
            ix.rebuild();
        }
        Ok(())
    }
    /// 导出使用稳定的 ID 快照；逐文件读取，无法导出的条目不能静默丢失。
    pub async fn export_ids(&self, ids: &[String], f: &Filter) -> Result<Vec<String>, Error> {
        let selected: HashSet<_> = ids.iter().collect();
        let ix = self.0.index.read().await;
        if selected.iter().any(|id| !ix.by_id.contains_key(*id)) {
            return Err(Error::NotFound);
        }
        Ok(ix
            .by_id
            .values()
            .filter(|e| (selected.is_empty() || selected.contains(&e.id)) && f.matches(e))
            .map(|e| e.id.clone())
            .collect())
    }
    /// 读取单个含明文凭证的备份对象；只应在已授权的导出流程使用。
    pub async fn export_one(&self, id: &str) -> Result<Value, Error> {
        let (_, mut v) = self.get_file(id).await?;
        v["runtime"] = json!({"health":"pending"});
        Ok(v)
    }
}
