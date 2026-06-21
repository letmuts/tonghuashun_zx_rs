/// 高层服务模块。
///
/// `PortfolioManager` 提供自选股管理的统一入口，负责协调认证、
/// API 调用、缓存策略和元数据填充。

use std::collections::HashMap;

use log::{info, warn};

use crate::api::FavoriteAPI;
use crate::auth::SessionManager;
use crate::client::ApiClient;
use crate::config::{
    API_BASE_URL, CACHE_FILE, COOKIE_CACHE_FILE, COOKIE_CACHE_TTL_SECONDS,
    SELF_STOCK_DEFAULT_NAME, SELF_STOCK_GROUP_ID,
};
use crate::constant::market_abbr;
use crate::errors::{api_error, ThsResult};
use crate::models::{StockEntry, StockGroup, StockItem};
use crate::storage::{load_favorite_cache, save_favorite_cache};
use crate::utils::extract_item_attr;

/// 自选股管理器。
///
/// 提供完整的自选股管理能力：
/// - 登录认证
/// - 查询分组与自选股
/// - 增删股票与分组
/// - 分享分组
/// - 本地缓存
pub struct PortfolioManager {
    api: FavoriteAPI,
    #[allow(dead_code)]
    session_manager: SessionManager,
    groups_cache: HashMap<String, StockGroup>,
    self_stock_cache: Option<StockGroup>,
    current_version: Option<String>,
    selfstock_detail_version: Option<String>,
    selfstock_detail_map: HashMap<(String, String), serde_json::Value>,
    enable_cache: bool,
    cache_path: String,
}

impl PortfolioManager {
    /// 创建新的 PortfolioManager。
    ///
    /// # Arguments
    /// * `cookies` - 可选的显式 Cookie。
    /// * `username` - 可选账号。
    /// * `password` - 可选密码。
    /// * `cookie_cache_path` - Cookie 缓存路径（默认 `ths_cookie_cache.json`）。
    /// * `cookie_cache_ttl` - Cookie 缓存有效期（秒，默认 24h）。
    /// * `enable_cache` - 是否启用数据缓存（默认 true）。
    pub fn new(
        cookies: Option<&HashMap<String, String>>,
        username: Option<&str>,
        password: Option<&str>,
        cookie_cache_path: Option<&str>,
        cookie_cache_ttl: Option<u64>,
        enable_cache: bool,
    ) -> Self {
        let cache_path = CACHE_FILE.to_string();

        let (groups_cache, self_stock_cache) = if enable_cache {
            load_favorite_cache(&cache_path)
        } else {
            (HashMap::new(), None)
        };

        let session_path = cookie_cache_path.unwrap_or(COOKIE_CACHE_FILE);
        let session_ttl = cookie_cache_ttl.unwrap_or(COOKIE_CACHE_TTL_SECONDS);

        let mut session_manager = SessionManager::new(
            cookies,
            username,
            password,
            Some(session_path),
            Some(session_ttl),
        );

        let resolved = session_manager.resolve();
        let api_client = ApiClient::new(API_BASE_URL, resolved.as_ref(), None);

        Self {
            api: FavoriteAPI::new(api_client),
            session_manager,
            groups_cache,
            self_stock_cache,
            current_version: None,
            selfstock_detail_version: None,
            selfstock_detail_map: HashMap::new(),
            enable_cache,
            cache_path,
        }
    }

