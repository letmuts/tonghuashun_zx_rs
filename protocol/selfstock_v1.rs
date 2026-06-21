/// v1 自选股协议实现。
///
/// 用于"我的自选"的批量操作。
/// 该协议采用读取-修改-写入（read-modify-write）模式：
/// 1. 先调用 query 获取当前完整列表与 version
/// 2. 本地合并增删操作
/// 3. 调用 modify 全量覆盖

use std::collections::HashMap;

use crate::config::{
    API_BASE_URL, DEFAULT_USER_AGENT, SELF_STOCK_HTTP_TIMEOUT, SELF_STOCK_V1_MODIFY_PATH,
    SELF_STOCK_V1_QUERY_PATH,
};
use crate::errors::{api_error, api_error_with_code, network_error, ThsResult};
use crate::models::{StockEntry, StockListVersion};

/// 从 v1 接口下载"我的自选"列表。
pub fn download_self_stocks_v1(
    cookies: &HashMap<String, String>,
) -> ThsResult<StockListVersion> {
    let url = format!("{}{}", API_BASE_URL, SELF_STOCK_V1_QUERY_PATH);
    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());
    if let Some(userid) = cookies.get("userid") {
        headers.insert("userid".to_string(), userid.clone());
    }

    let params = vec![("support_all", "0"), ("from", "thspc_hevo")];
    let response = crate::client::ApiClient::shared_get(
        &url,
        &params,
        cookies,
        Some(&headers),
        SELF_STOCK_HTTP_TIMEOUT,
    )?;

    let payload: serde_json::Value = response.json().map_err(|e| {
        network_error("我的自选 v1", e.to_string())
    })?;

    let status_code = payload
        .get("status_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    if status_code != 0 {
        let msg = payload
            .get("status_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(api_error_with_code(
            "我的自选 v1",
            msg,
            status_code.to_string(),
        ));
    }

    let data = payload.get("data").ok_or_else(|| {
        api_error("我的自选 v1", "响应缺少 data 字段")
    })?;

    let raw = data
        .get("selfstock")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let version = data
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut items = Vec::new();
    if !raw.is_empty() {
        if let Some(comma_idx) = raw.rfind(',') {
            let codes_segment = &raw[..comma_idx];
            let types_segment = &raw[comma_idx + 1..];
            let codes: Vec<&str> = codes_segment
                .split('|')
                .filter(|c| !c.is_empty())
                .collect();
            let type_codes: Vec<&str> = types_segment
                .split('|')
                .filter(|c| !c.is_empty())
                .collect();
            for (i, code) in codes.iter().enumerate() {
                let mtype = type_codes.get(i).copied().unwrap_or("");
                items.push(StockEntry::new(*code, mtype));
            }
        }
    }

    Ok(StockListVersion { version, items })
}

/// 通过 v1 接口全量写入"我的自选"列表。
///
/// # Arguments
/// * `stock_list` - 合并后的完整股票列表。
/// * `version` - 当前版本号，用于乐观锁。
pub fn modify_self_stocks_v1(
    cookies: &HashMap<String, String>,
    stock_list: &[StockEntry],
    version: &str,
) -> ThsResult<serde_json::Value> {
    let url = format!("{}{}", API_BASE_URL, SELF_STOCK_V1_MODIFY_PATH);

    let codes: Vec<String> = stock_list.iter().map(|e| e.code.clone()).collect();
    let types: Vec<String> = stock_list.iter().map(|e| e.market_type.clone()).collect();
    let selfstock_value = format!("{},{}", codes.join("|"), types.join("|"));
    let num_str = stock_list.len().to_string();

    let data: Vec<(&str, &str)> = vec![
        ("selfstock", &selfstock_value),
        ("from", "thspc_hevo"),
        ("version", version),
        ("num", &num_str),
    ];

    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());
    headers.insert(
        "Content-Type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    );
    if let Some(userid) = cookies.get("userid") {
        headers.insert("userid".to_string(), userid.clone());
    }

    let response = crate::client::ApiClient::shared_post(
        &url,
        &data,
        cookies,
        Some(&headers),
        SELF_STOCK_HTTP_TIMEOUT,
    )?;

    let payload: serde_json::Value = response.json().map_err(|e| {
        network_error("我的自选 v1 修改", e.to_string())
    })?;

    let status_code = payload
        .get("status_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    if status_code != 0 {
        let msg = payload
            .get("status_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(api_error_with_code(
            "我的自选 v1 修改",
            msg,
            status_code.to_string(),
        ));
    }

    Ok(payload.get("data").cloned().unwrap_or(serde_json::Value::Null))
}
