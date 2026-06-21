/// 同花顺认证模块。
///
/// 实现完整的四步登录流程和策略化 Cookie 管理。
///
/// ## 登录流程
/// 1. 获取 RSA 公钥
/// 2. RSA 加密账号密码，调用统一登录
/// 3. 执行 mainverify 获取 signvalid
/// 4. 调用 docookie2.php 兑换最终 Cookie

use std::collections::HashMap;
use std::time::Duration;

use base64::Engine;
use log::{debug, warn};
use rsa::RsaPublicKey;

use crate::config::{COOKIE_CACHE_FILE, COOKIE_CACHE_TTL_SECONDS};
use crate::cookie::parse_cookie_header;
use crate::errors::{api_error, auth_error, network_error, ThsResult};
use crate::storage::{
    load_cookie_cache, read_cached_auth_params, read_cached_cookies, write_cookie_cache,
};
use crate::utils::{extract_item_attr, parse_passport, parse_ths_xml_response};

// ── 认证常量 ──

const AUTH_BASE: &str = "https://auth.10jqka.com.cn";
const UPASS_BASE: &str = "https://upass.10jqka.com.cn";
const DOC_COOKIE_PATH: &str = "/docookie2.php";
const AUTH_USER_AGENT: &str =
    "同花顺/7.0.10 CFNetwork/1333.0.4 Darwin/21.5.0";
const IMEI_ENCODED: &str = "ZjI6MDY6NGE6NzI6MjQ6NTA=";
const QSID: &str = "8003";
const PRODUCT: &str = "S01";
const SECURITIES: &str = "%E5%90%8C%E8%8A%B1%E9%A1%BA%E8%BF%9C%E8%88%AA%E7%89%88";
const RSA_VERSION_FALLBACK: &str = "default_5";
const TA_APP_ID: &str = "2022021114090152";
const REQUEST_TIMEOUT: f64 = 10.0;

// ── 数据结构 ──

/// 完整登录流程的结果。
#[derive(Debug, Clone)]
pub struct SessionResult {
    pub userid: String,
    pub sessionid: String,
    #[allow(dead_code)]
    pub signvalid: String,
    pub cookies: HashMap<String, String>,
}

/// RSA 信息。
struct RsaInfo {
    pubkey: String,
    rsa_version: String,
}

/// 登录中间产物。
struct LoginBundle {
    userid: String,
    sessionid: String,
    account: String,
    rsa_version: String,
}

// ── SessionClient: 底层登录实现 ──

/// 底层会话客户端，负责执行完整的四步登录流程。
pub struct SessionClient {
    username: String,
    password: String,
    auth_base: String,
    upass_base: String,
    #[allow(dead_code)]
    timeout: f64,
    http: reqwest::blocking::Client,
}

