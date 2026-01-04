//! Antigravity CLI - 命令行工具
//!
//! 提供账号管理、反代服务和配置管理功能

use clap::{Parser, Subcommand};
use antigravity_tools_lib::{models, modules, proxy};
use comfy_table::{Table, Row, Cell, Color, Attribute};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "antigravity-cli")]
#[command(author = "Antigravity Team")]
#[command(version)]
#[command(about = "Antigravity 命令行工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 账号管理
    #[command(subcommand)]
    Account(AccountCommands),

    /// 反代服务管理
    #[command(subcommand)]
    Proxy(ProxyCommands),

    /// 配置管理
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand)]
enum AccountCommands {
    /// 列出所有账号
    List,

    /// 添加账号 (通过 refresh_token)
    Add {
        /// Refresh Token
        token: String,
    },

    /// 删除账号
    Delete {
        /// 账号 ID
        id: String,
    },

    /// 切换当前账号
    Switch {
        /// 账号 ID
        id: String,
    },

    /// 查看当前账号
    Current,

    /// 刷新所有账号配额
    Refresh,
}

#[derive(Subcommand)]
enum ProxyCommands {
    /// 启动反代服务
    Start {
        /// 监听端口
        #[arg(short, long, default_value = "8045")]
        port: u16,

        /// 允许局域网访问
        #[arg(long)]
        lan: bool,
    },

    /// 停止反代服务
    Stop,

    /// 查看反代状态
    Status,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// 显示当前配置
    Show,

    /// 设置配置项
    Set {
        /// 配置键
        key: String,
        /// 配置值
        value: String,
    },
}

#[tokio::main]
async fn main() {
    // 初始化简化的日志（仅输出到终端）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("antigravity_tools_lib=info".parse().unwrap())
        )
        .with_target(false)
        .with_level(true)
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Account(cmd) => handle_account(cmd).await,
        Commands::Proxy(cmd) => handle_proxy(cmd).await,
        Commands::Config(cmd) => handle_config(cmd).await,
    };

    if let Err(e) = result {
        eprintln!("错误: {}", e);
        std::process::exit(1);
    }
}

// ==================== 账号管理 ====================

async fn handle_account(cmd: AccountCommands) -> Result<(), String> {
    match cmd {
        AccountCommands::List => account_list().await,
        AccountCommands::Add { token } => account_add(&token).await,
        AccountCommands::Delete { id } => account_delete(&id),
        AccountCommands::Switch { id } => account_switch(&id),
        AccountCommands::Current => account_current(),
        AccountCommands::Refresh => account_refresh().await,
    }
}

async fn account_list() -> Result<(), String> {
    let accounts = modules::account::list_accounts()?;
    let current_id = modules::account::get_current_account_id()?;

    if accounts.is_empty() {
        println!("暂无账号");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["", "ID", "邮箱", "名称", "订阅", "配额状态"]);

    for account in &accounts {
        let is_current = current_id.as_ref() == Some(&account.id);
        let marker = if is_current { "*" } else { "" };

        // 获取订阅类型
        let subscription = account.quota.as_ref()
            .and_then(|q| q.subscription_tier.clone())
            .unwrap_or_else(|| "未知".to_string());

        // 配额状态概览
        let quota_status = if let Some(ref quota) = account.quota {
            if quota.is_forbidden {
                "已禁用".to_string()
            } else {
                let model_count = quota.models.len();
                format!("{} 个模型", model_count)
            }
        } else {
            "未查询".to_string()
        };

        let mut row = Row::new();
        row.add_cell(Cell::new(marker));
        row.add_cell(Cell::new(&account.id[..8])); // 只显示前8位
        row.add_cell(Cell::new(&account.email));
        row.add_cell(Cell::new(account.name.as_deref().unwrap_or("-")));
        row.add_cell(Cell::new(&subscription));
        row.add_cell(Cell::new(&quota_status));

        if is_current {
            // 高亮当前账号 - 重新创建行
            let mut highlighted_row = Row::new();
            highlighted_row.add_cell(Cell::new(marker).fg(Color::Green).add_attribute(Attribute::Bold));
            highlighted_row.add_cell(Cell::new(&account.id[..8]).fg(Color::Green).add_attribute(Attribute::Bold));
            highlighted_row.add_cell(Cell::new(&account.email).fg(Color::Green).add_attribute(Attribute::Bold));
            highlighted_row.add_cell(Cell::new(account.name.as_deref().unwrap_or("-")).fg(Color::Green).add_attribute(Attribute::Bold));
            highlighted_row.add_cell(Cell::new(&subscription).fg(Color::Green).add_attribute(Attribute::Bold));
            highlighted_row.add_cell(Cell::new(&quota_status).fg(Color::Green).add_attribute(Attribute::Bold));
            table.add_row(highlighted_row);
        } else {
            table.add_row(row);
        }
    }

    println!("{}", table);
    println!("\n共 {} 个账号 (* 表示当前账号)", accounts.len());

    Ok(())
}

