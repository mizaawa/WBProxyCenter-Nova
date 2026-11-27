use serde_json::{json, Value};
use std::time::Duration;
use wbpc_store::accounts::{Filter, Vault};
#[tokio::test]
async fn file_drop_visible_within_five_seconds_and_atomic_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let v = Vault::open(tmp.path()).await.unwrap();
    let id = v
        .import(json!({"access_token":"synthetic-only","label":"old"}), None)
        .await
        .unwrap()
        .remove(0);
    let mut doc = v.export_one(&id).await.unwrap();
    v.delete(&id).await.unwrap();
    doc["label"] = json!("external");
    let path = tmp.path().join("external.json");
    let (stop, rx) = tokio::sync::watch::channel(false);
    let vault = v.clone();
    let worker = tokio::spawn(async move {
        vault.watch(rx).await;
    });
    tokio::fs::write(&path, serde_json::to_vec(&doc).unwrap())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while v.list(&Filter::default()).await.total != 1 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let a = v.clone();
    let id2 = id.clone();
    let writer = tokio::spawn(async move {
        for i in 0..25 {
            a.patch(&id2, json!({"label":format!("label-{i}")}))
                .await
                .unwrap();
        }
    });
    for _ in 0..100 {
        let bytes = tokio::fs::read(&path).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["id"], id);
    }
    writer.await.unwrap();
    stop.send(true).unwrap();
    worker.await.unwrap();
}
#[tokio::test]
async fn invalid_batch_is_rejected_before_writes() {
    let t = tempfile::tempdir().unwrap();
    let v = Vault::open(t.path()).await.unwrap();
    assert!(v
        .import(
            json!([{"access_token":"synthetic-token"},{"schema":10}]),
            None
        )
        .await
        .is_err());
    assert_eq!(v.list(&Filter::default()).await.total, 0);
}