    /// 获取全部分组。
    ///
    /// # Arguments
    /// * `include_self_stocks` - 是否将"我的自选"并入返回结果。
    pub fn get_all_groups(
        &mut self,
        include_self_stocks: bool,
    ) -> ThsResult<HashMap<String, StockGroup>> {
        info!("开始获取所有自选股分组信息...");

        let raw_data = self.api.query_groups()?;
        let parsed_groups = self.parse_group_list(&raw_data);

        let _ = self.refresh_selfstock_detail();

        let mut formatted = HashMap::new();

        for group_raw in parsed_groups {
            let name = match group_raw.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let group_id = match group_raw.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };

            if FavoriteAPI::is_dynamic_group(&group_id) {
                // 动态板块
                let items = match self.api.query_dynamic_plate(&name) {
                    Ok(entries) => entries
                        .into_iter()
                        .map(|e| {
                            let m = market_abbr(&e.market_type);
                            StockItem::new(e.code, Some(m))
                        })
                        .collect(),
                    Err(e) => {
                        warn!("获取动态分组「{}」股票列表失败: {}", name, e);
                        Vec::new()
                    }
                };
                formatted.insert(
                    name.clone(),
                    StockGroup {
                        name,
                        group_id,
                        items,
                        readonly: true,
                    },
                );
            } else {
                let items: Vec<StockItem> = group_raw
                    .get("item_details")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|detail| {
                                let code = detail.get("code")?.as_str()?;
                                let api_type = detail.get("api_type").and_then(|v| v.as_str());
                                let m = api_type.map(|t| market_abbr(t));
                                Some(StockItem::new(code, m))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let mut group = StockGroup {
                    name: name.clone(),
                    group_id: group_id.clone(),
                    items,
                    readonly: false,
                };
                self.attach_selfstock_metadata(&mut group.items);
                formatted.insert(name, group);
            }
        }

        self.groups_cache = formatted.clone();

        if self.enable_cache {
            let cacheable: HashMap<String, StockGroup> = formatted
                .iter()
                .filter(|(_, g)| !g.readonly)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let _ = save_favorite_cache(&self.cache_path, &cacheable, &self.self_stock_cache);
        }

        if include_self_stocks {
            let self_group = self.get_self_stocks(false)?;
            formatted.insert(self_group.name.clone(), self_group);
        }

        info!("成功获取并处理了 {} 个分组。", formatted.len());
        Ok(formatted)
    }

    /// 获取"我的自选"。
    pub fn get_self_stocks(&mut self, refresh: bool) -> ThsResult<StockGroup> {
        let group_name = SELF_STOCK_DEFAULT_NAME.to_string();

        if self.enable_cache && !refresh {
            if let Some(ref cached) = self.self_stock_cache {
                let mut items = cached.items.clone();
                let _ = self.refresh_selfstock_detail();
                self.attach_selfstock_metadata(&mut items);
                return Ok(StockGroup {
                    name: group_name,
                    group_id: SELF_STOCK_GROUP_ID.to_string(),
                    items,
                    readonly: false,
                });
            }
        }

        let entries = self.api.list_self_stocks()?;
        let _ = self.refresh_selfstock_detail();

        let parsed_items: Vec<StockItem> = entries
            .into_iter()
            .map(|e| StockItem::new(e.code, Some(market_abbr(&e.market_type))))
            .collect();

        let group = StockGroup {
            name: group_name,
            group_id: SELF_STOCK_GROUP_ID.to_string(),
            items: parsed_items.clone(),
            readonly: false,
        };

        self.self_stock_cache = Some(group.clone());
        if self.enable_cache {
            let _ = save_favorite_cache(
                &self.cache_path,
                &self.groups_cache,
                &self.self_stock_cache,
            );
        }

        Ok(group)
    }

    /// 添加单支股票到分组。
    pub fn add_item(
        &mut self,
        group_identifier: &str,
        symbol: &str,
    ) -> ThsResult<serde_json::Value> {
        info!("尝试添加项目 '{}' 到分组 '{}'...", symbol, group_identifier);
        let parsed = parse_symbols(&[symbol.to_string()]);
        if parsed.is_empty() {
            return Err(api_error("添加股票", "无法解析股票代码"));
        }

        let is_self_stock = group_identifier == SELF_STOCK_DEFAULT_NAME
            || group_identifier == SELF_STOCK_GROUP_ID;

        if is_self_stock {
            let result = self.api.add_item(
                SELF_STOCK_GROUP_ID,
                &parsed[0],
                "",
                true,
            )?;
            let _ = self.get_self_stocks(true);
            return Ok(result);
        }

        // 查找分组 ID
        let target_id = self.find_group_id(group_identifier)?;
        let version = self.ensure_version();
        let result = self.api.add_item(&target_id, &parsed[0], &version, false)?;
        let _ = self.get_all_groups(false);
        Ok(result)
    }