impl SessionClient {
    /// 创建新的 SessionClient。
    pub fn new(username: &str, password: &str) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs_f64(REQUEST_TIMEOUT))
            .build()
            .expect("创建认证 HTTP 客户端失败");

        Self {
            username: username.to_string(),
            password: password.to_string(),
            auth_base: AUTH_BASE.to_string(),
            upass_base: UPASS_BASE.to_string(),
            timeout: REQUEST_TIMEOUT,
            http,
        }
    }

    /// 执行完整登录流程。
    pub fn create_session(&self) -> ThsResult<SessionResult> {
        let rsa_info = self.fetch_rsa_info()?;
        debug!("RSA 公钥获取成功, rsa_version={}", rsa_info.rsa_version);

        let login_bundle = self.login(&rsa_info)?;
        debug!(
            "统一登录成功, userid={}, account={}",
            login_bundle.userid, login_bundle.account
        );

        let signvalid = self.fetch_signvalid(&login_bundle)?;
        debug!("mainverify 成功, signvalid={}", signvalid);

        let cookies = self.fetch_cookies(&login_bundle.userid, &login_bundle.sessionid, &signvalid)?;
        debug!("Cookie 兑换成功, 共 {} 个", cookies.len());

        Ok(SessionResult {
            userid: login_bundle.userid,
            sessionid: login_bundle.sessionid,
            signvalid,
            cookies,
        })
    }

    /// 步骤 1：获取 RSA 公钥。
    fn fetch_rsa_info(&self) -> ThsResult<RsaInfo> {
        let url = format!("{}/verify2", self.auth_base);
        let params = [("reqtype", "do_rsa"), ("type", "get_pubkey")];

        let xml_text = self.get_xml(&url, &params, "RSA key fetch")?;

        let pubkey = extract_item_attr(&xml_text, "pubkey")
            .ok_or_else(|| api_error("RSA key fetch", "响应缺少 pubkey 属性"))?;

        let rsa_version = extract_item_attr(&xml_text, "rsa_version")
            .unwrap_or_else(|| RSA_VERSION_FALLBACK.to_string());

        Ok(RsaInfo {
            pubkey,
            rsa_version,
        })
    }

    /// 步骤 2：RSA 加密后统一登录。
    fn login(&self, rsa_info: &RsaInfo) -> ThsResult<LoginBundle> {
        let encrypted_account = encrypt_with_rsa(&rsa_info.pubkey, &self.username)?;
        let encrypted_password = encrypt_with_rsa(&rsa_info.pubkey, &self.password)?;

        let url = format!("{}/verify2", self.auth_base);
        let params = [
            ("account", encrypted_account.as_str()),
            ("msg", "1"),
            ("passwd", encrypted_password.as_str()),
            ("reqtype", "unified_login"),
            (
                "rsa_version",
                if rsa_info.rsa_version.is_empty() {
                    RSA_VERSION_FALLBACK
                } else {
                    &rsa_info.rsa_version
                },
            ),
            ("ta_appid", TA_APP_ID),
        ];

        let xml_text = self.get_xml(&url, &params, "Login")?;

        let userid = extract_item_attr(&xml_text, "userid")
            .ok_or_else(|| api_error("Login", "响应缺少 userid 属性"))?;
        let sessionid = extract_item_attr(&xml_text, "sessionid")
            .ok_or_else(|| api_error("Login", "响应缺少 sessionid 属性"))?;
        let account = extract_item_attr(&xml_text, "account")
            .ok_or_else(|| api_error("Login", "响应缺少 account 属性"))?;
        let rsa_version = extract_item_attr(&xml_text, "rsa_version")
            .unwrap_or_else(|| rsa_info.rsa_version.clone());

        Ok(LoginBundle {
            userid,
            sessionid,
            account,
            rsa_version,
        })
    }

    /// 步骤 3：mainverify 获取 signvalid。
    fn fetch_signvalid(&self, bundle: &LoginBundle) -> ThsResult<String> {
        let url = format!("{}/verify2", self.auth_base);
        let params = [
            ("reqtype", "mainverify"),
            ("userid", &bundle.userid),
            ("sessionid", &bundle.sessionid),
            ("qsid", QSID),
            ("product", PRODUCT),
            ("version", "11.4.1.3"),
            ("imei", IMEI_ENCODED),
            ("sdsn", ""),
            (
                "rsa_version",
                if bundle.rsa_version.is_empty() {
                    RSA_VERSION_FALLBACK
                } else {
                    &bundle.rsa_version
                },
            ),
            ("nohqlist", "0"),
            ("securities", SECURITIES),
        ];

        let xml_text = self.get_xml(&url, &params, "Mainverify")?;

        let passport_blob = extract_item_attr(&xml_text, "passport")
            .ok_or_else(|| api_error("Mainverify", "响应缺少 passport 数据"))?;

        let passport_map = parse_passport(&passport_blob);

        passport_map
            .get("signvalid")
            .cloned()
            .ok_or_else(|| api_error("Mainverify", "signvalid 不在 passport 中"))
    }

    /// 步骤 4：用三元组兑换最终 Cookie。
    fn fetch_cookies(&self, userid: &str, sessionid: &str, signvalid: &str) -> ThsResult<HashMap<String, String>> {
        let url = format!("{}{}", self.upass_base, DOC_COOKIE_PATH);
        let params = [
            ("userid", userid),
            ("sessionid", sessionid),
            ("signvalid", signvalid),
        ];

        let response = self
            .http
            .get(&url)
            .query(&params)
            .header("User-Agent", AUTH_USER_AGENT)
            .send()
            .map_err(|e| network_error("docookie2.php", e.to_string()))?;

        if !response.status().is_success() {
            return Err(network_error(
                "docookie2.php",
                format!("HTTP {}", response.status().as_u16()),
            ));
        }

        // 尝试从 response cookies 字典获取
        let mut cookies = HashMap::new();
        for cookie in response.cookies() {
            cookies.insert(cookie.name().to_string(), cookie.value().to_string());
        }

        if cookies.is_empty() {
            // Fallback: 从 Set-Cookie 头解析
            if let Some(header_val) = response.headers().get("Set-Cookie") {
                if let Ok(header_str) = header_val.to_str() {
                    cookies = parse_cookie_header(header_str);
                }
            }
        }

        if cookies.is_empty() {
            return Err(api_error("docookie2.php", "返回的 Cookie 为空"));
        }

        Ok(cookies)
    }

    /// 发送 GET 请求并解析 XML 响应。
    fn get_xml(&self, url: &str, params: &[(&str, &str)], action: &str) -> ThsResult<String> {
        let response = self
            .http
            .get(url)
            .query(params)
            .header("User-Agent", AUTH_USER_AGENT)
            .send()
            .map_err(|e| network_error(action, e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let preview = response
                .text()
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();
            return Err(network_error(
                action,
                format!("HTTP {}: {}", status.as_u16(), preview),
            ));
        }

        let text = response.text().map_err(|e| network_error(action, e.to_string()))?;
        parse_ths_xml_response(&text, action)
    }
}

