/// 动态板块查询协议。
///
/// 同花顺的"动态板块"（如行业板块、概念板块）在分组 API 中以 `1_` 开头
/// 的 ID 出现，为只读分组。本模块负责查询动态板块下的股票列表。

use std::collections::HashMap;

use crate::config::{
    DEFAULT_USER_AGENT, DYNAMIC_PLATE_BASE_URL, DYNAMIC_PLATE_SELECT_PATH,
    SELF_STOCK_HTTP_TIMEOUT,
};
use crate::errors::{api_error, network_error, ThsResult};
use crate::models::StockEntry;

/// 查询动态板块中的股票列表。
///
/// # Arguments
/// * `group_name` - 板块名称，如 "消费"。
/// * `cookies` - 当前会话 Cookie。
/// * `num` - 最多返回的股票数量，默认 1000。
pub fn query_dynamic_plate(
    group_name: &str,
    cookies: &HashMap<String, String>,
    num: u32,
) -> ThsResult<Vec<StockEntry>> {
    let encoded = url::form_urlencoded::byte_serialize(group_name.as_bytes()).collect::<String>();
    let url = format!(
        "{}{}?query={}&num={}",
        DYNAMIC_PLATE_BASE_URL, DYNAMIC_PLATE_SELECT_PATH, encoded, num
    );

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
        network_error("动态板块", e.to_string())
    })?;

    let codes = payload
        .get("data")
        .and_then(|v| v.get("codes"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| api_error("动态板块", "响应格式无效"))?;

    let entries: Vec<StockEntry> = codes
        .iter()
        .filter_map(|c| {
            let code = c.get("code")?.as_str()?;
            let market = c.get("market")?.as_str()?;
            Some(StockEntry::new(code, market))
        })
        .collect();

    Ok(entries)
}
