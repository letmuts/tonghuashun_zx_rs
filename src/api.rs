/// API 路由层。
///
/// 统一封装底层协议调用，提供标准化的 API 接口。
/// 根据操作类型自动路由到对应的协议（v1/v2/blockstock/JSON）。

use std::collections::HashMap;

use crate::client::ApiClient;
use crate::config;
use crate::config::{DEFAULT_FROM_PARAM, GROUP_QUERY_TYPES};
use crate::errors::{api_error, api_error_with_code, ThsResult};
use crate::models::{BlockstockDownload, StockEntry};
use crate::protocol::blockstock;
use crate::protocol::dynamicplate;
use crate::protocol::selfstock_v1;
use crate::protocol::selfstock_v2;

/// FavoriteAPI 提供自选股操作的统一入口。
///
/// 路由规则：
/// - "我的自选"单条操作 → v2 协议
/// - "我的自选"批量操作 → v1 协议（读-改-写）
/// - 自定义分组单条操作 → JSON API
/// - 自定义分组批量操作 → multiStorage 协议
pub struct FavoriteAPI {
    pub(crate) client: ApiClient,
}

impl FavoriteAPI {
    /// 创建 FavoriteAPI 实例。
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// 获取当前客户端的 Cookie 引用。
    pub fn get_cookies(&self) -> &HashMap<String, String> {
        self.client.get_cookies()
    }

    // ════════════════════════════════
    // 分组 CRUD
    // ════════════════════════════════

    /// 查询全部分组元数据。
    pub fn query_groups(&self) -> ThsResult<serde_json::Value> {
        let params = [
            ("from", DEFAULT_FROM_PARAM),
            ("types", GROUP_QUERY_TYPES),
        ];
        let response = self
            .client
            .get(config::endpoints::QUERY_GROUPS, &params)?;
        extract_data(&response, "获取分组")
    }

    /// 判断分组 ID 是否为动态板块（只读）。
    pub fn is_dynamic_group(group_id: &str) -> bool {
        group_id.starts_with(config::DYNAMIC_GROUP_PREFIX)
    }

    // ════════════════════════════════
    // 数据获取
    // ════════════════════════════════

    /// 获取"我的自选"列表（v2 协议）。
    pub fn list_self_stocks(&self) -> ThsResult<Vec<StockEntry>> {
        let (_payload, items) = selfstock_v2::download_self_stocks(self.client.get_cookies())?;
        Ok(items
            .into_iter()
            .map(|(code, market_id)| StockEntry::new(code, market_id))
            .collect())
    }

    /// 获取全部自定义分组数据（multiStorage 协议）。
    #[allow(dead_code)]
    pub fn list_group_stocks(&self) -> ThsResult<BlockstockDownload> {
        let auth_params = self.derive_auth_params()?;
        blockstock::download_blockstock(&auth_params, self.client.get_cookies())
    }

    /// 查询动态板块中的股票。
    pub fn query_dynamic_plate(&self, group_name: &str) -> ThsResult<Vec<StockEntry>> {
        dynamicplate::query_dynamic_plate(group_name, self.client.get_cookies(), 1000)
    }

    // ════════════════════════════════
    // 增删股票（单条）
    // ════════════════════════════════

    /// 添加单条股票到分组。
    pub fn add_item(
        &self,
        group_id: &str,
        symbol: &StockEntry,
        version: &str,
        is_self_stock: bool,
    ) -> ThsResult<serde_json::Value> {
        if is_self_stock {
            return selfstock_v2::modify_self_stock(
                self.client.get_cookies(),
                "add",
                &format!("{}_{}", symbol.code, symbol.market_type),
            );
        }
        self.item_operation(
            config::endpoints::ADD_ITEM,
            "添加股票",
            group_id,
            &symbol.code,
            &symbol.market_type,
            version,
        )
    }

