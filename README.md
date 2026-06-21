# 同花顺自选股管理工具(Rust重写)
同花顺股票自选管理，基于[sunnysab/ths-favorite](https://github.com/sunnysab/ths-favorite)项目的rust重写版。

支持账号密码登录、Cookie 注入、
自选股分组查询、股票增删、分组管理和本地缓存。

## 环境要求

- Rust >= 1.80（LazyLock 稳定版）
- Cargo

## 安装与编译

```bash
cd tonghuashun_rs
cargo build --release
```

编译产物位于 `target/release/tonghuashun_rs.exe`（Windows）或
`target/release/tonghuashun_rs`（Linux/macOS）。

## 快速开始

### 1. 首次使用：账号密码登录

```bash
tonghuashun_rs --username 13300000000 --password self list
```

登录成功后,显示自选股票，Cookie 会被缓存到 `ths_cookie_cache.json`，后续可直接复用。（`tonghuashun_rs self list`即可）

### 2. 复用缓存直接使用

```bash
tonghuashun_rs list
tonghuashun_rs self list
```

### 3. 显式注入 Cookie

```bash
tonghuashun_rs --cookies "userid=xxx; sessionid=yyy" list
```

## 命令参考

### 全局参数

| 参数 | 说明 |
|------|------|
| `--username <手机号>` | 同花顺账号 |
| `--password <密码>` | 登录密码 |
| `--cookies <字符串>` | 显式传入 Cookie（格式: `k1=v1; k2=v2`） |
| `--cookie-cache <路径>` | Cookie 缓存文件路径（默认 `ths_cookie_cache.json`） |
| `--no-cache` | 禁用数据缓存，每次都从 API 获取 |

### 查看分组

```bash
# 查看全部自选分组
tonghuashun_rs list

# 查看指定分组
tonghuashun_rs list -g 消费

# 查看"我的自选"
tonghuashun_rs self list
```

### 股票操作

```bash
# 添加单只股票
tonghuashun_rs stock add 消费 600519.SH

# 批量添加（一次 API 请求）
tonghuashun_rs stock add 我的自选 600519.SH 000858.SZ 300750.SZ

# 删除单只股票
tonghuashun_rs stock del 消费 600519.SH

# 批量删除
tonghuashun_rs stock del 我的自选 000001.SZ 000002.SZ
```

**股票代码格式：**

- `600519.SH` — 上海交易所
- `000001.SZ` — 深圳交易所
- `300750.SZ` — 深圳创业板
- `688981.SH` — 科创板
- `600519` — 纯数字代码默认视为上海

### 分组操作

```bash
# 添加新分组
tonghuashun_rs group add "长线跟踪"

# 删除分组
tonghuashun_rs group del 消费

# 分享分组（有效期 604800 秒 = 7 天）
tonghuashun_rs group share 消费 604800
```

## 项目架构

```
src/
├── main.rs          # CLI 入口（clap）
├── config.rs        # 全局配置常量
├── constant.rs      # 市场代码双向映射
├── errors.rs        # 错误类型定义
├── models.rs        # 数据模型（StockItem, StockGroup 等）
├── cookie.rs        # Cookie 字符串解析
├── protobuf.rs      # 简易 Protobuf 编解码
├── utils.rs         # XML 解析等工具函数
├── storage.rs       # 缓存数据持久化
├── client.rs        # HTTP 客户端封装
├── auth.rs          # 认证模块（四步登录 + 策略管理）
├── api.rs           # API 路由层
├── service.rs       # 高层服务（PortfolioManager）
└── protocol/        # 底层协议实现
    ├── mod.rs
    ├── blockstock.rs     # multiStorage 分组协议
    ├── selfstock_v1.rs   # v1 自选股批量协议
    ├── selfstock_v2.rs   # v2 自选股单条协议
    └── dynamicplate.rs   # 动态板块查询协议
```

## 登录流程

本项目实现了同花顺移动客户端的完整四步登录流程：

1. **获取 RSA 公钥**：从 `auth.10jqka.com.cn/verify2` 获取 PEM 格式公钥
2. **统一登录**：用 RSA PKCS1v15 加密账号密码，调用 `unified_login`
3. **Mainverify**：获取 `signvalid` 签名验证令牌
4. **Cookie 兑换**：使用 userid/sessionid/signvalid 三元组调用 `docookie2.php` 兑换最终 Cookie

## 缓存机制

### Cookie 缓存

- 文件：`ths_cookie_cache.json`（可自定义）
- Key：`credentials::SHA256(用户名)`
- TTL：默认 24 小时
- 内容：Cookie 映射表 + 时间戳 + auth_params

### 数据缓存

- 文件：`ths_favorite_cache.json`
- 内容：分组列表 + "我的自选"条目
- 只读分组（动态板块）不参与缓存

## 认证策略

`SessionManager` 按优先级选择认证方式：

1. 显式传入 `--cookies`（最高优先级）
2. 同时提供 `--username` + `--password` → 执行登录并缓存
3. 仅提供 `--username` → 读取该账号缓存（未命中则报错）
4. 无参数 → 读取最近的有效缓存

## 对比 Python 版

| 特性 | Python 版 | Rust 版 |
|------|----------|---------|
| 运行时 | Python >= 3.8 | 单一可执行文件 |
| 启动速度 | 慢（解释器启动 + 导入） | 快（原生编译） |
| 类型安全 | 运行时检查 | 编译期保证 |
| 内存占用 | ~30-50MB | ~5-10MB |
| 依赖管理 | uv/pip | Cargo（单一锁文件） |
| 错误处理 | try-except | Result 类型（强制处理） |
| 并发 | 单线程 | reqwest 线程池复用 |

## 开发

```bash
# 编译检查
cargo check

# 运行测试
cargo test

# 编译 release
cargo build --release
```

## 注意事项

- 本工具仅供学习和合法管理个人自选股使用。
- 接口协议可能随同花顺客户端更新而变化。
- 移动端 User-Agent 模拟可能在未来被服务端识别。
- Rust edition 为 2024，需要较新版本的 Rust 编译器。
