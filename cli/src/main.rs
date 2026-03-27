mod daemon_client;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "browser-tools", about = "Browser automation CLI powered by CDP")]
pub struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to Chrome/Chromium binary
    #[arg(long, global = true)]
    browser_path: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Navigate to a URL
    Navigate {
        /// URL to navigate to
        url: String,

        /// Capture a screenshot
        #[arg(long)]
        screenshot: bool,
    },
    /// Go back in browser history
    Back,
    /// Go forward in browser history
    Forward,
    /// Reload the current page
    Reload,
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        cmd: DaemonCmd,
    },
}

#[derive(Subcommand)]
enum DaemonCmd {
    /// Start the daemon (usually auto-started)
    Start,
    /// Stop the daemon
    Stop,
    /// Check daemon health
    Health,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Daemon { cmd } => match cmd {
            DaemonCmd::Start => cmd_daemon_start(&cli).await,
            DaemonCmd::Stop => cmd_daemon_stop(&cli).await,
            DaemonCmd::Health => cmd_daemon_health(&cli).await,
        },
        Commands::Navigate { url, .. } => cmd_navigate(&cli, url).await,
        Commands::Back => cmd_back(&cli).await,
        Commands::Forward => cmd_forward(&cli).await,
        Commands::Reload => cmd_reload(&cli).await,
    };

    if let Err(e) = result {
        if cli.json {
            let err = serde_json::json!({
                "error": {
                    "message": e.to_string(),
                    "retryHint": "Check daemon status with: browser-tools daemon health"
                }
            });
            eprintln!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("Error: {e}");
        }
        std::process::exit(1);
    }
}

type CmdResult = Result<(), Box<dyn std::error::Error>>;

async fn cmd_daemon_start(cli: &Cli) -> CmdResult {
    // When invoked as `browser-tools daemon start`, we ARE the daemon launcher.
    // Fork off the actual daemon process.
    daemon_client::start_daemon(cli.browser_path.as_deref()).await?;
    if cli.json {
        println!("{}", serde_json::json!({"status": "started"}));
    } else {
        println!("Daemon started.");
    }
    Ok(())
}

async fn cmd_daemon_stop(cli: &Cli) -> CmdResult {
    daemon_client::stop_daemon()?;
    if cli.json {
        println!("{}", serde_json::json!({"status": "stopped"}));
    } else {
        println!("Daemon stopped.");
    }
    Ok(())
}

async fn cmd_daemon_health(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("health", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    if let Some(result) = resp.result {
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Daemon: {}", result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown"));
            if let Some(pid) = result.get("pid") {
                println!("PID: {pid}");
            }
        }
    } else if let Some(err) = resp.error {
        return Err(format!("daemon error ({}): {}", err.code, err.message).into());
    }
    Ok(())
}

async fn cmd_navigate(cli: &Cli, url: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "navigate",
        serde_json::json!({"url": url}),
        cli.browser_path.as_deref(),
    )
    .await?;
    print_response(cli, resp)
}

async fn cmd_back(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("back", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    print_response(cli, resp)
}

async fn cmd_forward(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("forward", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    print_response(cli, resp)
}

async fn cmd_reload(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("reload", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    print_response(cli, resp)
}

fn print_response(
    cli: &Cli,
    resp: browser_tools_common::DaemonResponse,
) -> CmdResult {
    if let Some(err) = resp.error {
        return Err(format!("daemon error ({}): {}", err.code, err.message).into());
    }
    if let Some(result) = resp.result {
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            // Text mode: display key fields
            if let Some(title) = result.get("title").and_then(|v| v.as_str()) {
                println!("Title: {title}");
            }
            if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                println!("URL: {url}");
            }
            // Fallback for unknown result shapes
            if result.get("title").is_none() && result.get("url").is_none() {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
    }
    Ok(())
}