async fn account_add(refresh_token: &str) -> Result<(), String> {
    println!("正在验证并添加账号...");

    // 1. 使用 refresh_token 获取 access_token
    let token_response = modules::oauth::refresh_access_token(refresh_token)
        .await
        .map_err(|e| format!("Token 刷新失败: {}", e))?;

    // 2. 获取用户信息
    let user_info = modules::oauth::get_user_info(&token_response.access_token)
        .await
        .map_err(|e| format!("获取用户信息失败: {}", e))?;

    let email = user_info.email.clone();
    let name = user_info.get_display_name();

    // 3. 创建 TokenData
    let token_data = models::TokenData::new(
        token_response.access_token,
        refresh_token.to_string(),
        token_response.expires_in,
        Some(email.clone()),
        None, // project_id
        None, // session_id
    );

    // 4. 添加或更新账号
    let account = modules::account::upsert_account(email.clone(), name.clone(), token_data)?;

    println!("账号添加成功!");
    println!("  ID: {}", account.id);
    println!("  邮箱: {}", email);
    if let Some(n) = name {
        println!("  名称: {}", n);
    }

    Ok(())
}

fn account_delete(id: &str) -> Result<(), String> {
    // 支持短 ID 匹配
    let full_id = resolve_account_id(id)?;

    let account = modules::account::load_account(&full_id)?;
    modules::account::delete_account(&full_id)?;

    println!("已删除账号: {} ({})", account.email, &full_id[..8]);
    Ok(())
}

fn account_switch(id: &str) -> Result<(), String> {
    // 支持短 ID 匹配
    let full_id = resolve_account_id(id)?;

    let account = modules::account::load_account(&full_id)?;
    modules::account::set_current_account_id(&full_id)?;

    println!("已切换到账号: {} ({})", account.email, &full_id[..8]);
    println!("注意: CLI 模式仅切换反代服务使用的账号，不影响 IDE");

    Ok(())
}

fn account_current() -> Result<(), String> {
    match modules::account::get_current_account()? {
        Some(account) => {
            println!("当前账号:");
            println!("  ID: {}", account.id);
            println!("  邮箱: {}", account.email);
            if let Some(ref name) = account.name {
                println!("  名称: {}", name);
            }
            if let Some(ref quota) = account.quota {
                if quota.is_forbidden {
                    println!("  状态: 已禁用");
                } else {
                    println!("  订阅: {}", quota.subscription_tier.as_deref().unwrap_or("未知"));
                    println!("  模型数: {}", quota.models.len());
                }
            }
        }
        None => {
            println!("当前没有选中的账号");
        }
    }
    Ok(())
}

async fn account_refresh() -> Result<(), String> {
    let mut accounts = modules::account::list_accounts()?;

    if accounts.is_empty() {
        println!("暂无账号");
        return Ok(());
    }

    println!("正在刷新 {} 个账号的配额...\n", accounts.len());

    let mut success_count = 0;
    let mut error_count = 0;

    for account in accounts.iter_mut() {
        print!("  {} ... ", account.email);

        match modules::account::fetch_quota_with_retry(account).await {
            Ok(quota) => {
                // 保存更新后的配额
                account.update_quota(quota.clone());
                if let Err(e) = modules::account::save_account(account) {
                    println!("保存失败: {}", e);
                    error_count += 1;
                } else {
                    if quota.is_forbidden {
                        println!("已禁用");
                    } else {
                        println!("{} ({} 个模型)",
                            quota.subscription_tier.as_deref().unwrap_or("未知"),
                            quota.models.len()
                        );
                    }
                    success_count += 1;
                }
            }
            Err(e) => {
                println!("失败: {}", e);
                error_count += 1;
            }
        }
    }

    println!("\n刷新完成: {} 成功, {} 失败", success_count, error_count);
    Ok(())
}

