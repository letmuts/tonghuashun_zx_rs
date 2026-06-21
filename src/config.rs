/// 全局配置常量。
///
/// 对应 Python 项目中的 `config.py`，集中管理 API 端点、缓存路径、超时等设置。

// ── HTTP ──

/// 默认 User-Agent，模拟同花顺移动客户端。
pub const DEFAULT_USER_AGENT: &str =
    "Hexin_Gphone/11.28.03 (Royal Flush) hxtheme/0 innerversion/G037.09.028.1.32 \
     followPhoneSystemTheme/0 userid/000000000 getHXAPPAccessibilityMode/0 \
     hxNewFont/1 isVip/0 getHXAPPFontSetting/normal getHXAPPAdaptOldSetting/0 okhttp/3.14.9";

/// 默认 HTTP 超时时间（秒）。
pub const DEFAULT_HTTP_TIMEOUT: f64 = 10.0;

// ── API 端点 ──

/// API 基础 URL（ugc）。
pub const API_BASE_URL: &str = "https://ugc.10jqka.com.cn";

/// 自选股 v2 基础 URL。
pub const SELF_STOCK_V2_BASE_URL: &str = "https://t.10jqka.com.cn";

/// 自选股 v2 列表接口路径。
pub const SELF_STOCK_V2_LIST_PATH: &str = "/newcircle/group/getSelfStockWithMarket/";

/// 自选股 v2 修改接口路径。
pub const SELF_STOCK_V2_MODIFY_PATH: &str = "/newcircle/group/modifySelfStock/";

/// 自选股 v1 查询接口路径。
pub const SELF_STOCK_V1_QUERY_PATH: &str = "/optdata/selfstock/open/api/v1/query";

/// 自选股 v1 修改接口路径。
pub const SELF_STOCK_V1_MODIFY_PATH: &str = "/optdata/selfstock/open/api/v1/modify";

/// multiStorage 接口 URL。
pub const MULTI_STORAGE_URL: &str = "https://cs.10jqka.com.cn/multiStorage";

/// multiStorage 中 blockstock 的 appname。
pub const BLOCKSTOCK_APPNAME: &str = "blockstock";

/// multiStorage 默认 clienttype。
pub const MULTI_STORAGE_DEFAULT_CLIENTTYPE: &str = "hevo_pc";

/// 动态板块基础 URL。
pub const DYNAMIC_PLATE_BASE_URL: &str = "https://apigate.10jqka.com.cn";

/// 动态板块查询路径。
pub const DYNAMIC_PLATE_SELECT_PATH: &str = "/d/platform/dynamicplate/stocks/self/v2/select";

/// selfstock_detail API URL。
pub const SELFSTOCK_DETAIL_API_URL: &str = "https://ugc.10jqka.com.cn/selfstock_detail";

/// selfstock_detail 超时时间（秒）。
pub const SELFSTOCK_DETAIL_TIMEOUT: f64 = 10.0;

/// 自选股协议通用 HTTP 超时（秒）。
pub const SELF_STOCK_HTTP_TIMEOUT: f64 = 10.0;

// ── 分组相关 API 端点 ──

/// 自定义分组 API 端点集合。
pub mod endpoints {
    pub const QUERY_GROUPS: &str = "/optdata/selfgroup/open/api/group/v1/query";
    pub const ADD_ITEM: &str = "/optdata/selfgroup/open/api/content/v1/add";
    pub const DELETE_ITEM: &str = "/optdata/selfgroup/open/api/content/v1/delete";
    pub const ADD_GROUP: &str = "/optdata/selfgroup/open/api/group/v1/add";
    pub const DELETE_GROUP: &str = "/optdata/selfgroup/open/api/group/v1/delete";
    pub const SHARE_GROUP: &str = "/optdata/sharing_service/open/api/sharing/v1/create";
}

// ── 缓存 ──

/// 分组数据缓存文件路径。
pub const CACHE_FILE: &str = "ths_favorite_cache.json";

/// Cookie 缓存文件路径。
pub const COOKIE_CACHE_FILE: &str = "ths_cookie_cache.json";

/// Cookie 缓存有效期（秒），默认 24 小时。
pub const COOKIE_CACHE_TTL_SECONDS: u64 = 24 * 60 * 60;

// ── 自选股标识 ──

/// "我的自选"虚拟分组的保留 ID。
pub const SELF_STOCK_GROUP_ID: &str = "__selfstock__";

/// "我的自选"默认显示名称。
pub const SELF_STOCK_DEFAULT_NAME: &str = "我的自选";

// ── 请求参数常量 ──

/// 默认来源参数。
pub const DEFAULT_FROM_PARAM: &str = "sjcg_gphone";

/// 分组查询类型。
pub const GROUP_QUERY_TYPES: &str = "0,1";

/// 动态分组 ID 前缀，以此开头的分组为只读板块。
pub const DYNAMIC_GROUP_PREFIX: &str = "1_";
