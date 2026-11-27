use serde_json::json;
use wbpc_store::accounts::{Filter, Vault};
#[tokio::test]
async fn import_dedup_keeps_id_and_unknown_fields_but_list_hides_secrets() {
    let d = tempfile::tempdir().unwrap();
    let v = Vault::open(d.path()).await.unwrap();
    let a=v.import(json!({"accounts":[{"label":"测试","accessToken":"sample-a","refreshToken":"sample-r"}]}),None).await.unwrap();
    let id = a[0].clone();
    let b=v.import(json!({"label":"更新","access_token":"sample-b","refresh_token":"sample-r","extra":{"v":2}}),None).await.unwrap();
    assert_eq!(b[0], id);
    let page = v.list(&Filter::default()).await;
    assert_eq!(page.total, 1);
    assert!(!serde_json::to_string(&page).unwrap().contains("sample"));
    v.patch(&id, json!({"enabled":false})).await.unwrap();
    let ids = v
        .export_ids(std::slice::from_ref(&id), &Filter::default())
        .await
        .unwrap();
    assert_eq!(ids.len(), 1);
    let d = v.export_one(&id).await.unwrap();
    assert_eq!(d["extra"]["v"], 2);
    assert_eq!(d["enabled"], false);
    v.delete(&id).await.unwrap();
    assert_eq!(v.list(&Filter::default()).await.total, 0);
}
#[tokio::test]
async fn external_edits_reload_without_redis_and_new_schema_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let v = Vault::open(d.path()).await.unwrap();
    let id = v.import(json!("test-token"), None).await.unwrap().remove(0);
    let mut data = v.export_one(&id).await.unwrap();
    v.delete(&id).await.unwrap();
    data["label"] = json!("磁盘来源");
    tokio::fs::write(
        d.path().join("renamed.json"),
        serde_json::to_vec(&data).unwrap(),
    )
    .await
    .unwrap();
    v.rescan().await.unwrap();
    assert_eq!(
        v.list(&Filter::default()).await.items[0]["label"],
        "磁盘来源"
    );
    data["schema"] = json!(2);
    tokio::fs::write(
        d.path().join("renamed.json"),
        serde_json::to_vec(&data).unwrap(),
    )
    .await
    .unwrap();
    v.rescan().await.unwrap();
    assert_eq!(v.list(&Filter::default()).await.total, 0);
}
