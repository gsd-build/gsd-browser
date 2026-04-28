mod daemon;
mod daemon_client;
mod output;

use clap::{Parser, Subcommand};
use gsd_browser_common::config::Config;
use gsd_browser_common::validate_session_name;

#[derive(Parser)]
#[command(
    name = "gsd-browser",
    version,
    about = "Browser automation CLI powered by CDP"
)]
pub struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to Chrome/Chromium binary
    #[arg(long, global = true)]
    browser_path: Option<String>,

    /// CDP WebSocket URL to attach to an already-running Chrome
    /// (e.g. one launched with --remote-debugging-port=9222).
    /// Accepts ws:// URLs or http:// endpoints.
    #[arg(long, global = true)]
    cdp_url: Option<String>,

    /// Named session for parallel daemon instances
    #[arg(long, global = true)]
    session: Option<String>,

    /// Browser identity scope: session, project, or global
    #[arg(long, global = true)]
    identity_scope: Option<String>,

    /// Browser identity key
    #[arg(long, global = true)]
    identity_key: Option<String>,

    /// Project id for project-scoped browser identities
    #[arg(long, global = true)]
    identity_project: Option<String>,

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
    /// Take a screenshot of the current page or a specific element
    Screenshot {
        /// CSS selector for element crop (produces PNG)
        #[arg(long)]
        selector: Option<String>,
        /// Capture full scrollable page
        #[arg(long)]
        full_page: bool,
        /// JPEG quality 1-100 (default: 80)
        #[arg(long, default_value = "80")]
        quality: u32,
        /// Output file path — writes raw image bytes to disk
        #[arg(long)]
        output: Option<String>,
        /// Image format: jpeg or png (default: jpeg)
        #[arg(long, default_value = "jpeg")]
        format: String,
    },
    /// Get the accessibility tree of the page (roles, names, states)
    AccessibilityTree {
        /// CSS selector to scope the tree
        #[arg(long)]
        selector: Option<String>,
        /// Maximum tree depth (default: 10)
        #[arg(long, default_value = "10")]
        max_depth: u32,
        /// Maximum elements to include (default: 100)
        #[arg(long, default_value = "100")]
        max_count: u32,
    },
    /// Find elements by role, text content, or CSS selector
    Find {
        /// ARIA role to match (e.g. link, button, textbox)
        #[arg(long)]
        role: Option<String>,
        /// Text content to match (case-insensitive contains)
        #[arg(long)]
        text: Option<String>,
        /// CSS selector to scope search
        #[arg(long)]
        selector: Option<String>,
        /// Maximum elements to return (default: 20)
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Get raw HTML source of the page or a specific element
    PageSource {
        /// CSS selector to scope the source
        #[arg(long)]
        selector: Option<String>,
    },
    /// Wait for a condition before continuing
    WaitFor {
        /// Condition: selector_visible, selector_hidden, url_contains, network_idle,
        /// delay, text_visible, text_hidden, request_completed, console_message,
        /// element_count, region_stable
        #[arg(long)]
        condition: String,
        /// Condition-specific value (selector, text, URL substring, delay ms)
        #[arg(long)]
        value: Option<String>,
        /// Threshold for element_count (e.g. ">=3", "==0", "<5")
        #[arg(long)]
        threshold: Option<String>,
        /// Maximum wait time in milliseconds (default: 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Query the action timeline
    Timeline {
        /// Write timeline to disk as JSON
        #[arg(long)]
        write_to_disk: bool,
    },
    /// Take a snapshot of interactive elements and assign versioned refs
    Snapshot {
        /// CSS selector to scope the snapshot
        #[arg(long)]
        selector: Option<String>,
        /// Only include interactive elements (default: true)
        #[arg(long, default_value = "true")]
        interactive_only: bool,
        /// Maximum number of elements (default: 40)
        #[arg(long, default_value = "40")]
        limit: u32,
        /// Semantic mode: interactive, form, dialog, navigation, errors, headings, visible_only
        #[arg(long)]
        mode: Option<String>,
    },
    /// Get metadata for a specific element ref (e.g. @v1:e1)
    GetRef {
        /// Ref string in @vN:eM format
        #[arg(name = "ref")]
        ref_str: String,
    },
    /// Click an element by its snapshot ref
    ClickRef {
        /// Ref string in @vN:eM format
        #[arg(name = "ref")]
        ref_str: String,
    },
    /// Hover over an element by its snapshot ref
    HoverRef {
        /// Ref string in @vN:eM format
        #[arg(name = "ref")]
        ref_str: String,
    },
    /// Type text into an element by its snapshot ref
    FillRef {
        /// Ref string in @vN:eM format
        #[arg(name = "ref")]
        ref_str: String,
        /// Text to type
        text: String,
        /// Clear the field before typing
        #[arg(long)]
        clear_first: bool,
        /// Press Enter after typing
        #[arg(long)]
        submit: bool,
        /// Type character-by-character
        #[arg(long)]
        slowly: bool,
    },
    /// Run one or more assertions against current page state.
    ///
    /// Valid assertion kinds: url_contains, text_visible, text_hidden,
    /// selector_visible, selector_hidden, value_equals, checked,
    /// no_console_errors, no_failed_requests, request_url_seen,
    /// response_status, console_message_matches, network_count,
    /// console_count, element_count, no_console_errors_since,
    /// no_failed_requests_since
    Assert {
        /// JSON array of assertion checks, e.g. '[{"kind":"url_contains","text":"example"}]'
        #[arg(long)]
        checks: String,
    },
    /// Compare current page state against stored snapshot
    Diff {
        /// Compare against state from this action ID
        #[arg(long)]
        since: Option<u64>,
    },
    /// Execute multiple browser steps in sequence
    Batch {
        /// JSON array of step objects (each with "action" and action-specific fields)
        #[arg(long)]
        steps: String,
        /// Stop on first failing step (default: true)
        #[arg(long, default_value = "true")]
        stop_on_failure: bool,
        /// Return only the final summary, omitting per-step details
        #[arg(long)]
        summary_only: bool,
    },
    /// List all open browser pages/tabs
    ListPages,
    /// Switch active page by ID
    SwitchPage {
        /// Page ID to switch to (from list-pages)
        id: u64,
    },
    /// Close a browser page by ID
    ClosePage {
        /// Page ID to close (from list-pages)
        id: u64,
    },
    /// List all frames in the active page
    ListFrames,
    /// Select a frame for subsequent operations
    SelectFrame {
        /// Frame name to select ('main' to reset to main frame)
        #[arg(long)]
        name: Option<String>,
        /// Frame index (from list-frames)
        #[arg(long)]
        index: Option<u64>,
        /// URL substring to match
        #[arg(long)]
        url_pattern: Option<String>,
    },
    /// Analyze form fields, labels, and submit buttons
    AnalyzeForm {
        /// CSS selector to scope to a specific form
        #[arg(long)]
        selector: Option<String>,
    },
    /// Fill multiple form fields by identifier (label, name, placeholder, or aria-label)
    FillForm {
        /// JSON object mapping field identifiers to values, e.g. '{"Email": "a@b.com", "Password": "secret"}'
        #[arg(long)]
        values: String,
        /// CSS selector to scope to a specific form
        #[arg(long)]
        selector: Option<String>,
        /// Click the submit button after filling
        #[arg(long)]
        submit: bool,
    },
    /// Find the best-matching element for a semantic intent
    FindBest {
        /// Semantic intent: submit_form, close_dialog, primary_cta, search_field,
        /// next_step, dismiss, auth_action, back_navigation
        #[arg(long)]
        intent: String,
        /// CSS selector to narrow the search area
        #[arg(long)]
        scope: Option<String>,
    },
    /// Execute a semantic action (find best candidate and click/focus it)
    Act {
        /// Semantic intent: submit_form, close_dialog, primary_cta, search_field,
        /// next_step, dismiss, auth_action, back_navigation
        #[arg(long)]
        intent: String,
        /// CSS selector to narrow the search area
        #[arg(long)]
        scope: Option<String>,
    },
    /// Get a diagnostic summary of the current browser session
    SessionSummary,
    /// Capture a debug bundle (screenshot, logs, timeline, accessibility tree)
    DebugBundle {
        /// Optional name suffix for the bundle directory
        #[arg(long)]
        name: Option<String>,
    },
    /// Compare page screenshot against a stored baseline
    VisualDiff {
        /// Baseline name (default: auto from URL + viewport)
        #[arg(long)]
        name: Option<String>,
        /// CSS selector to scope comparison
        #[arg(long)]
        selector: Option<String>,
        /// Pixel matching tolerance 0–1 (default: 0.1)
        #[arg(long)]
        threshold: Option<f64>,
        /// Overwrite existing baseline with current screenshot
        #[arg(long)]
        update_baseline: bool,
    },
    /// Capture and upscale a rectangular region of the page
    ZoomRegion {
        /// Left coordinate in CSS pixels
        #[arg(long)]
        x: f64,
        /// Top coordinate in CSS pixels
        #[arg(long)]
        y: f64,
        /// Width of region in CSS pixels
        #[arg(long)]
        width: f64,
        /// Height of region in CSS pixels
        #[arg(long)]
        height: f64,
        /// Upscale factor (default: 2)
        #[arg(long, default_value = "2")]
        scale: f64,
    },
    /// Save the current page as a PDF file
    SavePdf {
        /// Page format: A4, Letter, Legal, Tabloid (default: A4)
        #[arg(long, default_value = "A4")]
        format: String,
        /// Include background graphics (default: true)
        #[arg(long, default_value = "true")]
        print_background: bool,
        /// Output filename
        #[arg(long)]
        filename: Option<String>,
        /// Full output path (overrides default directory)
        #[arg(long)]
        output: Option<String>,
    },
    /// Extract structured data from the page using CSS selectors
    Extract {
        /// JSON schema with _selector and _attribute hints per property
        #[arg(long)]
        schema: String,
        /// CSS selector to scope extraction
        #[arg(long)]
        selector: Option<String>,
        /// Extract array of items (container selector mode)
        #[arg(long)]
        multiple: bool,
    },
    /// Intercept network requests matching a URL pattern and respond with custom data
    MockRoute {
        /// URL pattern to intercept (glob, e.g. '**/api/users*')
        #[arg(long)]
        url: String,
        /// HTTP status code for the mock response (default: 200)
        #[arg(long, default_value = "200")]
        status: u16,
        /// Response body string
        #[arg(long)]
        body: Option<String>,
        /// Content-Type header (default: auto-detect)
        #[arg(long)]
        content_type: Option<String>,
        /// Response delay in milliseconds
        #[arg(long)]
        delay: Option<u64>,
        /// Additional response headers as JSON object
        #[arg(long)]
        headers: Option<String>,
    },
    /// Block network requests matching URL patterns
    BlockUrls {
        /// URL patterns to block (glob syntax)
        patterns: Vec<String>,
    },
    /// Remove all active route mocks and URL blocks
    ClearRoutes,
    /// Emulate a specific device (viewport, user agent, scale factor, touch)
    EmulateDevice {
        /// Device name (e.g. 'iPhone 15', 'Pixel 7') or 'list' for all presets
        device: String,
    },
    /// Save browser state (cookies, localStorage, sessionStorage) to disk
    SaveState {
        /// State name (default: "default")
        #[arg(long, default_value = "default")]
        name: String,
    },
    /// Restore browser state from a saved file
    RestoreState {
        /// State name (default: "default")
        #[arg(long, default_value = "default")]
        name: String,
    },
    /// Save credentials to the encrypted auth vault
    VaultSave {
        /// Profile name for this credential set
        #[arg(long)]
        profile: String,
        /// Login URL
        #[arg(long)]
        url: String,
        /// Username or email
        #[arg(long)]
        username: String,
        /// Password (encrypted at rest with GSD_BROWSER_VAULT_KEY)
        #[arg(long)]
        password: String,
        /// Extra fields as JSON (e.g. field_mappings for custom forms)
        #[arg(long)]
        extra_fields: Option<String>,
    },
    /// Login using a saved vault profile
    VaultLogin {
        /// Profile name to login with
        #[arg(long)]
        profile: String,
    },
    /// List all saved vault profiles (no credentials shown)
    VaultList,
    /// Manage the action cache (stats, get, put, clear)
    ActionCache {
        /// Action: stats, get, put, clear
        #[arg(long, default_value = "stats")]
        action: String,
        /// Intent key (for get/put)
        #[arg(long)]
        intent: Option<String>,
        /// CSS selector to cache (for put)
        #[arg(long)]
        selector: Option<String>,
        /// Confidence score 0–1 (for put, default: 1.0)
        #[arg(long)]
        score: Option<f64>,
    },
    /// Scan page content for prompt injection attempts
    CheckInjection {
        /// Also scan hidden/invisible text (default: true)
        #[arg(long, default_value = "true")]
        include_hidden: bool,
    },
    /// Generate a Playwright test from the recorded action timeline
    GenerateTest {
        /// Test name (default: recorded-session)
        #[arg(long, default_value = "recorded-session")]
        name: String,
        /// Output file path
        #[arg(long)]
        output: Option<String>,
        /// Include assertion steps (default: true)
        #[arg(long, default_value = "true")]
        include_assertions: bool,
    },
    /// Export network logs as a HAR 1.2 JSON file
    HarExport {
        /// Output filename/path
        #[arg(long)]
        filename: Option<String>,
    },
    /// Start a CDP performance trace
    TraceStart {
        /// Optional trace session name
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop an active CDP trace and write data to file
    TraceStop {
        /// Optional output filename override
        #[arg(long)]
        name: Option<String>,
    },
    /// Internal: run the daemon server (hidden, used for auto-start)
    #[command(name = "_serve", hide = true)]
    Serve {
        /// Path to Chrome/Chromium binary
        #[arg(long)]
        browser_path: Option<String>,
        /// CDP WebSocket URL to attach to an already-running Chrome
        #[arg(long)]
        cdp_url: Option<String>,
        /// Named session
        #[arg(long)]
        session: Option<String>,
        /// Browser identity scope: session, project, or global
        #[arg(long)]
        identity_scope: Option<String>,
        /// Browser identity key
        #[arg(long)]
        identity_key: Option<String>,
        /// Project id for project-scoped browser identities
        #[arg(long)]
        identity_project: Option<String>,
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
    let mut cli = Cli::parse();

    // Load config (layers 1-4: defaults → user → project → env vars)
    let config = Config::load();

    // Layer 5: CLI flags override config.
    // If no CLI flag was provided, use config value as fallback.
    if cli.browser_path.is_none() {
        cli.browser_path = config.browser.path.clone();
    }
    if cli.cdp_url.is_none() {
        cli.cdp_url = config.browser.cdp_url.clone();
    }
    if let Err(err) = validate_session_name(cli.session.as_deref()) {
        if cli.json {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "error": {
                        "message": err,
                    }
                }))
                .unwrap()
            );
        } else {
            eprintln!("Error: {err}");
        }
        std::process::exit(1);
    }
    if let Some(scope) = cli.identity_scope.as_deref() {
        if !matches!(scope, "session" | "project" | "global") {
            eprintln!("Error: invalid identity scope: {scope}");
            std::process::exit(1);
        }
        if cli.identity_key.is_none() {
            eprintln!("Error: --identity-scope requires --identity-key");
            std::process::exit(1);
        }
        if scope == "project" && cli.identity_project.is_none() {
            eprintln!("Error: project identity requires --identity-project");
            std::process::exit(1);
        }
        if scope != "project" && cli.identity_project.is_some() {
            eprintln!("Error: --identity-project is only valid with --identity-scope=project");
            std::process::exit(1);
        }
        std::env::set_var("GSD_BROWSER_IDENTITY_SCOPE", scope);
    } else {
        if cli.identity_key.is_some() {
            eprintln!("Error: --identity-key requires --identity-scope");
            std::process::exit(1);
        }
        if cli.identity_project.is_some() {
            eprintln!("Error: --identity-project requires --identity-scope");
            std::process::exit(1);
        }
    }
    if let Some(key) = cli.identity_key.as_deref() {
        std::env::set_var("GSD_BROWSER_IDENTITY_KEY", key);
    }
    if let Some(project_id) = cli.identity_project.as_deref() {
        std::env::set_var("GSD_BROWSER_IDENTITY_PROJECT", project_id);
    }

    let result = match &cli.command {
        Commands::Serve {
            browser_path,
            cdp_url,
            session,
            identity_scope,
            identity_key,
            identity_project,
        } => {
            if let Err(e) = daemon::run(
                browser_path.clone(),
                session.clone(),
                cdp_url.clone(),
                identity_scope.clone(),
                identity_key.clone(),
                identity_project.clone(),
            )
            .await
            {
                eprintln!("[gsd-browser-daemon] fatal: {e}");
                std::process::exit(1);
            }
            Ok(())
        }
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
        Commands::Screenshot {
            selector,
            full_page,
            quality,
            output,
            format,
        } => {
            cmd_screenshot(
                &cli,
                selector.as_deref(),
                *full_page,
                *quality,
                output.as_deref(),
                format,
            )
            .await
        }
        Commands::AccessibilityTree {
            selector,
            max_depth,
            max_count,
        } => cmd_accessibility_tree(&cli, selector.as_deref(), *max_depth, *max_count).await,
        Commands::Find {
            role,
            text,
            selector,
            limit,
        } => {
            cmd_find(
                &cli,
                role.as_deref(),
                text.as_deref(),
                selector.as_deref(),
                *limit,
            )
            .await
        }
        Commands::PageSource { selector } => cmd_page_source(&cli, selector.as_deref()).await,
        Commands::WaitFor {
            condition,
            value,
            threshold,
            timeout,
        } => {
            cmd_wait_for(
                &cli,
                condition,
                value.as_deref(),
                threshold.as_deref(),
                *timeout,
            )
            .await
        }
        Commands::Timeline { write_to_disk } => cmd_timeline(&cli, *write_to_disk).await,
        Commands::Snapshot {
            selector,
            interactive_only,
            limit,
            mode,
        } => {
            cmd_snapshot(
                &cli,
                selector.as_deref(),
                *interactive_only,
                *limit,
                mode.as_deref(),
            )
            .await
        }
        Commands::GetRef { ref_str } => cmd_get_ref(&cli, ref_str).await,
        Commands::ClickRef { ref_str } => cmd_click_ref(&cli, ref_str).await,
        Commands::HoverRef { ref_str } => cmd_hover_ref(&cli, ref_str).await,
        Commands::FillRef {
            ref_str,
            text,
            clear_first,
            submit,
            slowly,
        } => cmd_fill_ref(&cli, ref_str, text, *clear_first, *submit, *slowly).await,
        Commands::Assert { checks } => cmd_assert(&cli, checks).await,
        Commands::Diff { since } => cmd_diff(&cli, *since).await,
        Commands::Batch {
            steps,
            stop_on_failure,
            summary_only,
        } => cmd_batch(&cli, steps, *stop_on_failure, *summary_only).await,
        Commands::ListPages => cmd_list_pages(&cli).await,
        Commands::SwitchPage { id } => cmd_switch_page(&cli, *id).await,
        Commands::ClosePage { id } => cmd_close_page(&cli, *id).await,
        Commands::ListFrames => cmd_list_frames(&cli).await,
        Commands::SelectFrame {
            name,
            index,
            url_pattern,
        } => cmd_select_frame(&cli, name.as_deref(), *index, url_pattern.as_deref()).await,
        Commands::AnalyzeForm { selector } => cmd_analyze_form(&cli, selector.as_deref()).await,
        Commands::FillForm {
            values,
            selector,
            submit,
        } => cmd_fill_form(&cli, values, selector.as_deref(), *submit).await,
        Commands::FindBest { intent, scope } => cmd_find_best(&cli, intent, scope.as_deref()).await,
        Commands::Act { intent, scope } => cmd_act(&cli, intent, scope.as_deref()).await,
        Commands::SessionSummary => cmd_session_summary(&cli).await,
        Commands::DebugBundle { name } => cmd_debug_bundle(&cli, name.as_deref()).await,
        Commands::VisualDiff {
            name,
            selector,
            threshold,
            update_baseline,
        } => {
            cmd_visual_diff(
                &cli,
                name.as_deref(),
                selector.as_deref(),
                *threshold,
                *update_baseline,
            )
            .await
        }
        Commands::ZoomRegion {
            x,
            y,
            width,
            height,
            scale,
        } => cmd_zoom_region(&cli, *x, *y, *width, *height, *scale).await,
        Commands::SavePdf {
            format,
            print_background,
            filename,
            output,
        } => {
            cmd_save_pdf(
                &cli,
                format,
                *print_background,
                filename.as_deref(),
                output.as_deref(),
            )
            .await
        }
        Commands::Extract {
            schema,
            selector,
            multiple,
        } => cmd_extract(&cli, schema, selector.as_deref(), *multiple).await,
        Commands::MockRoute {
            url,
            status,
            body,
            content_type,
            delay,
            headers,
        } => {
            cmd_mock_route(
                &cli,
                url,
                *status,
                body.as_deref(),
                content_type.as_deref(),
                *delay,
                headers.as_deref(),
            )
            .await
        }
        Commands::BlockUrls { patterns } => cmd_block_urls(&cli, patterns).await,
        Commands::ClearRoutes => cmd_clear_routes(&cli).await,
        Commands::EmulateDevice { device } => cmd_emulate_device(&cli, device).await,
        Commands::SaveState { name } => cmd_save_state(&cli, &name).await,
        Commands::RestoreState { name } => cmd_restore_state(&cli, &name).await,
        Commands::VaultSave {
            profile,
            url,
            username,
            password,
            extra_fields,
        } => {
            cmd_vault_save(
                &cli,
                &profile,
                &url,
                &username,
                &password,
                extra_fields.as_deref(),
            )
            .await
        }
        Commands::VaultLogin { profile } => cmd_vault_login(&cli, &profile).await,
        Commands::VaultList => cmd_vault_list(&cli).await,
        Commands::ActionCache {
            action,
            intent,
            selector,
            score,
        } => {
            cmd_action_cache(
                &cli,
                &action,
                intent.as_deref(),
                selector.as_deref(),
                *score,
            )
            .await
        }
        Commands::CheckInjection { include_hidden } => {
            cmd_check_injection(&cli, *include_hidden).await
        }
        Commands::GenerateTest {
            name,
            output,
            include_assertions,
        } => cmd_generate_test(&cli, &name, output.as_deref(), *include_assertions).await,
        Commands::HarExport { filename } => cmd_har_export(&cli, filename.as_deref()).await,
        Commands::TraceStart { name } => cmd_trace_start(&cli, name.as_deref()).await,
        Commands::TraceStop { name } => cmd_trace_stop(&cli, name.as_deref()).await,
    };

    if let Err(e) = result {
        if cli.json {
            let err = serde_json::json!({
                "error": {
                    "message": e.to_string(),
                    "retryHint": "Check daemon status with: gsd-browser daemon health"
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
    daemon_client::start_daemon(
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
        cli.identity_scope.as_deref(),
        cli.identity_key.as_deref(),
        cli.identity_project.as_deref(),
    )
    .await?;
    if cli.json {
        println!("{}", serde_json::json!({"status": "started"}));
    } else {
        println!("Daemon started.");
    }
    Ok(())
}

async fn cmd_daemon_stop(cli: &Cli) -> CmdResult {
    daemon_client::stop_daemon(cli.session.as_deref())?;
    if cli.json {
        println!("{}", serde_json::json!({"status": "stopped"}));
    } else {
        println!("Daemon stopped.");
    }
    Ok(())
}

async fn cmd_daemon_health(cli: &Cli) -> CmdResult {
    let result = daemon_client::collect_health(cli.session.as_deref()).await?;
    if cli.json {
        println!("{}", output::format_json(&result));
    } else if let Some(session) = result.get("session") {
        let status = session
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        println!("Daemon: {status}");
        if let Some(pid) = session.get("daemonPid").and_then(|value| value.as_i64()) {
            println!("PID: {pid}");
        }
        if let Some(reason) = session.get("reason").and_then(|value| value.as_str()) {
            if !reason.is_empty() {
                println!("Reason: {reason}");
            }
        }
    }
    Ok(())
}

async fn cmd_navigate(cli: &Cli, url: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "navigate",
        serde_json::json!({"url": url}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_navigate)
}

async fn cmd_back(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "back",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_back)
}

async fn cmd_forward(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "forward",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_forward)
}

async fn cmd_reload(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "reload",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_reload)
}

async fn cmd_console(cli: &Cli, no_clear: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "console",
        serde_json::json!({"clear": !no_clear}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_console)
}

async fn cmd_network(cli: &Cli, no_clear: bool, filter: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "network",
        serde_json::json!({"clear": !no_clear, "filter": filter}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_network)
}

async fn cmd_dialog(cli: &Cli, no_clear: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "dialog",
        serde_json::json!({"clear": !no_clear}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_dialog)
}

async fn cmd_eval(cli: &Cli, expression: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "eval",
        serde_json::json!({"expression": expression}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
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
    let resp = daemon_client::send_request(
        "click",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
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
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_press(cli: &Cli, key: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "press",
        serde_json::json!({"key": key}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_hover(cli: &Cli, selector: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "hover",
        serde_json::json!({"selector": selector}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_scroll(cli: &Cli, direction: &str, amount: i32) -> CmdResult {
    let resp = daemon_client::send_request(
        "scroll",
        serde_json::json!({"direction": direction, "amount": amount}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_scroll)
}

async fn cmd_select_option(cli: &Cli, selector: &str, option: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "select_option",
        serde_json::json!({"selector": selector, "option": option}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_set_checked(cli: &Cli, selector: &str, checked: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "set_checked",
        serde_json::json!({"selector": selector, "checked": checked}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_drag(cli: &Cli, source: &str, target: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "drag",
        serde_json::json!({"source": source, "target": target}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
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
    let resp = daemon_client::send_request(
        "set_viewport",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_viewport)
}

async fn cmd_upload_file(cli: &Cli, selector: &str, files: &[String]) -> CmdResult {
    let resp = daemon_client::send_request(
        "upload_file",
        serde_json::json!({"selector": selector, "files": files}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_interaction)
}

async fn cmd_screenshot(
    cli: &Cli,
    selector: Option<&str>,
    full_page: bool,
    quality: u32,
    output_path: Option<&str>,
    format: &str,
) -> CmdResult {
    let mut params = serde_json::json!({
        "full_page": full_page,
        "quality": quality,
        "format": format,
    });
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }

    let resp = daemon_client::send_request(
        "screenshot",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;

    if let Some(err) = resp.error {
        if cli.json {
            eprintln!("{}", output::format_error_json(&err));
        } else {
            eprintln!("{}", output::format_error_text(&err));
        }
        std::process::exit(1);
    }

    if let Some(result) = resp.result {
        // If --output is specified, decode base64 and write raw bytes to file
        if let Some(path) = output_path {
            let data_b64 = result
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or("screenshot response missing 'data' field")?;

            use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
            let bytes = BASE64
                .decode(data_b64)
                .map_err(|e| format!("failed to decode base64: {e}"))?;

            std::fs::write(path, &bytes)
                .map_err(|e| format!("failed to write screenshot to '{path}': {e}"))?;

            let width = result.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            let height = result.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("Screenshot saved to {path} ({width}x{height})");
        } else if cli.json {
            println!("{}", output::format_json(&result));
        } else {
            println!("{}", output::format_text_screenshot(&result));
        }
    }

    Ok(())
}

async fn cmd_accessibility_tree(
    cli: &Cli,
    selector: Option<&str>,
    max_depth: u32,
    max_count: u32,
) -> CmdResult {
    let mut params = serde_json::json!({
        "max_depth": max_depth,
        "max_count": max_count,
    });
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    let resp = daemon_client::send_request(
        "accessibility_tree",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_accessibility_tree)
}

async fn cmd_find(
    cli: &Cli,
    role: Option<&str>,
    text: Option<&str>,
    selector: Option<&str>,
    limit: u32,
) -> CmdResult {
    let mut params = serde_json::json!({"limit": limit});
    if let Some(r) = role {
        params["role"] = serde_json::json!(r);
    }
    if let Some(t) = text {
        params["text"] = serde_json::json!(t);
    }
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    let resp = daemon_client::send_request(
        "find",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_find)
}

async fn cmd_page_source(cli: &Cli, selector: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    let resp = daemon_client::send_request(
        "page_source",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_page_source)
}

async fn cmd_wait_for(
    cli: &Cli,
    condition: &str,
    value: Option<&str>,
    threshold: Option<&str>,
    timeout: Option<u64>,
) -> CmdResult {
    let mut params = serde_json::json!({"condition": condition});
    if let Some(v) = value {
        params["value"] = serde_json::json!(v);
    }
    if let Some(t) = threshold {
        params["threshold"] = serde_json::json!(t);
    }
    if let Some(ms) = timeout {
        params["timeout"] = serde_json::json!(ms);
    }
    let resp = daemon_client::send_request(
        "wait_for",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_wait_for)
}

async fn cmd_timeline(cli: &Cli, write_to_disk: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "timeline",
        serde_json::json!({"write_to_disk": write_to_disk}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_timeline)
}

async fn cmd_snapshot(
    cli: &Cli,
    selector: Option<&str>,
    interactive_only: bool,
    limit: u32,
    mode: Option<&str>,
) -> CmdResult {
    let mut params = serde_json::json!({
        "interactive_only": interactive_only,
        "limit": limit,
    });
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    if let Some(m) = mode {
        params["mode"] = serde_json::json!(m);
    }
    let resp = daemon_client::send_request(
        "snapshot",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_snapshot)
}

async fn cmd_get_ref(cli: &Cli, ref_str: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "get_ref",
        serde_json::json!({"ref": ref_str}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_get_ref)
}

async fn cmd_click_ref(cli: &Cli, ref_str: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "click_ref",
        serde_json::json!({"ref": ref_str}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_ref_action)
}

async fn cmd_hover_ref(cli: &Cli, ref_str: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "hover_ref",
        serde_json::json!({"ref": ref_str}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_ref_action)
}

async fn cmd_fill_ref(
    cli: &Cli,
    ref_str: &str,
    text: &str,
    clear_first: bool,
    submit: bool,
    slowly: bool,
) -> CmdResult {
    let resp = daemon_client::send_request(
        "fill_ref",
        serde_json::json!({
            "ref": ref_str,
            "text": text,
            "clear_first": clear_first,
            "submit": submit,
            "slowly": slowly,
        }),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_ref_action)
}

async fn cmd_assert(cli: &Cli, checks: &str) -> CmdResult {
    // Parse the checks JSON to validate it before sending
    let checks_value: serde_json::Value =
        serde_json::from_str(checks).map_err(|e| format!("invalid checks JSON: {e}"))?;
    let resp = daemon_client::send_request(
        "assert",
        serde_json::json!({"checks": checks_value}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_assert)
}

async fn cmd_diff(cli: &Cli, since: Option<u64>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(id) = since {
        params["sinceActionId"] = serde_json::json!(id);
    }
    let resp = daemon_client::send_request(
        "diff",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_diff)
}

async fn cmd_batch(cli: &Cli, steps: &str, stop_on_failure: bool, summary_only: bool) -> CmdResult {
    // Parse the steps JSON to validate it before sending
    let steps_value: serde_json::Value =
        serde_json::from_str(steps).map_err(|e| format!("invalid steps JSON: {e}"))?;
    let resp = daemon_client::send_request(
        "batch",
        serde_json::json!({
            "steps": steps_value,
            "stop_on_failure": stop_on_failure,
            "summary_only": summary_only,
        }),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_batch)
}

async fn cmd_list_pages(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "list_pages",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_list_pages)
}

async fn cmd_switch_page(cli: &Cli, id: u64) -> CmdResult {
    let resp = daemon_client::send_request(
        "switch_page",
        serde_json::json!({"id": id}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_switch_page)
}

async fn cmd_close_page(cli: &Cli, id: u64) -> CmdResult {
    let resp = daemon_client::send_request(
        "close_page",
        serde_json::json!({"id": id}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_close_page)
}

async fn cmd_list_frames(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "list_frames",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_list_frames)
}

async fn cmd_select_frame(
    cli: &Cli,
    name: Option<&str>,
    index: Option<u64>,
    url_pattern: Option<&str>,
) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(n) = name {
        params["name"] = serde_json::json!(n);
    }
    if let Some(idx) = index {
        params["index"] = serde_json::json!(idx);
    }
    if let Some(pat) = url_pattern {
        params["urlPattern"] = serde_json::json!(pat);
    }
    let resp = daemon_client::send_request(
        "select_frame",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_select_frame)
}

async fn cmd_analyze_form(cli: &Cli, selector: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    let resp = daemon_client::send_request(
        "analyze_form",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_analyze_form)
}

async fn cmd_fill_form(cli: &Cli, values: &str, selector: Option<&str>, submit: bool) -> CmdResult {
    let values_value: serde_json::Value =
        serde_json::from_str(values).map_err(|e| format!("invalid values JSON: {e}"))?;
    let mut params = serde_json::json!({
        "values": values_value,
        "submit": submit,
    });
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    let resp = daemon_client::send_request(
        "fill_form",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_fill_form)
}

async fn cmd_find_best(cli: &Cli, intent: &str, scope: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({"intent": intent});
    if let Some(s) = scope {
        params["scope"] = serde_json::json!(s);
    }
    let resp = daemon_client::send_request(
        "find_best",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_find_best)
}

async fn cmd_act(cli: &Cli, intent: &str, scope: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({"intent": intent});
    if let Some(s) = scope {
        params["scope"] = serde_json::json!(s);
    }
    let resp = daemon_client::send_request(
        "act",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_act)
}

async fn cmd_session_summary(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "session_summary",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_session_summary)
}

async fn cmd_debug_bundle(cli: &Cli, name: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(n) = name {
        params["name"] = serde_json::json!(n);
    }
    let resp = daemon_client::send_request(
        "debug_bundle",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_debug_bundle)
}

async fn cmd_visual_diff(
    cli: &Cli,
    name: Option<&str>,
    selector: Option<&str>,
    threshold: Option<f64>,
    update_baseline: bool,
) -> CmdResult {
    let mut params = serde_json::json!({
        "update_baseline": update_baseline,
    });
    if let Some(n) = name {
        params["name"] = serde_json::json!(n);
    }
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    if let Some(t) = threshold {
        params["threshold"] = serde_json::json!(t);
    }
    let resp = daemon_client::send_request(
        "visual_diff",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_visual_diff)
}

async fn cmd_zoom_region(
    cli: &Cli,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
) -> CmdResult {
    let resp = daemon_client::send_request(
        "zoom_region",
        serde_json::json!({
            "x": x,
            "y": y,
            "width": width,
            "height": height,
            "scale": scale,
        }),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_zoom_region)
}

async fn cmd_save_pdf(
    cli: &Cli,
    format: &str,
    print_background: bool,
    filename: Option<&str>,
    output: Option<&str>,
) -> CmdResult {
    let mut params = serde_json::json!({
        "format": format,
        "print_background": print_background,
    });
    if let Some(f) = filename {
        params["filename"] = serde_json::json!(f);
    }
    if let Some(o) = output {
        params["output"] = serde_json::json!(o);
    }
    let resp = daemon_client::send_request(
        "save_pdf",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_save_pdf)
}

async fn cmd_extract(cli: &Cli, schema: &str, selector: Option<&str>, multiple: bool) -> CmdResult {
    let schema_value: serde_json::Value =
        serde_json::from_str(schema).map_err(|e| format!("invalid schema JSON: {e}"))?;
    let mut params = serde_json::json!({
        "schema": schema_value,
        "multiple": multiple,
    });
    if let Some(sel) = selector {
        params["selector"] = serde_json::json!(sel);
    }
    let resp = daemon_client::send_request(
        "extract",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_extract)
}

async fn cmd_mock_route(
    cli: &Cli,
    url: &str,
    status: u16,
    body: Option<&str>,
    content_type: Option<&str>,
    delay: Option<u64>,
    headers: Option<&str>,
) -> CmdResult {
    let mut params = serde_json::json!({
        "url": url,
        "status": status,
    });
    if let Some(b) = body {
        params["body"] = serde_json::json!(b);
    }
    if let Some(ct) = content_type {
        params["content_type"] = serde_json::json!(ct);
    }
    if let Some(d) = delay {
        params["delay"] = serde_json::json!(d);
    }
    if let Some(h) = headers {
        let headers_value: serde_json::Value =
            serde_json::from_str(h).map_err(|e| format!("invalid headers JSON: {e}"))?;
        params["headers"] = headers_value;
    }
    let resp = daemon_client::send_request(
        "mock_route",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_mock_route)
}

async fn cmd_block_urls(cli: &Cli, patterns: &[String]) -> CmdResult {
    let resp = daemon_client::send_request(
        "block_urls",
        serde_json::json!({"patterns": patterns}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_block_urls)
}

async fn cmd_clear_routes(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "clear_routes",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_clear_routes)
}

async fn cmd_emulate_device(cli: &Cli, device: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "emulate_device",
        serde_json::json!({"device": device}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_emulate_device)
}

async fn cmd_save_state(cli: &Cli, name: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "save_state",
        serde_json::json!({"name": name}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_save_state)
}

async fn cmd_restore_state(cli: &Cli, name: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "restore_state",
        serde_json::json!({"name": name}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_restore_state)
}

async fn cmd_vault_save(
    cli: &Cli,
    profile: &str,
    url: &str,
    username: &str,
    password: &str,
    extra_fields: Option<&str>,
) -> CmdResult {
    let mut params = serde_json::json!({
        "profile": profile,
        "url": url,
        "username": username,
        "password": password,
    });
    if let Some(ef) = extra_fields {
        let ef_value: serde_json::Value =
            serde_json::from_str(ef).map_err(|e| format!("invalid extra_fields JSON: {e}"))?;
        params["extra_fields"] = ef_value;
    }
    let resp = daemon_client::send_request(
        "vault_save",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_vault_save)
}

async fn cmd_vault_login(cli: &Cli, profile: &str) -> CmdResult {
    let resp = daemon_client::send_request(
        "vault_login",
        serde_json::json!({"profile": profile}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_vault_login)
}

async fn cmd_vault_list(cli: &Cli) -> CmdResult {
    let resp = daemon_client::send_request(
        "vault_list",
        serde_json::json!({}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_vault_list)
}

async fn cmd_action_cache(
    cli: &Cli,
    action: &str,
    intent: Option<&str>,
    selector: Option<&str>,
    score: Option<f64>,
) -> CmdResult {
    let mut params = serde_json::json!({"action": action});
    if let Some(i) = intent {
        params["intent"] = serde_json::json!(i);
    }
    if let Some(s) = selector {
        params["selector"] = serde_json::json!(s);
    }
    if let Some(sc) = score {
        params["score"] = serde_json::json!(sc);
    }
    let resp = daemon_client::send_request(
        "action_cache",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_action_cache)
}

async fn cmd_check_injection(cli: &Cli, include_hidden: bool) -> CmdResult {
    let resp = daemon_client::send_request(
        "check_injection",
        serde_json::json!({"includeHidden": include_hidden}),
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_check_injection)
}

async fn cmd_generate_test(
    cli: &Cli,
    name: &str,
    output_path: Option<&str>,
    include_assertions: bool,
) -> CmdResult {
    let mut params = serde_json::json!({
        "name": name,
        "includeAssertions": include_assertions,
    });
    if let Some(o) = output_path {
        params["outputPath"] = serde_json::json!(o);
    }
    let resp = daemon_client::send_request(
        "generate_test",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_generate_test)
}

async fn cmd_har_export(cli: &Cli, filename: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(f) = filename {
        params["filename"] = serde_json::json!(f);
    }
    let resp = daemon_client::send_request(
        "har_export",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_har_export)
}

async fn cmd_trace_start(cli: &Cli, name: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(n) = name {
        params["name"] = serde_json::json!(n);
    }
    let resp = daemon_client::send_request(
        "trace_start",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_trace_start)
}

async fn cmd_trace_stop(cli: &Cli, name: Option<&str>) -> CmdResult {
    let mut params = serde_json::json!({});
    if let Some(n) = name {
        params["name"] = serde_json::json!(n);
    }
    let resp = daemon_client::send_request(
        "trace_stop",
        params,
        cli.browser_path.as_deref(),
        cli.cdp_url.as_deref(),
        cli.session.as_deref(),
    )
    .await?;
    handle_response(cli, resp, output::format_text_trace_stop)
}

/// Generic response handler — delegates to the appropriate formatter based on --json flag.
fn handle_response(
    cli: &Cli,
    resp: gsd_browser_common::DaemonResponse,
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
