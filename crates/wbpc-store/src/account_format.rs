use super::Error;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use ulid::Ulid;
fn text<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| {
        v.get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
}
pub(crate) fn merge(target: &mut Value, update: Value) {
    if let (Some(t), Some(u)) = (target.as_object_mut(), update.as_object()) {
        for (k, v) in u {
            if let Some(old) = t.get_mut(k) {
                merge(old, v.clone())
            } else {
                t.insert(k.clone(), v.clone());
            }
        }
    } else {
        *target = update;
    }
}
// Defaults are for new files only; absent fields in a sparse reimport mean "keep".
pub(crate) fn sparse_credentials(delta: &mut Value, raw: &Value) {
    if let Some(o) = delta["credentials"].as_object_mut() {
        o.retain(|key, _| {
            if raw.is_string() {
                return key == "access_token";
            }
            let aliases: &[&str] = match key.as_str() {
                "access_token" => &["access_token", "accessToken", "token"],
                "refresh_token" => &["refresh_token", "refreshToken"],
                "device_token" => &["device_token", "deviceToken"],
                _ => &[key.as_str()],
            };
            aliases.iter().any(|k| {
                raw.get(k).is_some()
                    || raw["credentials"].get(k).is_some()
                    || raw["auth"].get(k).is_some()
            })
        });
    }
}
pub(crate) fn fingerprint(v: &Value) -> Result<String, Error> {
    let s = text(
        &v["credentials"],
        &["refresh_token", "access_token", "device_token"],
    )
    .ok_or(Error::Invalid("缺少凭证"))?;
    Ok(hex::encode(Sha256::digest(s.as_bytes())))
}
pub(crate) fn parts(v: Value) -> Result<Vec<Value>, Error> {
    match v {
        Value::Array(xs) => Ok(xs),
        Value::Object(mut o) => {
            for k in ["accounts", "items", "credentials"] {
                if o.get(k).is_some_and(Value::is_array) {
                    return parts(o.remove(k).ok_or(Error::Invalid("导入格式无效"))?);
                }
            }
            Ok(vec![Value::Object(o)])
        }
        Value::String(_) => Ok(vec![v]),
        _ => Err(Error::Invalid("导入格式无效")),
    }
}
pub(crate) fn validate(v: &Value) -> Result<(), Error> {
    if v["schema"] != 1
        || v["id"]
            .as_str()
            .is_none_or(|s| Ulid::from_string(s).is_err())
        || !matches!(v["realm"].as_str(), Some("cn" | "intl"))
        || !v["label"].is_string()
        || !v["enabled"].is_boolean()
    {
        return Err(Error::Invalid("文件版本或账号字段无效"));
    }
    fingerprint(v)?;
    Ok(())
}
fn inferred(token: &str) -> Option<&'static str> {
    let payload = URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?;
    let v: Value = serde_json::from_slice(&payload).ok()?;
    let s = text(&v, &["iss", "domain"])?;
    let host = url::Url::parse(s)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| s.to_owned());
    if host == "workbuddy.ai" || host.ends_with(".workbuddy.ai") {
        Some("intl")
    } else {
        None
    }
}
pub(crate) fn normalize(raw: Value, override_realm: Option<&str>) -> Result<Value, Error> {
    let mut v = if raw.is_string() {
        json!({"access_token":raw})
    } else {
        raw
    };
    if !v.is_object() {
        return Err(Error::Invalid("账号条目必须是对象或字符串"));
    }
    if v.get("schema").is_some_and(|s| s != 1) {
        return Err(Error::Invalid("不支持的账号版本"));
    }
    let mut creds = v
        .get("credentials")
        .cloned()
        .or_else(|| v.get("auth").cloned())
        .unwrap_or_else(|| json!({}));
    if !creds.is_object() {
        return Err(Error::Invalid("凭证必须是对象"));
    }
    for (canonical, aliases) in [
        (
            "access_token",
            &["access_token", "accessToken", "token"][..],
        ),
        ("refresh_token", &["refresh_token", "refreshToken"][..]),
        ("device_token", &["device_token", "deviceToken"][..]),
    ] {
        let val = text(&creds, aliases)
            .or_else(|| text(&v, aliases))
            .unwrap_or("")
            .to_owned();
        for alias in aliases {
            if let Some(o) = v.as_object_mut() {
                o.remove(*alias);
            }
            if *alias != canonical {
                if let Some(o) = creds.as_object_mut() {
                    o.remove(*alias);
                }
            }
        }
        if !val.is_empty() {
            creds[canonical] = json!(val);
        }
    }
    let token = creds["access_token"].as_str().unwrap_or("");
    let realm = override_realm
        .or_else(|| text(&v, &["realm"]))
        .or_else(|| inferred(token))
        .unwrap_or("cn")
        .to_owned();
    if !matches!(realm.as_str(), "cn" | "intl") {
        return Err(Error::Invalid("号池无效"));
    }
    let kind = if token.starts_with("ck_") || token.starts_with("pt_") {
        "apikey"
    } else if token.is_empty() && creds["device_token"].is_string() {
        "device"
    } else {
        "oauth"
    };
    let mut c = json!({"kind":kind,"access_token":"","refresh_token":"","device_token":"","token_type":"Bearer","expires_at":0,"uid":"","email":"","nickname":""});
    merge(&mut c, creds);
    let label = text(&v, &["label", "nickname"])
        .or_else(|| text(&v["account"], &["nickname", "label"]))
        .unwrap_or("未命名账号")
        .to_owned();
    let stamp = Utc::now().to_rfc3339();
    let international = realm == "intl";
    let billing = if international {
        "https://www.workbuddy.ai"
    } else {
        "https://www.codebuddy.cn"
    };
    let mut out = json!({"schema":1,"id":Ulid::new().to_string(),"label":label,"product":"workbuddy","enabled":true,"tasks_enabled":true,"note":"","created_at":stamp,"updated_at":stamp,"upstream":{"base_url":if international {billing}else{"https://copilot.tencent.com"},"billing_url":billing,"origin":billing},"egress":{"mode":"follow","proxy_id":"","url":"","sticky_id":""},"limits":{"daily_token_limit":0,"daily_credit_limit":0,"model_daily_token_limit":0,"reserve_credits":0,"max_concurrent":0},"models":{"disabled":[]}});
    if let Some(o) = v.as_object_mut() {
        for k in ["credentials", "auth", "runtime"] {
            o.remove(k);
        }
    }
    merge(&mut out, v);
    out["realm"] = json!(realm);
    out["credentials"] = c;
    validate(&out)?;
    Ok(out)
}
pub(crate) fn filename(v: &Value) -> String {
    let slug: String = v["label"]
        .as_str()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_-".contains(*c) || ('一'..='龥').contains(c))
        .take(24)
        .collect();
    let id = v["id"].as_str().unwrap_or("");
    format!(
        "{}-{}-{}.json",
        v["realm"].as_str().unwrap_or("cn"),
        if slug.is_empty() { "account" } else { &slug },
        &id[id.len().saturating_sub(8)..]
    )
}
pub(crate) fn summary(v: &Value) -> Value {
    json!({"id":v["id"],"label":v["label"],"realm":v["realm"],"enabled":v["enabled"],"tasks_enabled":v["tasks_enabled"],"kind":v["credentials"]["kind"],"health":"pending","created_at":v["created_at"],"updated_at":v["updated_at"]})
}