/// 解析账号 ID（支持短 ID 前缀匹配）
fn resolve_account_id(id: &str) -> Result<String, String> {
    let index = modules::account::load_account_index()?;

    // 精确匹配
    if index.accounts.iter().any(|a| a.id == id) {
        return Ok(id.to_string());
    }

    // 前缀匹配
    let matches: Vec<_> = index.accounts.iter()
        .filter(|a| a.id.starts_with(id))
        .collect();

    match matches.len() {
        0 => Err(format!("找不到账号: {}", id)),
        1 => Ok(matches[0].id.clone()),
        _ => Err(format!("ID 前缀 '{}' 匹配多个账号，请提供更长的 ID", id)),
    }
}

// ==================== 反代服务 ====================

async fn handle_proxy(cmd: ProxyCommands) -> Result<(), String> {
    match cmd {
        ProxyCommands::Start { port, lan } => proxy_start(port, lan).await,
        ProxyCommands::Stop => proxy_stop(),
        ProxyCommands::Status => proxy_status(),
    }
}

async fn proxy_start(port: u16, lan: bool) -> Result<(), String> {
    // 加载配置
    let mut config = modules::config::load_app_config()
        .unwrap_or_else(|_| models::AppConfig::default());

    config.proxy.port = port;
    config.proxy.allow_lan_access = lan;

    let host = if lan { "0.0.0.0" } else { "127.0.0.1" };

    // 初始化 TokenManager
    let data_dir = modules::account::get_data_dir()?;
    let token_manager = Arc::new(proxy::TokenManager::new(data_dir));

    // 加载账号
    let account_count = token_manager.load_accounts().await
        .map_err(|e| format!("加载账号失败: {}", e))?;

    if account_count == 0 {
        return Err("没有可用的账号，请先添加账号".to_string());
    }

    println!("Antigravity 反代服务");
    println!("====================");
    println!("监听地址: http://{}:{}", host, port);
    println!("API Key: {}", config.proxy.api_key);
    println!("账号池: {} 个账号", account_count);
    println!();
    println!("支持的协议:");
    println!("  - OpenAI: POST /v1/chat/completions");
    println!("  - Claude: POST /v1/messages");
    println!("  - Gemini: POST /v1beta/models/:model:generateContent");
    if config.proxy.zai.enabled {
        println!("  - z.ai: 已启用 ({})", config.proxy.zai.dispatch_mode_display());
    }
    println!();
    println!("按 Ctrl+C 停止服务");
    println!();

    // 创建安全配置
    let security_config = proxy::ProxySecurityConfig::from_proxy_config(&config.proxy);

    // 创建监控器 (CLI 模式下不传 app_handle)
    let monitor = Arc::new(proxy::monitor::ProxyMonitor::new(1000, None));
    if config.proxy.enable_logging {
        monitor.set_enabled(true);
        println!("监控日志: 已启用");
    }

    // 启动服务器
    let (server, handle) = proxy::AxumServer::start(
        host.to_string(),
        port,
        token_manager,
        config.proxy.anthropic_mapping.clone(),
        config.proxy.openai_mapping.clone(),
        config.proxy.custom_mapping.clone(),
        config.proxy.request_timeout,
        config.proxy.upstream_proxy.clone(),
        security_config,
        config.proxy.zai.clone(),
        monitor,
    ).await?;

    // 等待 Ctrl+C
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("信号处理失败: {}", e))?;

    println!("\n正在停止服务...");
    server.stop();
    let _ = handle.await;
    println!("服务已停止");

    Ok(())
}

fn proxy_stop() -> Result<(), String> {
    // CLI 模式下，服务是前台运行的，不需要单独的 stop 命令
    // 这个命令主要用于提示用户
    println!("CLI 模式下，反代服务在前台运行");
    println!("请使用 Ctrl+C 停止当前运行的服务");
    Ok(())
}

