mod daemon_client;
mod output;

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
    /// Get console log entries
    Console {
        /// Don't clear the buffer after reading
        #[arg(long)]
        no_clear: bool,
    },
    /// Get network log entries
    Network {
        /// Don't clear the buffer after reading
        #[arg(long)]
        no_clear: bool,

        /// Filter: all, errors, or fetch-xhr
        #[arg(long, default_value = "all")]
        filter: String,
    },
    /// Get dialog event entries
    Dialog {
        /// Don't clear the buffer after reading
        #[arg(long)]
        no_clear: bool,
    },
    /// Evaluate a JavaScript expression
    Eval {
        /// JavaScript expression to evaluate
        expression: String,
    },
    /// Click an element by selector or coordinates
    Click {
        /// CSS selector of the element to click
        selector: Option<String>,
        /// X coordinate (use with --y for coordinate click)
        #[arg(long)]
        x: Option<f64>,
        /// Y coordinate (use with --x for coordinate click)
        #[arg(long)]
        y: Option<f64>,
    },
    /// Type text into an input element
    Type {
        /// CSS selector of the input element
        selector: String,
        /// Text to type
        text: String,
        /// Type character-by-character instead of atomic fill
        #[arg(long)]
        slowly: bool,
        /// Clear the field before typing
        #[arg(long)]
        clear_first: bool,
        /// Press Enter after typing
        #[arg(long)]
        submit: bool,
    },
    /// Press a key or key combination (e.g. Enter, Meta+A)
    Press {
        /// Key name or combination (e.g. "Enter", "Meta+A", "Tab")
        key: String,
    },
    /// Hover over an element
    Hover {
        /// CSS selector of the element to hover
        selector: String,
    },
    /// Scroll the page up or down
    Scroll {
        /// Direction: "up" or "down"
        #[arg(long)]
        direction: String,
        /// Pixels to scroll (default: 300)
        #[arg(long, default_value = "300")]
        amount: i32,
    },
    /// Select an option from a <select> dropdown
    SelectOption {
        /// CSS selector of the <select> element
        selector: String,
        /// Option label or value to select
        option: String,
    },
    /// Set checkbox or radio button checked state
    SetChecked {
        /// CSS selector of the checkbox/radio element
        selector: String,
        /// Set to checked (true) or unchecked (false)
        #[arg(long)]
        checked: bool,
    },
    /// Drag an element to another element
    Drag {
        /// CSS selector of the source element
        source: String,
        /// CSS selector of the target element
        target: String,
    },
    /// Set the viewport size (preset or custom dimensions)
    SetViewport {
        /// Preset: mobile, tablet, desktop, wide
        #[arg(long)]
        preset: Option<String>,
        /// Custom width in pixels
        #[arg(long)]
        width: Option<u32>,
        /// Custom height in pixels
        #[arg(long)]
        height: Option<u32>,
    },
    /// Set files on a file input element
    UploadFile {
        /// CSS selector of the <input type="file"> element
        selector: String,
        /// File paths to upload
        files: Vec<String>,
    },
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
        Commands::Console { no_clear } => cmd_console(&cli, *no_clear).await,
        Commands::Network { no_clear, filter } => cmd_network(&cli, *no_clear, filter).await,
        Commands::Dialog { no_clear } => cmd_dialog(&cli, *no_clear).await,
        Commands::Eval { expression } => cmd_eval(&cli, expression).await,
        Commands::Click { selector, x, y } => cmd_click(&cli, selector.as_deref(), *x, *y).await,
        Commands::Type {
            selector,
            text,
            slowly,
            clear_first,
            submit,
        } => cmd_type(&cli, selector, text, *slowly, *clear_first, *submit).await,
        Commands::Press { key } => cmd_press(&cli, key).await,
        Commands::Hover { selector } => cmd_hover(&cli, selector).await,
        Commands::Scroll { direction, amount } => cmd_scroll(&cli, direction, *amount).await,
        Commands::SelectOption { selector, option } => {
            cmd_select_option(&cli, selector, option).await
        }
        Commands::SetChecked { selector, checked } => {
            cmd_set_checked(&cli, selector, *checked).await
        }
        Commands::Drag { source, target } => cmd_drag(&cli, source, target).await,
        Commands::SetViewport {
            preset,
            width,
            height,
        } => cmd_set_viewport(&cli, preset.as_deref(), *width, *height).await,
        Commands::UploadFile { selector, files } => cmd_upload_file(&cli, selector, files).await,
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
            println!("{}", output::format_json(&result));
        } else {
            println!(
                "Daemon: {}",
                result
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            );
            if let Some(pid) = result.get("pid") {
                println!("PID: {pid}");
            }
        }
    } else if let Some(err) = resp.error {
        if cli.json {
            eprintln!("{}", output::format_error_json(&err));
        } else {
            eprintln!("{}", output::format_error_text(&err));
        }
        std::process::exit(1);
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
    handle_response(cli, resp, output::format_text_navigate)
}