    /// 批量添加股票到分组。
    pub fn add_items(
        &mut self,
        group_identifier: &str,
        symbols: &[String],
    ) -> ThsResult<serde_json::Value> {
        let parsed = parse_symbols(symbols);
        if parsed.is_empty() {
            return Err(api_error("添加股票", "无法解析股票代码"));
        }

        let is_self_stock = group_identifier == SELF_STOCK_DEFAULT_NAME
            || group_identifier == SELF_STOCK_GROUP_ID;

        if is_self_stock {
            let result = self.api.add_items(&parsed, true, None)?;
            let _ = self.get_self_stocks(true);
            return Ok(result);
        }

        let group_name = group_identifier.to_string();
        let result = self.api.add_items(&parsed, false, Some(&group_name))?;
        let _ = self.get_all_groups(false);
        Ok(result)
    }

    /// 删除单支股票。
    pub fn remove_item(
        &mut self,
        group_identifier: &str,
        symbol: &str,
    ) -> ThsResult<serde_json::Value> {
        info!("尝试删除项目 '{}' 从分组 '{}'...", symbol, group_identifier);
        let parsed = parse_symbols(&[symbol.to_string()]);
        if parsed.is_empty() {
            return Err(api_error("删除股票", "无法解析股票代码"));
        }

        let is_self_stock = group_identifier == SELF_STOCK_DEFAULT_NAME
            || group_identifier == SELF_STOCK_GROUP_ID;

        if is_self_stock {
            let result = self.api.remove_item(
                SELF_STOCK_GROUP_ID,
                &parsed[0],
                "",
                true,
            )?;
            let _ = self.get_self_stocks(true);
            return Ok(result);
        }

        let target_id = self.find_group_id(group_identifier)?;
        let version = self.ensure_version();
        let result = self.api.remove_item(&target_id, &parsed[0], &version, false)?;
        let _ = self.get_all_groups(false);
        Ok(result)
    }

    /// 批量删除股票。
    pub fn remove_items(
        &mut self,
        group_identifier: &str,
        symbols: &[String],
    ) -> ThsResult<serde_json::Value> {
        let parsed = parse_symbols(symbols);
        if parsed.is_empty() {
            return Err(api_error("删除股票", "无法解析股票代码"));
        }

        let is_self_stock = group_identifier == SELF_STOCK_DEFAULT_NAME
            || group_identifier == SELF_STOCK_GROUP_ID;

        if is_self_stock {
            let result = self.api.remove_items(&parsed, true, None)?;
            let _ = self.get_self_stocks(true);
            return Ok(result);
        }

        let group_name = group_identifier.to_string();
        let result = self.api.remove_items(&parsed, false, Some(&group_name))?;
        let _ = self.get_all_groups(false);
        Ok(result)
    }

    /// 添加新分组。
    pub fn add_group(&mut self, group_name: &str) -> ThsResult<serde_json::Value> {
        if group_name.is_empty() {
            return Err(api_error("添加分组", "分组名称不能为空"));
        }
        let version = self.ensure_version();
        let result = self.api.add_group(group_name, &version)?;
        let _ = self.get_all_groups(false);
        Ok(result)
    }

    /// 删除分组。
    pub fn delete_group(&mut self, group_identifier: &str) -> ThsResult<serde_json::Value> {
        let target_id = self.find_group_id(group_identifier)?;
        let version = self.ensure_version();
        let result = self.api.delete_group(&target_id, &version)?;
        let _ = self.get_all_groups(false);
        Ok(result)
    }

    /// 分享分组，返回分享链接信息。
    pub fn share_group(
        &self,
        group_identifier: &str,
        valid_time: u64,
    ) -> ThsResult<serde_json::Value> {
        let target_id = self.find_group_id(group_identifier)?;
        let cookies = self.api.get_cookies();
        let userid = cookies
            .get("userid")
            .ok_or_else(|| api_error("分享分组", "当前 cookies 中缺少 userid"))?;

        let biz_suffix = target_id
            .split_once('_')
            .map(|(_, suffix)| suffix)
            .unwrap_or(&target_id);

        let payload = serde_json::json!({
            "biz": "selfstock",
            "valid_time": valid_time,
            "biz_key": format!("{}_{}", userid, biz_suffix),
            "name": group_identifier,
            "url_style": 0,
        });

        self.api.share_group(&payload)
    }

    // ════════════════════════════════
    // 内部工具
    // ════════════════════════════════

