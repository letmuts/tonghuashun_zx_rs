/// 数据模型定义。
///
/// 对应 Python 项目 `models.py`，定义自选股票目、分组等核心数据结构。

use serde::{Deserialize, Serialize};

/// 单个自选股票目。
///
/// 包含股票代码、市场缩写及可选的加入价格和加入时间。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockItem {
    /// 股票代码，如 "600519"。
    pub code: String,
    /// 市场缩写，如 "SH"、"SZ"。存储时转为大写。
    pub market: Option<String>,
    /// 加入价格（来自 selfstock_detail 接口）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    /// 加入时间（来自 selfstock_detail 接口）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_at: Option<String>,
}

impl StockItem {
    /// 创建仅包含代码和市场的新条目。
    pub fn new(code: impl Into<String>, market: Option<impl Into<String>>) -> Self {
        Self {
            code: code.into(),
            market: market.map(|m| m.into().to_uppercase()),
            price: None,
            added_at: None,
        }
    }
}

impl std::fmt::Display for StockItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(ref m) = self.market {
            write!(f, ".{}", m)?;
        }
        Ok(())
    }
}

/// 自选股分组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockGroup {
    /// 分组名称。
    pub name: String,
    /// 分组 ID。
    pub group_id: String,
    /// 分组内的股票列表。
    pub items: Vec<StockItem>,
    /// 是否为只读分组（如动态板块）。
    #[serde(default)]
    pub readonly: bool,
}

impl StockGroup {
    /// 比较两个分组的差异。
    ///
    /// # Returns
    /// (新增条目列表, 移除条目列表)
    #[allow(dead_code)]
    pub fn diff(&self, other: &StockGroup) -> (Vec<StockItem>, Vec<StockItem>) {
        use std::collections::HashMap;
        let self_map: HashMap<(&str, Option<&str>), &StockItem> = self
            .items
            .iter()
            .map(|item| ((item.code.as_str(), item.market.as_deref()), item))
            .collect();
        let other_map: HashMap<(&str, Option<&str>), &StockItem> = other
            .items
            .iter()
            .map(|item| ((item.code.as_str(), item.market.as_deref()), item))
            .collect();

        let added: Vec<StockItem> = other_map
            .iter()
            .filter(|(k, _)| !self_map.contains_key(*k))
            .map(|(_, v)| (*v).clone())
            .collect();
        let removed: Vec<StockItem> = self_map
            .iter()
            .filter(|(k, _)| !other_map.contains_key(*k))
            .map(|(_, v)| (*v).clone())
            .collect();

        (added, removed)
    }
}

/// 原始 API 返回的股票条目（代码 + 数字市场类型码）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StockEntry {
    /// 股票代码。
    pub code: String,
    /// 同花顺数字市场类型码，如 "17" = 上海、"33" = 深圳。
    pub market_type: String,
}

impl StockEntry {
    pub fn new(code: impl Into<String>, market_type: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            market_type: market_type.into(),
        }
    }
}

/// v1 selfstock 查询结果：版本号 + 股票条目列表。
#[derive(Debug, Clone)]
pub struct StockListVersion {
    pub version: String,
    pub items: Vec<StockEntry>,
}

/// multiStorage blockstock 下载返回的单个分组条目。
#[derive(Debug, Clone)]
pub struct BlockstockGroup {
    pub group_name: String,
    pub group_type: i32,
    pub stock_list: Vec<StockEntry>,
}

/// multiStorage blockstock 下载结果。
#[derive(Debug, Clone)]
pub struct BlockstockDownload {
    #[allow(dead_code)]
    pub count: u64,
    pub version: u64,
    pub groups: Vec<BlockstockGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stock_item_new_uppercase_market() {
        let item = StockItem::new("600519", Some("sh"));
        assert_eq!(item.code, "600519");
        assert_eq!(item.market, Some("SH".to_string()));
    }

    #[test]
    fn test_stock_item_display() {
        let item = StockItem::new("000001", Some("sz"));
        assert_eq!(item.to_string(), "000001.SZ");
    }

    #[test]
    fn test_group_diff() {
        let group_a = StockGroup {
            name: "test".into(),
            group_id: "1".into(),
            items: vec![
                StockItem::new("A", Some("SH")),
                StockItem::new("B", Some("SH")),
            ],
            readonly: false,
        };
        let group_b = StockGroup {
            name: "test".into(),
            group_id: "1".into(),
            items: vec![
                StockItem::new("B", Some("SH")),
                StockItem::new("C", Some("SZ")),
            ],
            readonly: false,
        };
        let (added, removed) = group_a.diff(&group_b);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].code, "C");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].code, "A");
    }
}