// ── RSA 加密工具 ──

/// 使用 RSA 公钥 (PKCS1v15) 加密明文。
fn encrypt_with_rsa(pubkey_pem: &str, value: &str) -> ThsResult<String> {
    use pkcs8::DecodePublicKey;

    let public_key = RsaPublicKey::from_public_key_pem(pubkey_pem)
        .map_err(|e| auth_error(format!("RSA 公钥解析失败: {}", e)))?;

    let mut rng = rand::thread_rng();
    let encrypted = public_key
        .encrypt(&mut rng, rsa::Pkcs1v15Encrypt, value.as_bytes())
        .map_err(|e| auth_error(format!("RSA 加密失败: {}", e)))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&encrypted))
}

// ── SessionManager: 策略化 Cookie 管理 ──

/// Cookie 策略管理。
///
/// 按优先级处理多种认证方式：
/// 1. 显式传入 Cookie
/// 2. 提供 username + password → 执行登录
/// 3. 仅提供 username → 尝试读取缓存
/// 4. 无参数 → 读取最近有效缓存
pub struct SessionManager {
    explicit_cookies: Option<HashMap<String, String>>,
    username: Option<String>,
    password: Option<String>,
    cookie_cache_path: String,
    cookie_cache_ttl: u64,
    resolved_cache: Option<HashMap<String, String>>,
    last_session_result: Option<SessionResult>,
}

impl SessionManager {
    /// 创建新的 SessionManager。
    pub fn new(
        cookies: Option<&HashMap<String, String>>,
        username: Option<&str>,
        password: Option<&str>,
        cookie_cache_path: Option<&str>,
        cookie_cache_ttl: Option<u64>,
    ) -> Self {
        Self {
            explicit_cookies: cookies.cloned(),
            username: username.map(|s| s.to_string()),
            password: password.map(|s| s.to_string()),
            cookie_cache_path: cookie_cache_path
                .unwrap_or(COOKIE_CACHE_FILE)
                .to_string(),
            cookie_cache_ttl: cookie_cache_ttl.unwrap_or(COOKIE_CACHE_TTL_SECONDS),
            resolved_cache: None,
            last_session_result: None,
        }
    }

    /// 解析 Cookie，按优先级返回最终可用的 Cookie。
    pub fn resolve(&mut self) -> Option<HashMap<String, String>> {
        if let Some(ref cookies) = self.explicit_cookies {
            return Some(cookies.clone());
        }

        if self.resolved_cache.is_none() {
            self.resolved_cache = self.resolve_from_inputs();
        }

        self.resolved_cache.clone()
    }

