/// HTTP 客户端封装。
///
/// 对应 Python 项目 `client.py`，封装底层 HTTP 请求、Cookie 管理与通用错误处理。

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use log::{debug, info};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};

use crate::config::{DEFAULT_HTTP_TIMEOUT, DEFAULT_USER_AGENT};
use crate::errors::{network_error, ThsResult};

/// 全局共享的 HTTP 会话（用于协议级请求，连接池复用）。
#[allow(dead_code)]
pub static SHARED_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs_f64(DEFAULT_HTTP_TIMEOUT))
        .build()
        .expect("创建全局 HTTP 客户端失败")
});

/// HTTP 客户端，管理 Cookie、默认请求头和超时。
pub struct ApiClient {
    /// 内部 reqwest 阻塞客户端。
    client: Client,
    /// API 基础 URL。
    base_url: String,
    /// 当前 Cookie 存储。
    cookies: HashMap<String, String>,
    /// 默认请求头。
    #[allow(dead_code)]
    default_headers: HeaderMap,
    /// 超时时间（秒）。
    #[allow(dead_code)]
    timeout: f64,
}

impl ApiClient {
    /// 创建新的 ApiClient 实例。
    ///
    /// # Arguments
    /// * `base_url` - API 基础 URL。
    /// * `cookies` - 可选的初始 Cookie 字符串或映射表。
    /// * `timeout` - 超时时间（秒），默认 10 秒。
    pub fn new(
        base_url: impl Into<String>,
        cookies: Option<&HashMap<String, String>>,
        timeout: Option<f64>,
    ) -> Self {
        let timeout = timeout.unwrap_or(DEFAULT_HTTP_TIMEOUT);
        let client = Client::builder()
            .timeout(Duration::from_secs_f64(timeout))
            .build()
            .expect("创建 HTTP 客户端失败");

        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            USER_AGENT,
            HeaderValue::from_static(DEFAULT_USER_AGENT),
        );

        let cookies_map = cookies.cloned().unwrap_or_default();
        let base = base_url.into();

        debug!(
            "ApiClient 初始化: base_url='{}', timeout={}s",
            base,
            timeout
        );