    /// 删除单条股票。
    pub fn remove_item(
        &self,
        group_id: &str,
        symbol: &StockEntry,
        version: &str,
        is_self_stock: bool,
    ) -> ThsResult<serde_json::Value> {
        if is_self_stock {
            return selfstock_v2::modify_self_stock(
                self.client.get_cookies(),
                "del",
                &format!("{}_{}", symbol.code, symbol.market_type),
            );
        }
        self.item_operation(
            config::endpoints::DELETE_ITEM,
            "删除股票",
            group_id,
            &symbol.code,
            &symbol.market_type,
            version,
        )
    }

    fn item_operation(
        &self,
        endpoint: &str,
        action_name: &str,
        group_id: &str,
        item_code: &str,
        api_item_type: &str,
        version: &str,
    ) -> ThsResult<serde_json::Value> {
        let data = [
            ("id", group_id),
            ("content", &format!("{},{}", item_code, api_item_type)),
            ("num", "1"),
        ];
        self.post_with_version(endpoint, &data, version, action_name)
    }

    // ════════════════════════════════
    // 增删股票（批量）
    // ════════════════════════════════

    /// 批量添加股票。
    pub fn add_items(
        &self,
        symbols: &[StockEntry],
        is_self_stock: bool,
        group_name: Option<&str>,
    ) -> ThsResult<serde_json::Value> {
        if symbols.is_empty() {
            return Err(api_error("添加股票", "股票列表不能为空"));
        }
        if is_self_stock {
            return self.batch_self_stock(symbols, "add");
        }
        if let Some(name) = group_name {
            if self.derive_auth_params().is_ok() {
                return self.batch_group_stock(name, symbols, "add");
            }
        }
        Err(api_error("添加股票", "批量添加自定义分组需要登录态"))
    }

    /// 批量删除股票。
    pub fn remove_items(
        &self,
        symbols: &[StockEntry],
        is_self_stock: bool,
        group_name: Option<&str>,
    ) -> ThsResult<serde_json::Value> {
        if symbols.is_empty() {
            return Err(api_error("删除股票", "股票列表不能为空"));
        }
        if is_self_stock {
            return self.batch_self_stock(symbols, "delete");
        }
        if let Some(name) = group_name {
            if self.derive_auth_params().is_ok() {
                return self.batch_group_stock(name, symbols, "delete");
            }
        }
        Err(api_error("删除股票", "批量删除自定义分组需要登录态"))
    }

    /// "我的自选"批量操作（v1 读-改-写）。
    fn batch_self_stock(
        &self,
        symbols: &[StockEntry],
        action: &str,
    ) -> ThsResult<serde_json::Value> {
        let current = selfstock_v1::download_self_stocks_v1(self.client.get_cookies())?;
        let merged = merge_entries(&current.items, symbols, action, "我的自选")?;
        selfstock_v1::modify_self_stocks_v1(
            self.client.get_cookies(),
            &merged,
            &current.version,
        )
    }

    /// 自定义分组批量操作（multiStorage 读-改-写）。
    fn batch_group_stock(
        &self,
        group_name: &str,
        symbols: &[StockEntry],
        action: &str,
    ) -> ThsResult<serde_json::Value> {
        let auth_params = self.derive_auth_params()?;
        let data = blockstock::download_blockstock(&auth_params, self.client.get_cookies())?;

        let mut group_type = 0i32;
        let mut current = Vec::new();
        for g in &data.groups {
            if g.group_name == group_name {
                group_type = g.group_type;
                current = g.stock_list.clone();
                break;
            }
        }

        let merged = merge_entries(&current, symbols, action, "批量操作")?;
        blockstock::upload_blockstock(
            &auth_params,
            self.client.get_cookies(),
            group_name,
            group_type,
            &merged,
            &data.version.to_string(),
        )
    }

    // ════════════════════════════════
    // 分组管理
    // ════════════════════════════════

    /// 添加新分组。
    pub fn add_group(&self, name: &str, version: &str) -> ThsResult<serde_json::Value> {
        let data = [("name", name), ("type", "0")];
        self.post_with_version(config::endpoints::ADD_GROUP, &data, version, "添加分组")
    }

    /// 删除分组。
    pub fn delete_group(&self, group_id: &str, version: &str) -> ThsResult<serde_json::Value> {
        let data = [("ids", group_id)];
        self.post_with_version(
            config::endpoints::DELETE_GROUP,
            &data,
            version,
            "删除分组",
        )
    }

