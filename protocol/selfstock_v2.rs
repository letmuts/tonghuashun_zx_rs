/// v2 自选股协议实现。
///
/// 用于"我的自选"的查询和单条增删操作。
/// 这是较新的接口，基于 `t.10jqka.com.cn`。

use std::collections::HashMap;

use crate::config::{
    DEFAULT_USER_AGENT, SELF_STOCK_HTTP_TIMEOUT, SELF_STOCK_V2_BASE_URL,
    SELF_STOCK_V2_LIST_PATH, SELF_STOCK_V2_MODIFY_PATH,
};
use crate::errors::{api_error_with_code, network_error, ThsResult};

/// 从 v2 接口下载"我的自选"列表。
///
/// # Returns
/// `(原始响应 JSON, [(股票代码, 市场类型码)])`
pub fn download_self_stocks(
    cookies: &HashMap<String, String>,
) -> ThsResult<(serde_json::Value, Vec<(String, String)>)> {
    let url = format!("{}{}", SELF_STOCK_V2_BASE_URL, SELF_STOCK_V2_LIST_PATH);
    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());

    let response = crate::client::ApiClient::shared_get(
        &url,
        &[],
        cookies,
        Some(&headers),
        SELF_STOCK_HTTP_TIMEOUT,
    )?;

    let payload: serde_json::Value = response.json().map_err(|e| {
        network_error("我的自选 v2", e.to_string())
    })?;

    let result = extract_v2_result(&payload, "我的自选")?;

    let result_array = result
        .as_array()
        .ok_or_else(|| api_error_with_code("我的自选", "响应缺少 result 列表", "?"))?;

    let mut items = Vec::new();
    for entry in result_array {
        let entry = entry.as_object().ok_or_else(|| {
            api_error_with_code("我的自选", "result 条目格式不正确", "?")
        })?;
        let code = entry
            .get("code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let marketid = entry
            .get("marketid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match (code, marketid) {
            (Some(c), Some(m)) => items.push((c, m)),
            _ => {
                return Err(api_error_with_code(
                    "我的自选",
                    "result 条目缺少 code 或 marketid",
                    "?",
                ));
            }
        }
    }

    Ok((payload, items))
}

/// 在"我的自选"中增删单条股票。
///
/// # Arguments
/// * `op` - 操作类型：`"add"` 或 `"del"`。
/// * `stockcode` - 格式为 `代码_市场类型码`，如 `"600519_17"`。
pub fn modify_self_stock(
    cookies: &HashMap<String, String>,
    op: &str,
    stockcode: &str,
) -> ThsResult<serde_json::Value> {
    let url = format!("{}{}", SELF_STOCK_V2_BASE_URL, SELF_STOCK_V2_MODIFY_PATH);
    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());

    let params = vec![("op", op), ("stockcode", stockcode)];
    let response = crate::client::ApiClient::shared_get(
        &url,
        &params,
        cookies,
        Some(&headers),
        SELF_STOCK_HTTP_TIMEOUT,
    )?;

    let payload: serde_json::Value = response.json().map_err(|e| {
        network_error("我的自选 v2 修改", e.to_string())
    })?;

    extract_v2_result(&payload, "我的自选")?;
    Ok(payload)
}

/// 提取 v2 接口返回的 result 字段，检查 errorCode。
fn extract_v2_result<'a>(
    payload: &'a serde_json::Value,
    action_name: &str,
) -> ThsResult<&'a serde_json::Value> {
    let obj = payload
        .as_object()
        .ok_or_else(|| api_error_with_code(action_name, "响应格式无效", "?"))?;

    let error_code = obj
        .get("errorCode")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    if error_code != 0 {
        let error_msg = obj
            .get("errorMsg")
            .and_then(|v| v.as_str())
            .unwrap_or("未知业务错误");
        return Err(api_error_with_code(
            action_name,
            error_msg,
            error_code.to_string(),
        ));
    }

    obj.get("result")
        .ok_or_else(|| api_error_with_code(action_name, "响应缺少 result 字段", "?"))
}
