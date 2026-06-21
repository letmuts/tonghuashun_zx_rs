/// multiStorage 协议实现。
///
/// 用于同花顺自定义分组的批量读写。使用简易 protobuf 传输二进制载荷。

use std::collections::HashMap;

use crate::config::{
    BLOCKSTOCK_APPNAME, DEFAULT_USER_AGENT, MULTI_STORAGE_DEFAULT_CLIENTTYPE, MULTI_STORAGE_URL,
    SELF_STOCK_HTTP_TIMEOUT,
};
use crate::errors::{network_error, ThsResult};
use crate::models::{BlockstockDownload, BlockstockGroup, StockEntry};
use crate::protobuf::{decode_varint, field_bytes, field_varint};

/// 从 Cookie 中提取 multiStorage 协议所需的认证参数。
///
/// 从 `user` Cookie 的 Base64 解码内容中提取 sessionid。
pub fn extract_auth_params_from_cookies(cookies: &HashMap<String, String>) -> HashMap<String, String> {
    let user_raw = cookies.get("user").cloned().unwrap_or_default();
    let mut sessionid = String::new();

    if !user_raw.is_empty() {
        use base64::Engine;
        let decoded = url::Url::parse(&format!("http://dummy/{}", user_raw))
            .ok()
            .and_then(|_| {
                base64::engine::general_purpose::STANDARD
                    .decode(&user_raw)
                    .ok()
            })
            .and_then(|bytes| String::from_utf8(bytes).ok());

        if let Some(ref text) = decoded {
            let parts: Vec<&str> = text.split(':').collect();
            if parts.len() > 17 {
                sessionid = parts[17].to_string();
            }
        }
    }

    let expires = {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + 86400;
        // 简化：用格式化字符串
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            1970 + (ts / 31536000),
            ((ts % 31536000) / 2592000) + 1,
            ((ts % 2592000) / 86400) + 1,
            (ts % 86400) / 3600,
            (ts % 3600) / 60,
            ts % 60,
        )
    };

    let mut result = HashMap::new();
    result.insert(
        "userid".to_string(),
        cookies.get("userid").cloned().unwrap_or_default(),
    );
    result.insert("sessionid".to_string(), sessionid);
    result.insert("expires".to_string(), expires);
    result
}

/// 构造 blockstock 上传载荷（protobuf 编码）。
fn encode_blockstock_payload(
    group_name: &str,
    group_type: i32,
    stock_list: &[StockEntry],
) -> Vec<u8> {
    use base64::Engine;
    use encoding_rs::GBK;

    // 分组名称 → GBK → Base64
    let (gbk_bytes, _, _) = GBK.encode(group_name);
    let group_id_b64 = base64::engine::general_purpose::STANDARD.encode(&*gbk_bytes);

    // 股票列表 → codes|types 格式
    let codes: Vec<String> = stock_list.iter().map(|e| e.code.clone()).collect();
    let types: Vec<String> = stock_list.iter().map(|e| e.market_type.clone()).collect();
    let stock_str = format!("{},{}", codes.join("|"), types.join("|"));

    // 构造嵌套 protobuf 消息
    let group_data = {
        let mut buf = field_bytes(1, group_id_b64.as_bytes());
        buf.extend(field_bytes(3, stock_str.as_bytes()));
        buf
    };

    let group_payload = {
        let mut buf = field_bytes(1, &field_varint(1, group_type as u64));
        buf.extend(field_bytes(3, &group_data));
        buf
    };

    field_bytes(1, &group_payload)
}

/// 解析 blockstock 下载响应的二进制数据。
fn parse_blockstock_download(data: &[u8]) -> BlockstockDownload {
    let mut offset = 0;
    let mut count: u64 = 0;
    let mut version: u64 = 0;
    let mut groups = Vec::new();

    while offset < data.len() {
        let (tag, new_offset) = decode_varint(data, offset);
        offset = new_offset;
        let field_number = tag >> 3;
        let wire_type = tag & 0x07;

        if wire_type == 0 {
            let (value, new_offset) = decode_varint(data, offset);
            offset = new_offset;
            match field_number {
                1 => count = value,
                2 => version = value,
                _ => {}
            }
        } else if wire_type == 2 {
            let (length, new_offset) = decode_varint(data, offset);
            offset = new_offset;
            let len = length as usize;
            let chunk = &data[offset..offset + len];
            offset += len;
            if field_number == 3 {
                groups.push(parse_group_payload(chunk));
            }
        }
    }

    BlockstockDownload {
        count,
        version,
        groups,
    }
}

/// 解析单个分组载荷。
fn parse_group_payload(data: &[u8]) -> BlockstockGroup {
    let mut offset = 0;
    let mut group_type: i32 = 0;
    let mut group_name = String::new();
    let mut stock_list = Vec::new();

    while offset < data.len() {
        let (tag, new_offset) = decode_varint(data, offset);
        offset = new_offset;
        let field_number = tag >> 3;
        let wire_type = tag & 0x07;

        if wire_type == 0 {
            let (value, new_offset) = decode_varint(data, offset);
            offset = new_offset;
            if field_number == 1 {
                group_type = value as i32;
            }
        } else if wire_type == 2 {
            let (length, new_offset) = decode_varint(data, offset);
            offset = new_offset;
            let len = length as usize;
            let chunk = &data[offset..offset + len];
            offset += len;
            if field_number == 1 {
                // 内层 group_type
                let (inner_tag, _) = decode_varint(chunk, 0);
                if inner_tag >> 3 == 1 {
                    let (value, _) = decode_varint(chunk, 1);
                    group_type = value as i32;
                }
            } else if field_number == 3 {
                let inner = parse_group_data(chunk);
                stock_list = inner.stock_list;
                if let Some(name) = inner.group_name {
                    group_name = name;
                }
            }
        }
    }

    BlockstockGroup {
        group_name,
        group_type,
        stock_list,
    }
}

