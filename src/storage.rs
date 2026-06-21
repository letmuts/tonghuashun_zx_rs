/// 缓存数据持久化模块。
///
/// 管理分组数据缓存（`ths_favorite_cache.json`）和
/// Cookie 凭据缓存（`ths_cookie_cache.json`）的读写。

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use log::{info, warn};
use serde::{Deserialize, Serialize};

use crate::errors::{ThsError, ThsResult};
use crate::models::{StockGroup, StockItem};

// ── 分组数据缓存 ──

/// 从磁盘加载自选股缓存。
///
/// # Returns
/// `(分组映射表, 我的自选分组)`
pub fn load_favorite_cache(cache_path: &str) -> (HashMap<String, StockGroup>, Option<StockGroup>) {
    if !Path::new(cache_path).exists() {
        info!("缓存文件 '{}' 不存在，跳过加载。", cache_path);
        return (HashMap::new(), None);
    }

    let text = match fs::read_to_string(cache_path) {
        Ok(t) => t,
        Err(e) => {
            warn!("读取缓存文件 '{}' 失败: {}", cache_path, e);
            return (HashMap::new(), None);
        }
    };

    let root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            warn!("缓存文件 '{}' 不是有效的 JSON: {}", cache_path, e);
            return (HashMap::new(), None);
        }
    };

    let mut groups = HashMap::new();
    if let Some(groups_array) = root.get("groups").and_then(|v| v.as_array()) {
        for group_val in groups_array {
            let name = group_val.get("name").and_then(|v| v.as_str());
            let group_id = group_val.get("group_id").and_then(|v| v.as_str());
            if let (Some(name), Some(group_id)) = (name, group_id) {
                let items: Vec<StockItem> = group_val
                    .get("items")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                let code = item.get("code")?.as_str()?;
                                let market = item
                                    .get("market")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                Some(StockItem::new(code, market))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                groups.insert(
                    name.to_string(),
                    StockGroup {
                        name: name.to_string(),
                        group_id: group_id.to_string(),
                        items,
                        readonly: false,
                    },
                );
            }
        }
    }

    let self_stock = root
        .get("self_stock")
        .and_then(|v| {
            let name = v.get("name")?.as_str()?;
            let group_id = v.get("group_id")?.as_str()?;
            let items: Vec<StockItem> = v
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let code = item.get("code")?.as_str()?;
                            let market = item
                                .get("market")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            Some(StockItem::new(code, market))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(StockGroup {
                name: name.to_string(),
                group_id: group_id.to_string(),
                items,
                readonly: false,
            })
        });

    info!(
        "已从 '{}' 加载 {} 个分组{}。",
        cache_path,
        groups.len(),
        if self_stock.is_some() {
            format!("，自选股「{}」", self_stock.as_ref().unwrap().name)
        } else {
            String::new()
        }
    );

    (groups, self_stock)
}