async fn cmd_back(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("back", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    handle_response(cli, resp, output::format_text_back)
}

async fn cmd_forward(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("forward", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    handle_response(cli, resp, output::format_text_forward)
}

async fn cmd_reload(cli: &Cli) -> CmdResult {
    let resp =
        daemon_client::send_request("reload", serde_json::json!({}), cli.browser_path.as_deref())
            .await?;
    handle_response(cli, resp, output::format_text_reload)
}

async fn cmd_console(cli: &Cli, no_clear: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "console",
        serde_json::json!({"clear": !no_clear}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_console)
}

async fn cmd_network(cli: &Cli, no_clear: bool, filter: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "network",
        serde_json::json!({"clear": !no_clear, "filter": filter}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_network)
}

async fn cmd_dialog(cli: &Cli, no_clear: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "dialog",
        serde_json::json!({"clear": !no_clear}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_dialog)
}

async fn cmd_eval(cli: &Cli, expression: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "eval",
        serde_json::json!({"expression": expression}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_eval)
}

async fn cmd_click(cli: &Cli, selector: Option<&str>, x: Option<f64>, y: Option<f64>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    if let Some(xv) = x {
        params["x"] = serde_json::json!(xv);
    }
    if let Some(yv) = y {
        params["y"] = serde_json::json!(yv);
    }
    let resp = daemon_client::send_request("click", params, cli.browser_path.as_deref()).await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_type(
    cli: &Cli,
    selector: &str,
    text: &str,
    slowly: bool,
    clear_first: bool,
    submit: bool,
) -> CmdResult {
    let resp = daemon_client::send_request(
        "type",
        serde_json::json!({
            "selector": selector,
            "text": text,
            "slowly": slowly,
            "clear_first": clear_first,
            "submit": submit,
        }),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_press(cli: &Cli, key: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "press",
        serde_json::json!({"key": key}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_hover(cli: &Cli, selector: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "hover",
        serde_json::json!({"selector": selector}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_scroll(cli: &Cli, direction: &str, amount: i32) -> CmdResult {
    let resp = daemon_client::send_request(
        "scroll",
        serde_json::json!({"direction": direction, "amount": amount}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_scroll)
}

async fn cmd_select_option(cli: &Cli, selector: &str, option: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "select_option",
        serde_json::json!({"selector": selector, "option": option}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_set_checked(cli: &Cli, selector: &str, checked: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "set_checked",
        serde_json::json!({"selector": selector, "checked": checked}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_drag(cli: &Cli, source: &str, target: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "drag",
        serde_json::json!({"source": source, "target": target}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_set_viewport(
    cli: &Cli,
    preset: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(p) = preset {
        params["preset"] = serde_json::json!(p);
    }
    if let Some(w) = width {
        params["width"] = serde_json::json!(w);
    }
    if let Some(h) = height {
        params["height"] = serde_json::json!(h);
    }
    let resp =
        daemon_client::send_request("set_viewport", params, cli.browser_path.as_deref()).await?;
    handle_response(cli, resp, output::format_text_viewport)
}

async fn cmd_upload_file(cli: &Cli, selector: &str, files: &[String]) -> CmdResult {
    let resp = daemon_client::send_request(
        "upload_file",
        serde_json::json!({"selector": selector, "files": files}),
        cli.browser_path.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

/// Generic response handler — delegates to the appropriate formatter based on --json flag.
fn handle_response(
    cli: &Cli,
    resp: browser_tools_common::DaemonResponse,
    text_formatter: fn(&serde_json::Value) -> String,
) -> CmdResult {
    if let Some(err) = resp.error {
        if cli.json {
            eprintln!("{}", output::format_error_json(&err));
        } else {
            eprintln!("{}", output::format_error_text(&err));
        }
        std::process::exit(1);
    }
    if let Some(result) = resp.result {
        if cli.json {
            println!("{}", output::format_json(&result));
        } else {
            println!("{}", text_formatter(&result));
        }
    }
    Ok(())
}