    /// 获取 multiStorage 协议所需的 auth_params。
    #[allow(dead_code)]
    pub fn get_auth_params(&self) -> Option<HashMap<String, String>> {
        if let Some(ref sr) = self.last_session_result {
            let expires = system_time_expires();
            return Some({
                let mut m = HashMap::new();
                m.insert("userid".to_string(), sr.userid.clone());
                m.insert("sessionid".to_string(), sr.sessionid.clone());
                m.insert("expires".to_string(), expires);
                m
            });
        }

        // 从缓存查找 auth_params
        if let Some(username) = &self.username {
            let key = credentials_cache_key(username);
            let cached = read_cached_auth_params(
                &self.cookie_cache_path,
                &key,
                self.cookie_cache_ttl,
            );
            if let Some(params) = cached {
                return Some(params);
            }
        }

        // 从缓存文件查找最近有效的 auth_params
        let cache_data = load_cookie_cache(&self.cookie_cache_path);
        let now = current_timestamp();
        let mut latest_ts = 0.0_f64;
        let mut latest_params: Option<HashMap<String, String>> = None;

        for (cache_key, entry) in &cache_data {
            if !cache_key.starts_with("credentials::") {
                continue;
            }
            let ts = entry
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if now - ts > self.cookie_cache_ttl as f64 {
                continue;
            }
            if let Some(ap) = entry.get("auth_params") {
                if let Some(obj) = ap.as_object() {
                    if !obj.is_empty() && ts > latest_ts {
                        latest_ts = ts;
                        let params: HashMap<String, String> = obj
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                            .collect();
                        latest_params = Some(params);
                    }
                }
            }
        }

        if let Some(params) = latest_params {
            return Some(params);
        }

        // 最后的 fallback：从 cookies 的 user 字段提取
        self.extract_auth_from_cookies(&cache_data)
    }

    #[allow(dead_code)]
    fn extract_auth_from_cookies(
        &self,
        cache_data: &HashMap<String, serde_json::Value>,
    ) -> Option<HashMap<String, String>> {
        use crate::protocol::blockstock::extract_auth_params_from_cookies;

        for entry in cache_data.values() {
            let cookies = entry.get("cookies").and_then(|v| v.as_object());
            if let Some(cookies_obj) = cookies {
                let cookies_map: HashMap<String, String> = cookies_obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect();
                let userid = cookies_map.get("userid").cloned().unwrap_or_default();
                if userid.is_empty() {
                    continue;
                }
                let result = extract_auth_params_from_cookies(&cookies_map);
                if !result.get("userid").map_or(true, |v| v.is_empty()) {
                    return Some(result);
                }
            }
        }

        None
    }

    fn resolve_from_inputs(&mut self) -> Option<HashMap<String, String>> {
        if self.username.is_none() && self.password.is_none() {
            return self.read_latest_cached_cookies();
        }
        self.resolve_credentials_flow()
    }

    fn resolve_credentials_flow(&mut self) -> Option<HashMap<String, String>> {
        let username_opt = self.username.clone();
        let password_opt = self.password.clone();

        if let (Some(username), Some(password)) = (&username_opt, &password_opt) {
            let cache_key = credentials_cache_key(username);
            let u = username.clone();
            let p = password.clone();
            // Inline fetch_with_cache to avoid closure borrow issues
            let cached = read_cached_cookies(&self.cookie_cache_path, &cache_key, self.cookie_cache_ttl);
            if let Some(cookies) = cached {
                return Some(cookies);
            }
            let fresh = self.load_from_credentials(&u, &p);
            if let Some(cookies) = &fresh {
                let _ = write_cookie_cache(&self.cookie_cache_path, &cache_key, cookies, None);
            }
            return fresh;
        }

        if let Some(username) = &username_opt {
            let cache_key = credentials_cache_key(username);
            let cached = read_cached_cookies(
                &self.cookie_cache_path,
                &cache_key,
                self.cookie_cache_ttl,
            );
            if let Some(cookies) = cached {
                return Some(cookies);
            }
            warn!(
                "未找到用户 '{}' 的凭据缓存，请同时提供密码。",
                username
            );
            return None;
        }

        None
    }

