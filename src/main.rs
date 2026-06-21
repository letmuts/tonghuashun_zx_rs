/// 同花顺自选股管理工具 — Rust 重写版 CLI 入口。
///
/// 支持以下命令：
/// - `list`: 查看全部分组
/// - `self list`: 查看"我的自选"
/// - `stock add <分组> <代码...>`: 添加股票
/// - `stock del <分组> <代码...>`: 删除股票
/// - `group add <名称>`: 添加分组
/// - `group del <名称>`: 删除分组
/// - `group share <名称> <有效期秒>`: 分享分组

mod api;
mod auth;
mod client;
mod config;
mod constant;
mod cookie;
mod errors;
mod models;
mod protobuf;
mod protocol;
mod service;
mod storage;
mod utils;

use clap::{Parser, Subcommand};
use crate::cookie::parse_cookie_string;
use crate::service::PortfolioManager;

#[derive(Parser)]
#[command(name = "tonghuashun_rs", about = "同花顺自选股管理工具")]
struct Cli {
    /// 账号（手机号）
    #[arg(long)]
    username: Option<String>,

    /// 密码
    #[arg(long)]
    password: Option<String>,

    /// 显式传入 Cookie 字符串
    #[arg(long)]
    cookies: Option<String>,

    /// Cookie 缓存文件路径
    #[arg(long)]
    cookie_cache: Option<String>,

    /// 禁用数据缓存
    #[arg(long)]
    no_cache: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 查看全部自选分组
    List {
        /// 只查看指定分组
        #[arg(short = 'g', long)]
        group: Option<String>,
    },
    /// "我的自选"操作
    #[command(name = "self")]
    SelfStock {
        #[command(subcommand)]
        cmd: SelfCommand,
    },
    /// 股票操作
    Stock {
        #[command(subcommand)]
        cmd: StockCommand,
    },
    /// 分组操作
    Group {
        #[command(subcommand)]
        cmd: GroupCommand,
    },
}

#[derive(Subcommand)]
enum SelfCommand {
    /// 列出"我的自选"
    List,
}

#[derive(Subcommand)]
enum StockCommand {
    /// 添加股票到分组
    Add {
        /// 分组名称（或"我的自选"）
        group: String,
        /// 股票代码，格式: 600519.SH 或 000001.SZ
        symbols: Vec<String>,
    },
    /// 从分组删除股票
    Del {
        /// 分组名称
        group: String,
        /// 股票代码
        symbols: Vec<String>,
    },
}

#[derive(Subcommand)]
enum GroupCommand {
    /// 添加新分组
    Add {
        /// 分组名称
        name: String,
    },
    /// 删除分组
    Del {
        /// 分组名称
        name: String,
    },
    /// 分享分组
    Share {
        /// 分组名称
        name: String,
        /// 分享有效期（秒）
        valid_time: u64,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    let cookies = cli.cookies.as_ref().map(|s| parse_cookie_string(s));

    let mut manager = PortfolioManager::new(
        cookies.as_ref(),
        cli.username.as_deref(),
        cli.password.as_deref(),
        cli.cookie_cache.as_deref(),
        None,
        !cli.no_cache,
    );

    match cli.command {
        Commands::List { group } => cmd_list(&mut manager, group.as_deref()),
        Commands::SelfStock { cmd: SelfCommand::List } => cmd_self_list(&mut manager),
        Commands::Stock { cmd } => match cmd {
            StockCommand::Add { group, symbols } => cmd_stock_add(&mut manager, &group, &symbols),
            StockCommand::Del { group, symbols } => cmd_stock_del(&mut manager, &group, &symbols),
        },
        Commands::Group { cmd } => match cmd {
            GroupCommand::Add { name } => cmd_group_add(&mut manager, &name),
            GroupCommand::Del { name } => cmd_group_del(&mut manager, &name),
            GroupCommand::Share { name, valid_time } => {
                cmd_group_share(&manager, &name, valid_time)
            }
        },
    }
}

fn cmd_list(manager: &mut PortfolioManager, group_filter: Option<&str>) {
    match manager.get_all_groups(true) {
        Ok(groups) => {
            if let Some(filter) = group_filter {
                if let Some(group) = groups.get(filter) {
                    print_group(group);
                } else {
                    println!("未找到分组: {}", filter);
                }
            } else {
                println!("共 {} 个分组:", groups.len());
                for (_name, group) in &groups {
                    print_group(group);
                }
            }
        }
        Err(e) => eprintln!("获取分组失败: {}", e),
    }
}

fn cmd_self_list(manager: &mut PortfolioManager) {
    match manager.get_self_stocks(false) {
        Ok(group) => print_group(&group),
        Err(e) => eprintln!("获取我的自选失败: {}", e),
    }
}

fn cmd_stock_add(manager: &mut PortfolioManager, group: &str, symbols: &[String]) {
    if symbols.len() == 1 {
        match manager.add_item(group, &symbols[0]) {
            Ok(_) => println!("已添加 '{}' 到分组 '{}'。", symbols[0], group),
            Err(e) => eprintln!("添加失败: {}", e),
        }
    } else {
        match manager.add_items(group, symbols) {
            Ok(_) => println!("已批量添加 {} 个项目到分组 '{}'。", symbols.len(), group),
            Err(e) => eprintln!("批量添加失败: {}", e),
        }
    }
}

fn cmd_stock_del(manager: &mut PortfolioManager, group: &str, symbols: &[String]) {
    if symbols.len() == 1 {
        match manager.remove_item(group, &symbols[0]) {
            Ok(_) => println!("已从分组 '{}' 删除 '{}'。", group, symbols[0]),
            Err(e) => eprintln!("删除失败: {}", e),
        }
    } else {
        match manager.remove_items(group, symbols) {
            Ok(_) => println!("已批量从分组 '{}' 删除 {} 个项目。", group, symbols.len()),
            Err(e) => eprintln!("批量删除失败: {}", e),
        }
    }
}

fn cmd_group_add(manager: &mut PortfolioManager, name: &str) {
    match manager.add_group(name) {
        Ok(_) => println!("已添加分组 '{}'。", name),
        Err(e) => eprintln!("添加分组失败: {}", e),
    }
}

fn cmd_group_del(manager: &mut PortfolioManager, name: &str) {
    match manager.delete_group(name) {
        Ok(_) => println!("已删除分组 '{}'。", name),
        Err(e) => eprintln!("删除分组失败: {}", e),
    }
}

fn cmd_group_share(manager: &PortfolioManager, name: &str, valid_time: u64) {
    match manager.share_group(name, valid_time) {
        Ok(result) => {
            println!("分享分组 '{}' 成功:", name);
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
        }
        Err(e) => eprintln!("分享分组失败: {}", e),
    }
}

fn print_group(group: &crate::models::StockGroup) {
    let readonly_mark = if group.readonly { " [只读]" } else { "" };
    println!("  {} ({}){}{}", group.name, group.group_id, readonly_mark, if group.items.is_empty() {
        String::from("")
    } else {
        format!(" - {} 只股票", group.items.len())
    });
    for item in &group.items {
        let mut info = format!("    {}", item);
        if let Some(price) = item.price {
            info.push_str(&format!("  加入价: {:.2}", price));
        }
        if let Some(ref added_at) = item.added_at {
            info.push_str(&format!("  {}", added_at));
        }
        println!("{}", info);
    }
}
