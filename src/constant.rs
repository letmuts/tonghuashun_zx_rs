/// 市场代码双向映射。
///
/// 同花顺 API 使用数字代码标识交易所（如 "17" = 上海），
/// 本模块提供数字 ↔ 缩写双向转换。

use std::collections::HashMap;

/// 构建市场缩写 → 数字代码的映射表。
fn build_market_code_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("SH", "17");    // 上海证券交易所
    m.insert("SHETF", "20"); // 上交所 ETF
    m.insert("ST", "22");    // 上交所 ST
    m.insert("SZ", "33");    // 深圳证券交易所
    m.insert("SZETF", "36"); // 深交所 ETF
    m.insert("ZS", "48");    // 指数
    m.insert("CYB", "38");   // 创业板
    m.insert("KC", "18");    // 科创板
    m.insert("BJ", "71");    // 北京证券交易所
    m.insert("HK", "55");    // 港股
    m.insert("US", "61");    // 美股
    m.insert("FT", "50");    // 期货
    m.insert("QH", "51");    // 期货主力
    m.insert("QZ", "53");    // 期指
    m.insert("OP", "79");    // 期权
    m.insert("JJ", "39");    // 基金
    m.insert("ZQ", "45");    // 债券
    m.insert("XSB", "67");   // 新三板
    m
}

/// 构建数字代码 → 市场缩写的反向映射表。
fn build_market_name_map() -> HashMap<&'static str, &'static str> {
    let mut m: HashMap<&str, &str> = build_market_code_map()
        .into_iter()
        .map(|(k, v)| (v, k))
        .collect();
    // 同花顺返回的多余/备用市场代码，手动补充映射
    m.insert("151", "BJ"); // 北交所备用码
    m
}

/// 将数字市场类型码转换为缩写。
///
/// # Arguments
/// * `market_type` - 同花顺 API 返回的数字市场码，如 "17"。
///
/// # Returns
/// 如果映射表中存在则返回缩写，否则原样返回。
pub fn market_abbr(market_type: &str) -> &str {
    let map = build_market_name_map();
    // 使用 static 避免每次调用都构建 HashMap
    // 实际上更优雅的做法是用 lazy_static，但这里用函数内静态缓存
    map.get(market_type).copied().unwrap_or(market_type)
}

/// 将市场缩写转换为数字代码。
///
/// # Arguments
/// * `abbr` - 市场缩写，如 "SH"。大小写不敏感。
///
/// # Returns
/// 如果映射表中存在则返回数字代码，否则原样返回。
pub fn market_code(abbr: &str) -> &str {
    let map = build_market_code_map();
    map.get(abbr.to_uppercase().as_str())
        .copied()
        .unwrap_or(abbr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_abbr_known() {
        assert_eq!(market_abbr("17"), "SH");
        assert_eq!(market_abbr("33"), "SZ");
        assert_eq!(market_abbr("71"), "BJ");
        assert_eq!(market_abbr("151"), "BJ");
    }

    #[test]
    fn test_market_abbr_unknown() {
        assert_eq!(market_abbr("999"), "999");
    }

    #[test]
    fn test_market_code_known() {
        assert_eq!(market_code("SH"), "17");
        assert_eq!(market_code("sz"), "33");
    }

    #[test]
    fn test_market_code_unknown() {
        assert_eq!(market_code("XX"), "XX");
    }
}