    #[allow(dead_code)]
    fn fetch_with_cache<F>(&mut self, cache_key: &str, loader: F) -> Option<HashMap<String, String>>
    where
        F: FnOnce() -> Option<HashMap<String, String>>,
    {
        let cached = read_cached_cookies(
            &self.cookie_cache_path,
            cache_key,
            self.cookie_cache_ttl,
        );
        if let Some(cookies) = cached {
            return Some(cookies);
        }

        let fresh = loader();
        if let Some(ref cookies) = fresh {
            let _ = write_cookie_cache(
                &self.cookie_cache_path,
                cache_key,
                cookies,
                None,
            );
        }
        fresh
    }

    fn load_from_credentials(
        &mut self,
        username: &str,
        password: &str,
    ) -> Option<HashMap<String, String>> {
        let client = SessionClient::new(username, password);
        let session = match client.create_session() {
            Ok(s) => s,
            Err(e) => {
                warn!("登录失败: {}", e);
                return None;
            }
        };

        let cache_key = credentials_cache_key(username);

        let expires = system_time_expires();
        let auth_params: HashMap<String, String> = {
            let mut m = HashMap::new();
            m.insert("userid".to_string(), session.userid.clone());
            m.insert("sessionid".to_string(), session.sessionid.clone());
            m.insert("expires".to_string(), expires);
            m
        };

        let _ = write_cookie_cache(
            &self.cookie_cache_path,
            &cache_key,
            &session.cookies,
            Some(&auth_params),
        );

        self.last_session_result = Some(session.clone());
        Some(session.cookies)
    }

    fn read_latest_cached_cookies(&self) -> Option<HashMap<String, String>> {
        let cache_data = load_cookie_cache(&self.cookie_cache_path);
        let now = current_timestamp();
        let mut latest_ts: Option<f64> = None;
        let mut latest_cookies: Option<HashMap<String, String>> = None;

        for (cache_key, entry) in &cache_data {
            if !cache_key.starts_with("credentials::") {
                continue;
            }
            let ts = entry
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if now - ts > self.cookie_cache_ttl as f64 {
                continue;
            }
            let cookies_payload = entry.get("cookies").and_then(|v| v.as_object());
            if let Some(obj) = cookies_payload {
                let cookies: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect();
                if !cookies.is_empty() {
                    if latest_ts.is_none() || ts > latest_ts.unwrap() {
                        latest_ts = Some(ts);
                        latest_cookies = Some(cookies);
                    }
                }
            }
        }

        latest_cookies
    }
}

// ── 工具函数 ──

/// 根据用户名生成缓存 key（SHA-256）。
fn credentials_cache_key(username: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(username.as_bytes());
    format!("credentials::{}", hex::encode(digest))
}

/// 获取当前 Unix 时间戳（秒）。
fn current_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// 生成明天此时的时间字符串，用于 expires 字段。
fn system_time_expires() -> String {
    let now = std::time::SystemTime::now();
    let expires = now + Duration::from_secs(86400);
    // Windows 不支持 SystemTime 直接格式化，用简单的偏移计算
    let dur = expires
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    // 用简单算法：从 Unix epoch 开始计算年月日时分秒
    let days = total_secs / 86400;
    // 简化为可读的日期时间
    // 使用标准库的安全方法
    let secs_of_day = total_secs % 86400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;
    let seconds = secs_of_day % 60;

    // 计算年月日（简化算法，近似值，适用于 2020-2099）
    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// 将距 Unix epoch 的天数转换为 (年, 月, 日)。
fn days_to_date(mut days: u64) -> (u64, u64, u64) {
    // 从 1970 年 1 月 1 日开始
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ── 便捷登录函数 ──

/// 使用用户名密码执行登录并返回 SessionResult。
#[allow(dead_code)]
pub fn create_session(username: &str, password: &str) -> ThsResult<SessionResult> {
    let client = SessionClient::new(username, password);
    client.create_session()
}
