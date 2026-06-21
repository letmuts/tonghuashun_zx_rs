/// 工具函数模块。
///
/// 包含同花顺 XML 响应解析等通用工具。

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::errors::{api_error, ThsResult};

/// 解析同花顺 XML 响应并检查业务错误码。
///
/// 格式示例：
/// ```xml
/// <root><ret code="0" msg="success"/><item .../></root>
/// ```
///
/// # Returns
/// 解析成功时返回 XML 字符串本身（供后续属性提取使用）。
pub fn parse_ths_xml_response(xml_text: &str, action_name: &str) -> ThsResult<String> {
    let mut reader = Reader::from_str(xml_text);
    reader.config_mut().trim_text(true);

    let mut ret_code = String::new();
    let mut ret_msg = String::new();
    let mut in_ret = false;

    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let tag_name = String::from_utf8_lossy(&name_bytes);
                if tag_name == "ret" {
                    in_ret = true;
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        let val = String::from_utf8_lossy(&attr.value);
                        match key.as_ref() {
                            "code" => ret_code = val.to_string(),
                            "msg" => ret_msg = val.to_string(),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                if String::from_utf8_lossy(e.name().as_ref()) == "ret" {
                    in_ret = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(api_error(action_name, format!("XML 解析失败: {}", e)));
            }
            _ => {}
        }

        if !ret_code.is_empty() && !in_ret {
            break;
        }
    }

    if ret_code != "0" {
        let msg = if ret_msg.is_empty() { "未知错误" } else { &ret_msg };
        return Err(api_error(
            action_name,
            format!("{} (code={})", msg, ret_code),
        ));
    }

    Ok(xml_text.to_string())
}

/// 从同花顺 XML 响应的 `<item>` 节点中提取属性值。
///
/// 使用简单的字符串搜索，兼容 auth 和 selfstock_detail 接口的不同结构。
pub fn extract_item_attr(xml_text: &str, attr_name: &str) -> Option<String> {
    let start_tag = xml_text.find("<item")?;
    let end_tag = xml_text[start_tag..].find('>')?;
    let item_str = &xml_text[start_tag..start_tag + end_tag + 1];

    let pattern = format!("{}=\"", attr_name);
    let attr_start = item_str.find(&pattern)?;
    let value_start = attr_start + pattern.len();
    let value_end = item_str[value_start..].find('"')?;

    Some(item_str[value_start..value_start + value_end].to_string())
}

/// 解析 passport 格式的键值对字符串。
///
/// 格式：`key1=value1|key2=value2|...`
pub fn parse_passport(passport_blob: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for chunk in passport_blob.split('|') {
        if chunk.is_empty() {
            continue;
        }
        if let Some((key, value)) = chunk.split_once('=') {
            out.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ths_xml_success() {
        let xml = r#"<root><ret code="0" msg="success"/><item pubkey="abc"/></root>"#;
        let result = parse_ths_xml_response(xml, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_ths_xml_error_code() {
        let xml = r#"<root><ret code="-1" msg="invalid"/></root>"#;
        let result = parse_ths_xml_response(xml, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_item_attr() {
        let xml = r#"<root><ret code="0"/><item pubkey="abc123" rsa_version="v5"/></root>"#;
        assert_eq!(extract_item_attr(xml, "pubkey").as_deref(), Some("abc123"));
        assert_eq!(
            extract_item_attr(xml, "rsa_version").as_deref(),
            Some("v5")
        );
        assert_eq!(extract_item_attr(xml, "nonexistent"), None);
    }

    #[test]
    fn test_parse_passport() {
        let blob = "userid=123|sessionid=abc|signvalid=xyz";
        let result = parse_passport(blob);
        assert_eq!(result.get("userid").unwrap(), "123");
        assert_eq!(result.get("sessionid").unwrap(), "abc");
        assert_eq!(result.get("signvalid").unwrap(), "xyz");
    }
}