        Self {
            client,
            base_url: base,
            cookies: cookies_map,
            default_headers,
            timeout,
        }
    }

    /// 设置 Cookie。
    #[allow(dead_code)]
    pub fn set_cookies(&mut self, cookies: &HashMap<String, String>) {
        self.cookies = cookies.clone();
        info!("客户端 cookies 已更新，共 {} 个。", self.cookies.len());
    }

    /// 获取当前 Cookie 的只读引用。
    pub fn get_cookies(&self) -> &HashMap<String, String> {
        &self.cookies
    }

    /// 发送 HTTP GET 请求。
    pub fn get(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> ThsResult<serde_json::Value> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path.trim_start_matches('/'));
        info!("发送 GET 请求到 {}", url);

        let req = self.client.get(&url).query(params);

        // 注入 Cookie
        let cookie_str = crate::cookie::cookies_to_header_string(&self.cookies);
        let req = if !cookie_str.is_empty() {
            req.header("Cookie", &cookie_str)
        } else {
            req
        };

        self.execute(req, &url)
    }

    /// 发送 application/x-www-form-urlencoded POST 请求。
    pub fn post_form_urlencoded(
        &self,
        path: &str,
        data: &[(&str, &str)],
    ) -> ThsResult<serde_json::Value> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path.trim_start_matches('/'));
        info!("发送 POST (form) 请求到 {}", url);

        let req = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded; charset=utf-8")
            .form(&data);

        let cookie_str = crate::cookie::cookies_to_header_string(&self.cookies);
        let req = if !cookie_str.is_empty() {
            req.header("Cookie", &cookie_str)
        } else {
            req
        };

        self.execute(req, &url)
    }

    /// 发送 application/json POST 请求。
    pub fn post_json(
        &self,
        path: &str,
        json_body: &serde_json::Value,
    ) -> ThsResult<serde_json::Value> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path.trim_start_matches('/'));
        info!("发送 POST (json) 请求到 {}", url);

        let req = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .json(json_body);

        let cookie_str = crate::cookie::cookies_to_header_string(&self.cookies);
        let req = if !cookie_str.is_empty() {
            req.header("Cookie", &cookie_str)
        } else {
            req
        };

        self.execute(req, &url)
    }

    /// 执行 HTTP 请求并解析 JSON 响应。
    fn execute(
        &self,
        req: reqwest::blocking::RequestBuilder,
        url: &str,
    ) -> ThsResult<serde_json::Value> {
        let response: Response = req.send().map_err(|e| {
            network_error(&format!("请求 {}", url), e.to_string())
        })?;

        let status = response.status();
        debug!("收到响应: 状态码 {}, URL: {}", status, url);

        if !status.is_success() {
            let preview = response
                .text()
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();
            return Err(network_error(
                &format!("请求 {}", url),
                format!("HTTP {}: {}", status.as_u16(), preview),
            ));
        }

        let text = response.text().map_err(|e| {
            network_error(&format!("请求 {}", url), format!("读取响应体失败: {}", e))
        })?;

        if text.trim().is_empty() {
            info!("请求 {} 成功，但响应体为空。返回空对象。", url);
            return Ok(serde_json::Value::Object(serde_json::Map::new()));
        }

        serde_json::from_str(&text).map_err(|e| {
            let preview = text.chars().take(200).collect::<String>();
            network_error(
                &format!("请求 {}", url),
                format!("JSON 解析失败: {}. 响应: {}", e, preview),
            )
        })
    }

    /// 发送任意 URL 的 GET 请求（不带 base_url 拼接）。
    ///
    /// 用于协议级请求，如 selfstock_v2 使用不同基础域名。
    #[allow(dead_code)]
    pub fn get_raw(
        &self,
        full_url: &str,
        params: &[(&str, &str)],
        headers: Option<&HashMap<String, String>>,
    ) -> ThsResult<Response> {
        info!("发送 GET 请求到 {}", full_url);

        let mut req = self.client.get(full_url).query(params);

        let cookie_str = crate::cookie::cookies_to_header_string(&self.cookies);
        if !cookie_str.is_empty() {
            req = req.header("Cookie", &cookie_str);
        }

        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        req.send().map_err(|e| network_error(&format!("请求 {}", full_url), e.to_string()))
    }

    /// 发送任意 URL 的 POST 请求（form-encoded，不带 base_url 拼接）。
    #[allow(dead_code)]
    pub fn post_form_raw(
        &self,
        full_url: &str,
        data: &[(&str, &str)],
        headers: Option<&HashMap<String, String>>,
    ) -> ThsResult<Response> {
        info!("发送 POST (form) 请求到 {}", full_url);

        let mut req = self
            .client
            .post(full_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(data);

        let cookie_str = crate::cookie::cookies_to_header_string(&self.cookies);
        if !cookie_str.is_empty() {
            req = req.header("Cookie", &cookie_str);
        }

        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        req.send()
            .map_err(|e| network_error(&format!("请求 {}", full_url), e.to_string()))
    }

    /// 使用共享会话发送 GET 请求（无 Cookie 注入，用于协议级请求直接传入 Cookie）。
    pub fn shared_get(
        url: &str,
        params: &[(&str, &str)],
        cookies: &HashMap<String, String>,
        headers: Option<&HashMap<String, String>>,
        timeout_secs: f64,
    ) -> ThsResult<reqwest::blocking::Response> {
        let client = Client::builder()
            .timeout(Duration::from_secs_f64(timeout_secs))
            .build()
            .map_err(|e| network_error("创建临时客户端", e.to_string()))?;

        let mut req = client.get(url).query(params);

        let cookie_str = crate::cookie::cookies_to_header_string(cookies);
        if !cookie_str.is_empty() {
            req = req.header("Cookie", &cookie_str);
        }

        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        req.send()
            .map_err(|e| network_error(&format!("请求 {}", url), e.to_string()))
    }

    /// 使用共享会话发送 POST 请求。
    pub fn shared_post(
        url: &str,
        data: &[(&str, &str)],
        cookies: &HashMap<String, String>,
        headers: Option<&HashMap<String, String>>,
        timeout_secs: f64,
    ) -> ThsResult<reqwest::blocking::Response> {
        let client = Client::builder()
            .timeout(Duration::from_secs_f64(timeout_secs))
            .build()
            .map_err(|e| network_error("创建临时客户端", e.to_string()))?;

        let mut req = client
            .post(url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(data);

        let cookie_str = crate::cookie::cookies_to_header_string(cookies);
        if !cookie_str.is_empty() {
            req = req.header("Cookie", &cookie_str);
        }

        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        req.send()
            .map_err(|e| network_error(&format!("请求 {}", url), e.to_string()))
    }
}