    /// 分享分组。
    pub fn share_group(&self, share_payload: &serde_json::Value) -> ThsResult<serde_json::Value> {
        let response = self
            .client
            .post_json(config::endpoints::SHARE_GROUP, share_payload)?;
        extract_data(&response, "分享分组")
    }

    // ════════════════════════════════
    // 内部工具
    // ════════════════════════════════

    fn post_with_version(
        &self,
        endpoint: &str,
        payload: &[(&str, &str)],
        version: &str,
        action_name: &str,
    ) -> ThsResult<serde_json::Value> {
        let mut data: Vec<(&str, &str)> = payload.to_vec();
        data.push(("version", version));
        data.push(("from", DEFAULT_FROM_PARAM));
        let response = self.client.post_form_urlencoded(endpoint, &data)?;
        extract_data(&response, action_name)
    }

    fn derive_auth_params(&self) -> ThsResult<HashMap<String, String>> {
        let cookies = self.client.get_cookies();
        if cookies.get("userid").map_or(true, |v| v.is_empty()) {
            return Err(api_error(
                "获取分组",
                "缺少 multiStorage 凭据，无法同步分组数据",
            ));
        }
        let result = blockstock::extract_auth_params_from_cookies(cookies);
        if result.get("sessionid").map_or(true, |v| v.is_empty()) {
            return Err(api_error(
                "获取分组",
                "缺少 multiStorage 凭据，无法同步分组数据",
            ));
        }
        Ok(result)
    }
}

// ── 共享工具函数 ──

/// 合并条目列表：将增/删操作应用到当前列表。
///
/// # Arguments
/// * `current` - 当前条目列表。
/// * `parsed_new` - 待增/删的条目列表。
/// * `action` - `"add"` 或 `"delete"`。
pub fn merge_entries(
    current: &[StockEntry],
    parsed_new: &[StockEntry],
    action: &str,
    context: &str,
) -> ThsResult<Vec<StockEntry>> {
    let mut current_map: HashMap<String, StockEntry> = current
        .iter()
        .map(|e| (e.code.clone(), e.clone()))
        .collect();

    match action {
        "add" => {
            for e in parsed_new {
                current_map.insert(e.code.clone(), e.clone());
            }
        }
        "delete" => {
            for e in parsed_new {
                current_map.remove(&e.code);
            }
        }
        _ => {
            return Err(api_error(context, format!("未知操作: {}", action)));
        }
    }

    Ok(current_map.into_values().collect())
}

/// 从 JSON 响应中提取 data 字段并检查状态码。
pub fn extract_data<'a>(
    response: &'a serde_json::Value,
    action_name: &str,
) -> ThsResult<serde_json::Value> {
    let obj = response
        .as_object()
        .ok_or_else(|| api_error(action_name, "响应格式无效"))?;

    let status_code = obj
        .get("status_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    if status_code != 0 {
        let msg = obj
            .get("status_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("未知业务错误");
        return Err(api_error_with_code(action_name, msg, status_code.to_string()));
    }

    let data = obj
        .get("data")
        .ok_or_else(|| api_error(action_name, "响应缺少 data 字段"))?;

    if !data.is_object() {
        return Err(api_error(action_name, "data 字段格式不正确"));
    }

    Ok(data.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_entries_add() {
        let current = vec![
            StockEntry::new("A", "17"),
            StockEntry::new("B", "33"),
        ];
        let new_items = vec![StockEntry::new("C", "17")];
        let result = merge_entries(&current, &new_items, "add", "test").unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_merge_entries_delete() {
        let current = vec![
            StockEntry::new("A", "17"),
            StockEntry::new("B", "33"),
        ];
        let to_remove = vec![StockEntry::new("A", "17")];
        let result = merge_entries(&current, &to_remove, "delete", "test").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].code, "B");
    }

    #[test]
    fn test_merge_entries_unknown_action() {
        let current = vec![StockEntry::new("A", "17")];
        let result = merge_entries(&current, &[], "unknown", "test");
        assert!(result.is_err());
    }
}