    /// 按名称或 ID 查找分组 ID。
    fn find_group_id(&self, identifier: &str) -> ThsResult<String> {
        // 先尝试在缓存中按名称查找
        for (name, group) in &self.groups_cache {
            if name == identifier || group.group_id == identifier {
                return Ok(group.group_id.clone());
            }
        }
        Err(api_error(
            "查找分组",
            format!("未能找到分组 '{}'", identifier),
        ))
    }

    /// 确保版本号可用，必要时从缓存推断。
    fn ensure_version(&self) -> String {
        self.current_version.clone().unwrap_or_else(|| "0".to_string())
    }

    /// 解析股票符号列表。
    fn parse_group_list(&self, raw: &serde_json::Value) -> Vec<serde_json::Value> {
        raw.get("groups")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    }

    /// 刷新 selfstock_detail 元数据（尽力而为，失败不抛出）。
    fn refresh_selfstock_detail(&mut self) -> Option<()> {
        use crate::config::{DEFAULT_USER_AGENT, SELFSTOCK_DETAIL_API_URL, SELFSTOCK_DETAIL_TIMEOUT};

        let cookies = self.api.get_cookies();
        let userid = cookies.get("userid")?;

        if userid.is_empty() {
            return None;
        }

        let url = format!("{}?reqtype=download&app_flag=0E&userid={}", SELFSTOCK_DETAIL_API_URL, userid);

        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());
        headers.insert("userid".to_string(), userid.clone());

        let response = crate::client::ApiClient::shared_get(
            &url,
            &[],
            cookies,
            Some(&headers),
            SELFSTOCK_DETAIL_TIMEOUT,
        )
        .ok()?;

        let text = response.text().ok()?;
        let xml = crate::utils::parse_ths_xml_response(&text, "selfstock_detail").ok()?;

        let version = extract_item_attr(&xml, "version");
        let detail_blob = extract_item_attr(&xml, "selfstock_detail").unwrap_or_default();

        if !detail_blob.is_empty() {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&detail_blob)
                .ok()?;
            let json_str = String::from_utf8(decoded).ok()?;
            if let Ok(detail_data) = serde_json::from_str::<Vec<serde_json::Value>>(&json_str) {
                let mut map = HashMap::new();
                for entry in detail_data {
                    let code = entry
                        .get("C")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let market = entry
                        .get("M")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if let (Some(c), Some(m)) = (code, market) {
                        map.insert((c, m), entry);
                    }
                }
                self.selfstock_detail_map = map;
                self.selfstock_detail_version = version;
            }
        }

        Some(())
    }

    /// 填充自选股元数据（价格、加入时间）。
    fn attach_selfstock_metadata(&self, items: &mut [StockItem]) {
        for item in items.iter_mut() {
            let market = item.market.clone().unwrap_or_default();
            if let Some(detail) = self.selfstock_detail_map.get(&(item.code.clone(), market)) {
                if item.price.is_none() {
                    item.price = detail
                        .get("P")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok());
                }
                if item.added_at.is_none() {
                    item.added_at = detail
                        .get("T")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
    }
}

/// 解析 "600519.SH" 或 "600519" 格式的股票代码。
///
/// # Returns
/// StockEntry 列表，market_type 使用数字代码。
pub fn parse_symbols(symbols: &[String]) -> Vec<StockEntry> {
    use crate::constant::market_code;

    symbols
        .iter()
        .filter_map(|s| {
            let s = s.trim().to_uppercase();
            if s.is_empty() {
                return None;
            }
            if let Some((code, market_abbr)) = s.split_once('.') {
                Some(StockEntry::new(code, market_code(market_abbr)))
            } else if s.chars().all(|c| c.is_ascii_digit()) {
                // 纯数字，默认上海
                Some(StockEntry::new(s, "17"))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_symbols_with_market() {
        let result = parse_symbols(&["600519.SH".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].code, "600519");
        assert_eq!(result[0].market_type, "17");
    }

    #[test]
    fn test_parse_symbols_no_market() {
        let result = parse_symbols(&["000001".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].code, "000001");
        assert_eq!(result[0].market_type, "17"); // 默认上海
    }

    #[test]
    fn test_parse_symbols_empty() {
        let result = parse_symbols(&[]);
        assert!(result.is_empty());
    }
}