fn proxy_status() -> Result<(), String> {
    let config = modules::config::load_app_config()
        .unwrap_or_else(|_| models::AppConfig::default());

    println!("反代服务配置:");
    println!("  端口: {}", config.proxy.port);
    println!("  API Key: {}", config.proxy.api_key);
    println!("  局域网访问: {}", if config.proxy.allow_lan_access { "允许" } else { "禁止" });
    println!("  请求超时: {}s", config.proxy.request_timeout);
    println!("  自动启动: {}", if config.proxy.auto_start { "是" } else { "否" });

    if config.proxy.upstream_proxy.enabled {
        println!("  上游代理: {}", config.proxy.upstream_proxy.url);
    }

    // 检查账号数量
    match modules::account::list_accounts() {
        Ok(accounts) => {
            let valid_count = accounts.iter()
                .filter(|a| a.quota.as_ref().map_or(true, |q| !q.is_forbidden))
                .count();
            println!("\n可用账号: {} 个 (共 {} 个)", valid_count, accounts.len());
        }
        Err(e) => {
            println!("\n账号加载失败: {}", e);
        }
    }

    Ok(())
}

// ==================== 配置管理 ====================

async fn handle_config(cmd: ConfigCommands) -> Result<(), String> {
    match cmd {
        ConfigCommands::Show => config_show(),
        ConfigCommands::Set { key, value } => config_set(&key, &value),
    }
}

fn config_show() -> Result<(), String> {
    let config = modules::config::load_app_config()
        .unwrap_or_else(|_| models::AppConfig::default());

    println!("当前配置:");
    println!();
    println!("[基本]");
    println!("  language: {}", config.language);
    println!("  theme: {}", config.theme);
    println!("  auto_refresh: {}", config.auto_refresh);
    println!("  refresh_interval: {}", config.refresh_interval);
    println!("  auto_launch: {}", config.auto_launch);
    println!();
    println!("[反代]");
    println!("  proxy.enabled: {}", config.proxy.enabled);
    println!("  proxy.port: {}", config.proxy.port);
    println!("  proxy.api_key: {}", config.proxy.api_key);
    println!("  proxy.allow_lan_access: {}", config.proxy.allow_lan_access);
    println!("  proxy.auto_start: {}", config.proxy.auto_start);
    println!("  proxy.request_timeout: {}", config.proxy.request_timeout);
    println!();
    println!("[上游代理]");
    println!("  proxy.upstream_proxy.enabled: {}", config.proxy.upstream_proxy.enabled);
    println!("  proxy.upstream_proxy.url: {}", config.proxy.upstream_proxy.url);

    Ok(())
}

fn config_set(key: &str, value: &str) -> Result<(), String> {
    let mut config = modules::config::load_app_config()
        .unwrap_or_else(|_| models::AppConfig::default());

    match key {
        "language" => config.language = value.to_string(),
        "theme" => config.theme = value.to_string(),
        "auto_refresh" => config.auto_refresh = value.parse().map_err(|_| "无效的布尔值")?,
        "refresh_interval" => config.refresh_interval = value.parse().map_err(|_| "无效的整数")?,
        "auto_launch" => config.auto_launch = value.parse().map_err(|_| "无效的布尔值")?,

        "proxy.enabled" => config.proxy.enabled = value.parse().map_err(|_| "无效的布尔值")?,
        "proxy.port" => config.proxy.port = value.parse().map_err(|_| "无效的端口号")?,
        "proxy.api_key" => config.proxy.api_key = value.to_string(),
        "proxy.allow_lan_access" => config.proxy.allow_lan_access = value.parse().map_err(|_| "无效的布尔值")?,
        "proxy.auto_start" => config.proxy.auto_start = value.parse().map_err(|_| "无效的布尔值")?,
        "proxy.request_timeout" => config.proxy.request_timeout = value.parse().map_err(|_| "无效的整数")?,

        "proxy.upstream_proxy.enabled" => config.proxy.upstream_proxy.enabled = value.parse().map_err(|_| "无效的布尔值")?,
        "proxy.upstream_proxy.url" => config.proxy.upstream_proxy.url = value.to_string(),

        _ => return Err(format!("未知的配置项: {}", key)),
    }

    modules::config::save_app_config(&config)?;
    println!("配置已更新: {} = {}", key, value);

    Ok(())
}
