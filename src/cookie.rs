/// Cookie 字符串解析工具。
///
/// 支持解析 HTTP Cookie 头（`name=value; ...`）和 Set-Cookie 头中的键值对。

use std::collections::HashMap;

/// 解析 `Cookie` 请求头格式的字符串。
///
/// 输入格式如 `"userid=123; sessionid=abc"`，以分号分隔。
///
/// # Arguments
/// * `raw` - 原始 Cookie 字符串。
///
/// # Returns
/// Cookie 键值对映射表。
pub fn parse_cookie_string(raw: &str) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    if raw.is_empty() {
        return cookies;
    }
    for pair_str in raw.split(';') {
        let pair_str = pair_str.trim();
        if pair_str.is_empty() {
            continue;
        }
        if let Some((name, value)) = pair_str.split_once('=') {
            cookies.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    cookies
}

/// 解析 `Set-Cookie` 响应头格式的字符串。
///
/// 输入格式中可能包含逗号分隔的多个 cookie，
/// 本函数取每个段落的第一个 `name=value` 对。
///
/// # Arguments
/// * `header` - 原始 Set-Cookie 头值。
///
/// # Returns
/// Cookie 键值对映射表。
pub fn parse_cookie_header(header: &str) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    if header.is_empty() {
        return cookies;
    }
    for part in header.split(',') {
        let segment = part.trim();
        if segment.is_empty() {
            continue;
        }
        // 取第一个 ; 之前的部分作为键值对
        let pair = segment.split(';').next().unwrap_or(segment).trim();
        if let Some((name, value)) = pair.split_once('=') {
            cookies.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    cookies
}

/// 将 Cookie 映射表序列化为 `Cookie` 请求头格式的字符串。
pub fn cookies_to_header_string(cookies: &HashMap<String, String>) -> String {
    cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cookie_string_empty() {
        assert!(parse_cookie_string("").is_empty());
    }

    #[test]
    fn test_parse_cookie_string_normal() {
        let result = parse_cookie_string("userid=123; sessionid=abc");
        assert_eq!(result.get("userid").unwrap(), "123");
        assert_eq!(result.get("sessionid").unwrap(), "abc");
    }

    #[test]
    fn test_parse_cookie_header_set_cookie() {
        let result =
            parse_cookie_header("userid=123; Path=/, sessionid=abc; HttpOnly");
        assert_eq!(result.get("userid").unwrap(), "123");
        assert_eq!(result.get("sessionid").unwrap(), "abc");
    }

    #[test]
    fn test_cookies_to_header_string() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), "1".to_string());
        map.insert("b".to_string(), "2".to_string());
        let s = cookies_to_header_string(&map);
        // 注意 HashMap 迭代顺序不保证
        assert!(s.contains("a=1"));
        assert!(s.contains("b=2"));
    }
}