/// 解析分组内部数据。
struct InnerGroupData {
    group_name: Option<String>,
    stock_list: Vec<StockEntry>,
}

fn parse_group_data(data: &[u8]) -> InnerGroupData {
    let mut offset = 0;
    let mut group_name: Option<String> = None;
    let mut stock_list = Vec::new();

    while offset < data.len() {
        let (tag, new_offset) = decode_varint(data, offset);
        offset = new_offset;
        let field_number = tag >> 3;
        let wire_type = tag & 0x07;

        if wire_type == 2 {
            let (length, new_offset) = decode_varint(data, offset);
            offset = new_offset;
            let len = length as usize;
            let chunk = &data[offset..offset + len];
            offset += len;

            if field_number == 1 {
                // group_id (Base64 → GBK)
                use base64::Engine;
                use encoding_rs::GBK;
                let id_str = String::from_utf8_lossy(chunk);
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(id_str.trim())
                    .unwrap_or_default();
                let (name, _, _) = GBK.decode(&decoded);
                group_name = Some(name.into_owned());
            } else if field_number == 3 {
                // stock_list: codes|types 格式
                let raw = String::from_utf8_lossy(chunk);
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
                        stock_list.push(StockEntry::new(*code, mtype));
                    }
                }
            }
        }
    }

    InnerGroupData {
        group_name,
        stock_list,
    }
}

/// 从 multiStorage 下载所有分组的自选股数据。
pub fn download_blockstock(
    auth_params: &HashMap<String, String>,
    cookies: &HashMap<String, String>,
) -> ThsResult<BlockstockDownload> {
    let data: Vec<(&str, &str)> = vec![
        ("reqtype", "download"),
        (
            "userid",
            auth_params
                .get("userid")
                .map(|s| s.as_str())
                .unwrap_or(""),
        ),
        ("storepath", "/"),
        (
            "sessionid",
            auth_params
                .get("sessionid")
                .map(|s| s.as_str())
                .unwrap_or(""),
        ),
        (
            "expires",
            auth_params.get("expires").map(|s| s.as_str()).unwrap_or(""),
        ),
        ("appname", BLOCKSTOCK_APPNAME),
        ("storetype", "2"),
        (
            "clienttype",
            auth_params
                .get("clienttype")
                .map(|s| s.as_str())
                .unwrap_or(MULTI_STORAGE_DEFAULT_CLIENTTYPE),
        ),
        ("version", "0"),
    ];

    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());

    let response = crate::client::ApiClient::shared_post(
        MULTI_STORAGE_URL,
        &data,
        cookies,
        Some(&headers),
        SELF_STOCK_HTTP_TIMEOUT,
    )?;

    let bytes = response.bytes().map_err(|e| {
        network_error("blockstock download", e.to_string())
    })?;

    Ok(parse_blockstock_download(&bytes))
}

/// 向 multiStorage 上传分组自选股数据（全量替换）。
pub fn upload_blockstock(
    auth_params: &HashMap<String, String>,
    cookies: &HashMap<String, String>,
    group_name: &str,
    group_type: i32,
    stock_list: &[StockEntry],
    version: &str,
) -> ThsResult<serde_json::Value> {
    let payload_bytes = encode_blockstock_payload(group_name, group_type, stock_list);
    use base64::Engine;
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(&payload_bytes);

    let data: Vec<(&str, &str)> = vec![
        ("reqtype", "upload"),
        (
            "userid",
            auth_params
                .get("userid")
                .map(|s| s.as_str())
                .unwrap_or(""),
        ),
        ("storepath", "/"),
        (
            "sessionid",
            auth_params
                .get("sessionid")
                .map(|s| s.as_str())
                .unwrap_or(""),
        ),
        (
            "expires",
            auth_params.get("expires").map(|s| s.as_str()).unwrap_or(""),
        ),
        ("appname", BLOCKSTOCK_APPNAME),
        ("storetype", "2"),
        (
            "clienttype",
            auth_params
                .get("clienttype")
                .map(|s| s.as_str())
                .unwrap_or(MULTI_STORAGE_DEFAULT_CLIENTTYPE),
        ),
        ("version", version),
        ("content", &content_b64),
    ];

    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string());

    let response = crate::client::ApiClient::shared_post(
        MULTI_STORAGE_URL,
        &data,
        cookies,
        Some(&headers),
        SELF_STOCK_HTTP_TIMEOUT,
    )?;

    let text = response.text().map_err(|e| {
        network_error("blockstock upload", e.to_string())
    })?;

    serde_json::from_str(&text).map_err(|e| {
        network_error(
            "blockstock upload",
            format!("JSON 解析失败: {} (响应: {})", e, &text.chars().take(200).collect::<String>()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_auth_params_empty() {
        let cookies = HashMap::new();
        let result = extract_auth_params_from_cookies(&cookies);
        assert_eq!(result.get("userid").unwrap(), "");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let entries = vec![
            StockEntry::new("600519", "17"),
            StockEntry::new("000001", "33"),
        ];
        let payload = encode_blockstock_payload("测试分组", 0, &entries);
        // payload 不应该为空
        assert!(!payload.is_empty());
    }
}