/// 将自选股数据持久化到磁盘。
pub fn save_favorite_cache(
    cache_path: &str,
    groups: &HashMap<String, StockGroup>,
    self_stock: &Option<StockGroup>,
) -> ThsResult<()> {
    let groups_array: Vec<serde_json::Value> = groups
        .values()
        .filter(|g| !g.readonly)
        .map(|g| {
            serde_json::json!({
                "name": g.name,
                "group_id": g.group_id,
                "items": g.items.iter().map(|item| {
                    serde_json::json!({
                        "code": item.code,
                        "market": item.market,
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect();

    let mut root = serde_json::json!({ "groups": groups_array });

    if let Some(ss) = self_stock {
        root["self_stock"] = serde_json::json!({
            "name": ss.name,
            "group_id": ss.group_id,
            "items": ss.items.iter().map(|item| {
                serde_json::json!({
                    "code": item.code,
                    "market": item.market,
                })
            }).collect::<Vec<_>>(),
        });
    }

    let json_str = serde_json::to_string_pretty(&root).map_err(|e| {
        ThsError::Api {
            action: "保存缓存".into(),
            message: format!("序列化失败: {}", e),
            code: None,
        }
    })?;

    fs::write(cache_path, &json_str).map_err(|e| ThsError::Api {
        action: "保存缓存".into(),
        message: format!("写入文件失败: {}", e),
        code: None,
    })?;

    info!("已成功保存缓存到 '{}'。", cache_path);
    Ok(())
}

// ── Cookie 凭据缓存 ──

/// 从 Cookie 缓存文件中读取全部数据。
pub fn load_cookie_cache(cache_path: &str) -> HashMap<String, serde_json::Value> {
    if !Path::new(cache_path).exists() {
        return HashMap::new();
    }
    let text = match fs::read_to_string(cache_path) {
        Ok(t) => t,
        Err(e) => {
            warn!("读取 cookie 缓存文件 '{}' 失败: {}", cache_path, e);
            return HashMap::new();
        }
    };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        warn!("cookie 缓存文件 '{}' 内容无效: {}", cache_path, e);
        HashMap::new()
    })
}

/// 读取有效的缓存 Cookie。
///
/// 当缓存条目在 TTL 内时返回其 Cookie 映射表。
pub fn read_cached_cookies(
    cache_path: &str,
    cache_key: &str,
    ttl_seconds: u64,
) -> Option<HashMap<String, String>> {
    let cache_data = load_cookie_cache(cache_path);
    let entry = cache_data.get(cache_key)?;

    let timestamp = entry.get("timestamp")?.as_f64()?;
    let now = current_timestamp();
    if now - timestamp > ttl_seconds as f64 {
        info!("cookies 缓存已过期: {}", cache_key);
        return None;
    }

    let cookies = entry.get("cookies")?.as_object()?;
    let result: HashMap<String, String> = cookies
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
        .collect();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// 读取有效的 auth_params（用于 multiStorage 协议）。
#[allow(dead_code)]
pub fn read_cached_auth_params(
    cache_path: &str,
    cache_key: &str,
    ttl_seconds: u64,
) -> Option<HashMap<String, String>> {
    let cache_data = load_cookie_cache(cache_path);
    let entry = cache_data.get(cache_key)?;

    let timestamp = entry.get("timestamp")?.as_f64()?;
    let now = current_timestamp();
    if now - timestamp > ttl_seconds as f64 {
        return None;
    }

    let auth = entry.get("auth_params")?.as_object()?;
    let result: HashMap<String, String> = auth
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
        .collect();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// 写入 Cookie 缓存条目。
pub fn write_cookie_cache(
    cache_path: &str,
    cache_key: &str,
    cookies: &HashMap<String, String>,
    auth_params: Option<&HashMap<String, String>>,
) -> ThsResult<()> {
    let mut cache_data = load_cookie_cache(cache_path);

    let cookies_json: serde_json::Map<String, serde_json::Value> = cookies
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();

    let mut entry = serde_json::json!({
        "cookies": cookies_json,
        "timestamp": current_timestamp(),
    });

    if let Some(params) = auth_params {
        if let Some(obj) = entry.as_object_mut() {
            let params_json: serde_json::Map<String, serde_json::Value> = params
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            obj.insert(
                "auth_params".to_string(),
                serde_json::Value::Object(params_json),
            );
        }
    }

    cache_data.insert(cache_key.to_string(), entry);

    let json_str = serde_json::to_string(&cache_data).map_err(|e| ThsError::Api {
        action: "保存 Cookie 缓存".into(),
        message: format!("序列化失败: {}", e),
        code: None,
    })?;

    fs::write(cache_path, &json_str).map_err(|e| ThsError::Api {
        action: "保存 Cookie 缓存".into(),
        message: format!("写入文件失败: {}", e),
        code: None,
    })?;

    Ok(())
}

/// 获取当前 Unix 时间戳（秒）。
fn current_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
