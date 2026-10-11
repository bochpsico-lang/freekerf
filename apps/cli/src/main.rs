//! `freekerf`: the headless command line of FreeKerf.
//!
//! A thin shell over `freekerf-service`: every operation is an action of the
//! registry (see `doc/features/cli.md`).

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use freekerf_service::analysis::{AnalyzeOptions, analyze};
use freekerf_service::{Caller, CallerKind, Service, Stores, Tier};
use serde_json::{Value, json};

mod host;

#[derive(Parser)]
#[command(
    name = "freekerf",
    version,
    about = "FreeKerf: design jobs and drive laser machines (headless)"
)]
struct Cli {
    /// Directory for settings and data (default: the user's directories; `$FREEKERF_HOME`).
    #[arg(long, global = true, value_name = "DIR")]
    home: Option<PathBuf>,
    /// Log level for diagnostics on stderr (error, warn, info, debug, trace).
    #[arg(long, global = true, default_value = "warn")]
    log: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Analyzes a G-code file: estimated time, ranges, quadrant.
    Analyze {
        /// G-code file.
        file: PathBuf,
        /// Print the JSON report (same format as the conformance golden files).
        #[arg(long)]
        json: bool,
        /// Maximum X rate ($110) used by the time estimate, mm/min.
        #[arg(long, default_value_t = 4000)]
        max_rate: i64,
    },
    /// Streams a G-code file to the machine and waits for the end of the job.
    Send {
        /// G-code file.
        file: PathBuf,
        #[command(flatten)]
        target: Target,
        /// Number of passes.
        #[arg(long, default_value_t = 1)]
        passes: u32,
        /// Home ($H) before the job.
        #[arg(long)]
        homing: bool,
    },
    /// Interactive console: type G-code; `:help` lists the commands.
    Console {
        #[command(flatten)]
        target: Target,
    },
    /// Runs the built-in Grbl 1.1 emulator.
    Emulate {
        /// Expose it on a pseudo terminal and print its path (Unix).
        #[arg(long)]
        pty: bool,
        /// Execute motions instantly instead of taking their time.
        #[arg(long)]
        instant: bool,
    },
    /// Host protocol on stdin/stdout (JSON Lines), used by tests and tools.
    Host {
        /// Use stdin/stdout (the only mode).
        #[arg(long)]
        stdio: bool,
        /// Use the settings files instead of in-memory defaults.
        #[arg(long)]
        persistent: bool,
    },
    /// Serves this instance on the local endpoint for `freekerf mcp` (one instance
    /// per user). Ctrl-C sends STOP and exits.
    Serve {
        /// No user interface (the only mode until the desktop app): permission
        /// requests and laser actions are refused.
        #[arg(long)]
        headless: bool,
        /// Exit after this many seconds without clients (instances started by
        /// `freekerf mcp`).
        #[arg(long, value_name = "SECONDS")]
        idle_exit: Option<u64>,
    },
    /// MCP server on stdin/stdout for AI agents: attaches to the running instance,
    /// or starts a headless one. Needs `automation.mcp.enabled` (off by default).
    Mcp {
        /// Seconds to wait for a started instance to listen.
        #[arg(long, default_value_t = 10)]
        launch_timeout: u64,
        /// Seconds a started instance stays up without clients.
        #[arg(long, default_value_t = 60)]
        idle_exit: u64,
    },
    /// Imports data from another program (read only on its side).
    Migrate {
        #[command(subcommand)]
        from: MigrateFrom,
    },
    /// Reads or changes the settings file.
    Config {
        #[command(subcommand)]
        op: ConfigOp,
    },
    /// Lists the actions of the registry.
    Actions {
        /// As JSON (with parameter schemas).
        #[arg(long)]
        json: bool,
        /// As the Markdown catalogue (`doc/features/actions.md`).
        #[arg(long)]
        markdown: bool,
    },
    /// Runs a conformance importer golden case and prints its JSON result
    /// (`conformance` compares it with LaserGRBL's golden file).
    ImportCase {
        /// Case file (`fixtures/importers/cases/*.json`), `-` for stdin.
        case: Option<PathBuf>,
        /// Directory of the cases' input files (default: `../inputs` next to the case).
        #[arg(long)]
        inputs: Option<PathBuf>,
        /// Print the case kinds this build runs, one per line, and exit.
        #[arg(long)]
        kinds: bool,
    },
    /// Runs one action, or a JavaScript file (`*.js`), and prints its JSON result.
    Run {
        /// Action name (`state.get`) or script file (`macro.js`).
        action: String,
        /// Parameters as JSON (actions only).
        #[arg(default_value = "{}")]
        params: String,
        /// Scripts: run against a copy of the document and simulate machine,
        /// laser and file actions; nothing is applied.
        #[arg(long)]
        dry_run: bool,
        /// Scripts: highest tier the script may use (`read`, `ui`, `edit`,
        /// `machine`; `laser` with `--dry-run` only). Laser actions never run from
        /// the command line.
        #[arg(long, default_value = "machine")]
        tier: String,
        /// Scripts: wall-clock limit in seconds.
        #[arg(long, default_value_t = 30)]
        timeout: u64,
    },
}

#[derive(Subcommand)]
enum MigrateFrom {
    /// LaserGRBL: settings, laser module counters and custom buttons (as scripts)
    /// from its data folder (`%APPDATA%\LaserGRBL` by default on Windows). Without
    /// `--apply` nothing is written.
    Lasergrbl {
        /// LaserGRBL's data folder.
        dir: Option<PathBuf>,
        /// Write the result into FreeKerf's settings and machine cache.
        #[arg(long)]
        apply: bool,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// LaserGRBL custom buttons from a `.zbn` export (or `CustomButtons.bin`), as
    /// scripts in FreeKerf's macro folder. Without `--apply` nothing is written.
    LasergrblButtons {
        /// The button file.
        file: PathBuf,
        /// Write the scripts.
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum ConfigOp {
    /// Prints the path of the settings file.
    Path,
    /// Prints a setting (`machine.baud`) or all of them.
    Get {
        /// Dotted key.
        key: Option<String>,
    },
    /// Sets a setting; the value is JSON (`true`, `250000`, `"COM3"`) or plain text.
    Set {
        /// Dotted key.
        key: String,
        /// Value.
        value: String,
    },
    /// Prints the JSON schema of the settings file.
    Schema,
}

#[derive(Args, Clone)]
struct Target {
    /// Serial port (default: the last one used).
    #[arg(long)]
    port: Option<String>,
    /// Baud rate (default: from the settings).
    #[arg(long)]
    baud: Option<u32>,
    /// Use the built-in emulator instead of a serial port.
    #[arg(long, conflicts_with = "port")]
    emulator: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_new(&cli.log)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let home = cli.home.clone();
    match cli.command {
        Command::Analyze { file, json, max_rate } => cmd_analyze(&file, json, max_rate),
        Command::ImportCase { case, inputs, kinds } => {
            cmd_import_case(case.as_deref(), inputs.as_deref(), kinds)
        }
        Command::Send {
            file,
            target,
            passes,
            homing,
        } => cmd_send(home.as_deref(), &file, &target, passes, homing),
        Command::Console { target } => cmd_console(home.as_deref(), &target),
        Command::Emulate { pty, instant } => cmd_emulate(pty, !instant),
        Command::Host { stdio, persistent } => {
            if !stdio {
                bail!("only `--stdio` is supported");
            }
            let service = if persistent {
                open_service(home.as_deref())?
            } else {
                Service::in_memory()?
            };
            host::serve(&service, std::io::stdin().lock(), std::io::stdout().lock())?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Config { op } => cmd_config(home.as_deref(), op),
        Command::Serve { headless, idle_exit } => cmd_serve(home.as_deref(), headless, idle_exit),
        Command::Mcp {
            launch_timeout,
            idle_exit,
        } => cmd_mcp(home, launch_timeout, idle_exit),
        Command::Migrate { from } => cmd_migrate(home.as_deref(), from),
        Command::Actions { json, markdown } => {
            let service = Service::in_memory()?;
            let mut out = std::io::stdout().lock();
            if json {
                writeln!(out, "{}", serde_json::to_string_pretty(&service.specs())?)?;
            } else if markdown {
                write!(out, "{}", service.registry().to_markdown())?;
            } else {
                for s in service.specs() {
                    writeln!(
                        out,
                        "{:<24} {:<8} {}",
                        s.name,
                        s.tier,
                        s.description.lines().next().unwrap_or("")
                    )?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Run {
            action,
            dry_run,
            tier,
            timeout,
            ..
        } if action.ends_with(".js") => cmd_run_script(home.as_deref(), &action, dry_run, &tier, timeout),
        Command::Run { action, params, .. } => {
            let service = open_service(home.as_deref())?;
            let params: Value = serde_json::from_str(&params).context("parameters must be JSON")?;
            let result = service.dispatch(&Caller::user(), &action, params)?;
            writeln!(
                std::io::stdout().lock(),
                "{}",
                serde_json::to_string_pretty(&result)?
            )?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `freekerf run <script.js>`: one transaction, laser refused (no one to confirm),
/// Ctrl-C cancels the script and stops the machine.
fn cmd_run_script(
    home: Option<&std::path::Path>,
    file: &str,
    dry_run: bool,
    tier: &str,
    timeout: u64,
) -> Result<ExitCode> {
    use freekerf_service::script::ScriptRequest;
    let max_tier: Tier = serde_json::from_value(Value::String(tier.to_owned()))
        .map_err(|_| anyhow!("unknown tier `{tier}` (read, ui, edit, machine)"))?;
    if max_tier == Tier::Laser && !dry_run {
        // a dry run only simulates them
        bail!(
            "laser actions need a person to confirm each one; scripts cannot run them from the command line (use --dry-run to preview)"
        );
    }
    let code = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
    let name = std::path::Path::new(file)
        .file_name()
        .map_or_else(|| file.to_owned(), |n| n.to_string_lossy().into_owned());
    let service = open_service(home)?;
    install_stop_on_ctrl_c(&service);
    let mut request = ScriptRequest::new(name.clone(), code);
    request.dry_run = dry_run;
    request.limits.timeout = Duration::from_secs(timeout);
    let report = service.run_script(&Caller::new(CallerKind::Macro, name, max_tier), &request);
    writeln!(
        std::io::stdout().lock(),
        "{}",
        serde_json::to_string_pretty(&report)?
    )?;
    Ok(if report.outcome.error.is_some() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_serve(home: Option<&std::path::Path>, headless: bool, idle_exit: Option<u64>) -> Result<ExitCode> {
    use freekerf_service::remote::{ServeOptions, endpoint, serve};
    if !headless {
        bail!("only `--headless` is available until the desktop app");
    }
    let service = open_service(home)?;
    install_stop_on_ctrl_c(&service);
    let endpoint = endpoint(home).map_err(|e| anyhow!(e))?;
    tracing::info!(endpoint = %endpoint.name, "serving");
    serve(
        &service,
        &endpoint,
        ServeOptions {
            idle_exit: idle_exit.map(Duration::from_secs),
        },
    )?;
    Ok(ExitCode::SUCCESS)
}

/// `freekerf mcp`: refuses at once while MCP is disabled; otherwise attaches to the
/// running instance (or starts `freekerf serve --headless`) when the agent
/// initializes, and proxies MCP on stdin/stdout.
fn cmd_mcp(home: Option<PathBuf>, launch_timeout: u64, idle_exit: u64) -> Result<ExitCode> {
    use freekerf_service::remote::{MCP_DISABLED, endpoint};
    let settings = match Stores::settings_path(home.as_deref()) {
        Some(path) => freekerf_settings::Settings::load(&freekerf_service::FileStore(path))?,
        None => freekerf_settings::Settings::default(),
    };
    if !settings.automation.mcp.enabled {
        bail!(MCP_DISABLED);
    }
    let endpoint = endpoint(home.as_deref()).map_err(|e| anyhow!(e))?;
    let exe = std::env::current_exe().context("locating the freekerf binary")?;
    let launch = move || {
        let mut c = std::process::Command::new(exe);
        if let Some(h) = &home {
            c.arg("--home").arg(h);
        }
        c.args(["serve", "--headless", "--idle-exit", &idle_exit.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        detach(&mut c);
        c.spawn().map(|_| ())
    };
    let connect: freekerf_mcp::Connect = Box::new(move |name: &str| {
        freekerf_mcp::attach_or_launch(&endpoint, name, launch, Duration::from_secs(launch_timeout))
    });
    freekerf_mcp::serve(connect, std::io::stdin().lock(), std::io::stdout())?;
    Ok(ExitCode::SUCCESS)
}

/// The started instance outlives `freekerf mcp` (its own process group).
fn detach(c: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        c.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
}

fn open_service(home: Option<&std::path::Path>) -> Result<Service> {
    let stores = Stores::files(home).map_err(|e| anyhow!(e))?;
    Ok(Service::new(stores)?)
}

fn cmd_import_case(
    case: Option<&std::path::Path>,
    inputs: Option<&std::path::Path>,
    kinds: bool,
) -> Result<ExitCode> {
    use freekerf_service::import_case;
    let mut out = std::io::stdout().lock();
    if kinds {
        for k in import_case::SUPPORTED_KINDS {
            writeln!(out, "{k}")?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    let case = case.ok_or_else(|| anyhow!("a case file is needed (or --kinds)"))?;
    let text = if case.as_os_str() == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        std::fs::read_to_string(case).with_context(|| format!("cannot read {}", case.display()))?
    };
    let value: Value = serde_json::from_str(&text).context("the case is not JSON")?;
    let dir = match inputs {
        Some(d) => d.to_path_buf(),
        None => case
            .parent()
            .and_then(std::path::Path::parent)
            .map_or_else(|| PathBuf::from("inputs"), |p| p.join("inputs")),
    };
    let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| e.to_string());
    let result = import_case::run(&value, &read)?;
    writeln!(out, "{}", serde_json::to_string(&result)?)?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_analyze(file: &std::path::Path, json: bool, max_rate: i64) -> Result<ExitCode> {
    let bytes = std::fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
    let name = file.file_name().map_or_else(
        || file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let opts = AnalyzeOptions {
        max_rate_x_mm_min: max_rate.into(),
        ..AnalyzeOptions::default()
    };
    let report = analyze(&name, &bytes, &opts);
    let mut out = std::io::stdout().lock();
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&report)?)?;
    } else {
        writeln!(
            out,
            "{}: {} commands, estimated {:.1} s",
            report.input,
            report.commands.len(),
            report.estimated_time_s
        )?;
        match report.drawing_range {
            Some([x0, y0, x1, y1]) => writeln!(
                out,
                "drawing: X {x0}..{x1}  Y {y0}..{y1}  ({:.3} x {:.3} mm), quadrant {}",
                x1 - x0,
                y1 - y0,
                report.quadrant
            )?,
            None => writeln!(out, "drawing: nothing burns")?,
        }
        if let Some(s) = report.spindle_range {
            writeln!(out, "power: S {}..{}", s.min, s.max)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn connect(service: &Service, target: &Target) -> Result<()> {
    let user = Caller::user();
    let params = json!({"port": target.port, "baud": target.baud, "emulator": target.emulator});
    service.dispatch(&user, "machine.connect", params)?;
    wait(service, Duration::from_secs(15), |st| {
        st["machine"]["ready"] == true
    })
    .context("the machine did not become ready")?;
    Ok(())
}

fn state(service: &Service) -> Result<Value> {
    Ok(service.dispatch(&Caller::user(), "state.get", Value::Null)?)
}

fn wait(service: &Service, timeout: Duration, pred: impl Fn(&Value) -> bool) -> Result<Value> {
    let end = Instant::now() + timeout;
    let events = service.subscribe();
    loop {
        let st = state(service)?;
        if pred(&st) {
            return Ok(st);
        }
        if Instant::now() >= end {
            bail!("timed out");
        }
        let _ = events.recv_timeout(Duration::from_millis(50));
    }
}

fn install_stop_on_ctrl_c(service: &Service) {
    let s = service.clone();
    let _ = ctrlc::set_handler(move || {
        // STOP the machine before leaving: never leave a job running unattended
        s.stop();
        let _ = writeln!(std::io::stderr(), "\nstopped (soft reset sent)");
        std::thread::sleep(Duration::from_millis(200));
        std::process::exit(130);
    });
}

fn cmd_send(
    home: Option<&std::path::Path>,
    file: &std::path::Path,
    target: &Target,
    passes: u32,
    homing: bool,
) -> Result<ExitCode> {
    let service = open_service(home)?;
    install_stop_on_ctrl_c(&service);
    let user = Caller::user();
    connect(&service, target)?;
    let info = service.dispatch(&user, "job.load", json!({"path": file.to_string_lossy()}))?;
    let mut err = std::io::stderr().lock();
    writeln!(
        err,
        "{}: {} lines, estimated {:.1} s",
        info["name"].as_str().unwrap_or(""),
        info["lines"],
        info["estimated_ms"].as_f64().unwrap_or(0.0) / 1000.0
    )?;
    let events = service.subscribe();
    service.dispatch(&user, "job.run", json!({"passes": passes, "homing": homing}))?;
    let mut result = None;
    let mut last_print = Instant::now();
    while result.is_none() {
        if let Ok(e) = events.recv_timeout(Duration::from_millis(500)) {
            let j = e.to_json();
            if j["event"] == "disconnected" {
                bail!("the machine disconnected");
            }
            if let Some(end) = j["data"].get("JobEnded") {
                result = Some(end.clone());
            }
            if let Some(issue) = j["data"].get("Issue") {
                writeln!(err, "issue: {issue}")?;
            }
        }
        if last_print.elapsed() > Duration::from_secs(2) {
            last_print = Instant::now();
            let st = state(&service)?;
            let m = &st["machine"];
            writeln!(
                err,
                "{} {}/{} lines",
                m["status"].as_str().unwrap_or(""),
                m["job_executed"],
                m["job_target"]
            )?;
        }
    }
    let r = result.unwrap_or(Value::Null);
    // wait for the board to finish what it buffered
    let _ = wait(&service, Duration::from_secs(3600), |st| {
        st["machine"]["status"] == "Idle"
    });
    let errors = r["errors"].as_u64().unwrap_or(0);
    writeln!(
        err,
        "done: {} lines, {} errors, {:.1} s",
        r["lines"],
        errors,
        r["time_ms"].as_f64().unwrap_or(0.0) / 1000.0
    )?;
    let issues = state(&service)?["issues"].as_array().map_or(0, Vec::len);
    service.dispatch(&user, "machine.disconnect", Value::Null)?;
    Ok(if errors > 0 || issues > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

const CONSOLE_HELP: &str = "\
Type G-code or $ commands to send them. Commands:
  :stop      soft reset (STOP)          :hold      feed hold
  :resume    cycle start                :unlock    $X
  :home      $H                         :state     machine state (JSON)
  :jog DIR [STEP]   jog (N S E W NE NW SE SW Zup Zdown Home)
  :run ACTION [JSON]   any action of the registry
  :quit      disconnect and exit";

fn cmd_console(home: Option<&std::path::Path>, target: &Target) -> Result<ExitCode> {
    let service = open_service(home)?;
    install_stop_on_ctrl_c(&service);
    let events = service.subscribe();
    let (printed_tx, printed_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::spawn(move || {
        for e in events {
            if let Some(line) = e.console_line() {
                let _ = writeln!(std::io::stdout().lock(), "{line}");
            }
            // events arrive in order: once the disconnection is printed, so is everything before it
            if e.to_json()["event"] == "disconnected" {
                let _ = printed_tx.try_send(());
            }
        }
    });
    connect(&service, target)?;
    writeln!(std::io::stdout().lock(), "{CONSOLE_HELP}")?;
    let user = Caller::user();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (action, params) = match line.split_once(' ').unwrap_or((line, "")) {
            (":quit", _) => break,
            (":help", _) => {
                writeln!(std::io::stdout().lock(), "{CONSOLE_HELP}")?;
                continue;
            }
            (":stop", _) => ("machine.stop", Value::Null),
            (":hold", _) => ("machine.feed_hold", Value::Null),
            (":resume", _) => ("machine.resume", Value::Null),
            (":unlock", _) => ("machine.unlock", Value::Null),
            (":home", _) => ("machine.home", Value::Null),
            (":state", _) => ("state.get", Value::Null),
            (":jog", rest) => {
                let mut it = rest.split_whitespace();
                let dir = it.next().unwrap_or("");
                let step: Option<f64> = it.next().and_then(|s| s.parse().ok());
                ("machine.jog", json!({"direction": dir, "step_mm": step}))
            }
            (":run", rest) => {
                let (name, p) = rest.split_once(' ').unwrap_or((rest, "{}"));
                match serde_json::from_str(p) {
                    Ok(v) => (name, v),
                    Err(e) => {
                        writeln!(std::io::stderr(), "parameters must be JSON: {e}")?;
                        continue;
                    }
                }
            }
            _ if line.starts_with(':') => {
                writeln!(std::io::stderr(), "unknown command; :help")?;
                continue;
            }
            _ => ("machine.send", json!({"line": line})),
        };
        match service.dispatch(&user, action, params) {
            Ok(Value::Null) => {}
            Ok(v) => writeln!(std::io::stdout().lock(), "{}", serde_json::to_string_pretty(&v)?)?,
            Err(e) => writeln!(std::io::stderr(), "{e}")?,
        }
    }
    let _ = service.dispatch(&user, "machine.disconnect", Value::Null);
    // let the printer thread show the last rows before the process exits
    let _ = printed_rx.recv_timeout(Duration::from_secs(5));
    Ok(ExitCode::SUCCESS)
}

#[cfg(unix)]
fn cmd_emulate(pty: bool, timed: bool) -> Result<ExitCode> {
    if !pty {
        bail!("use `--pty` (the emulator is also available in-process with `--emulator`)");
    }
    let emu = freekerf_service::emulator::serve_pty(timed).map_err(|e| anyhow!(e))?;
    {
        let mut out = std::io::stdout().lock();
        writeln!(out, "{}", emu.path())?;
        out.flush()?;
    }
    let _ = writeln!(
        std::io::stderr(),
        "Grbl 1.1 emulator on {} (close stdin or press Ctrl-C to stop)",
        emu.path()
    );
    // runs until stdin closes
    let mut sink = String::new();
    while std::io::stdin().lock().read_line(&mut sink)? > 0 {
        sink.clear();
    }
    emu.stop();
    Ok(ExitCode::SUCCESS)
}

#[cfg(not(unix))]
fn cmd_emulate(_pty: bool, _timed: bool) -> Result<ExitCode> {
    bail!("`emulate --pty` needs a Unix system; use `--emulator` with send/console")
}

fn cmd_migrate(home: Option<&std::path::Path>, from: MigrateFrom) -> Result<ExitCode> {
    let macros = freekerf_service::migrate::macros_dir(home).map_err(|e| anyhow!(e))?;
    let (dir, apply, json) = match from {
        MigrateFrom::Lasergrbl { dir, apply, json } => (dir, apply, json),
        MigrateFrom::LasergrblButtons { file, apply } => {
            let names = freekerf_service::migrate::lasergrbl_buttons(&file, &macros, apply)
                .map_err(|e| anyhow!(e))?;
            let mut out = std::io::stdout().lock();
            writeln!(out, "buttons: {}", names.join(", "))?;
            if apply {
                writeln!(out, "scripts written to {}", macros.display())?;
            } else {
                writeln!(out, "nothing written (use --apply)")?;
            }
            return Ok(ExitCode::SUCCESS);
        }
    };
    let dir = dir
        .or_else(freekerf_service::migrate::default_lasergrbl_dir)
        .ok_or_else(|| anyhow!("give LaserGRBL's data folder (no default on this system)"))?;
    if !dir.is_dir() {
        bail!("{}: not a folder", dir.display());
    }
    let stores = Stores::files(home).map_err(|e| anyhow!(e))?;
    let r = freekerf_service::migrate::lasergrbl(&dir, &stores, &macros, apply).map_err(|e| anyhow!(e))?;
    let mut out = std::io::stdout().lock();
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&r)?)?;
        return Ok(ExitCode::SUCCESS);
    }
    writeln!(out, "LaserGRBL data: {}", r.source)?;
    writeln!(out, "settings taken: {}", r.settings_applied.len())?;
    for (lg, fk) in &r.settings_applied {
        writeln!(out, "  {lg} -> {fk}")?;
    }
    writeln!(out, "settings not taken: {}", r.settings_skipped.len())?;
    writeln!(out, "laser counters: {}", r.laser_counters.join(", "))?;
    if !r.buttons.is_empty() {
        writeln!(
            out,
            "custom buttons (as scripts in {}): {}",
            macros.display(),
            r.buttons.join(", ")
        )?;
    }
    if r.user_materials_found {
        writeln!(
            out,
            "UserMaterials.psh found (imported with the material library, later)"
        )?;
    }
    for (file, e) in &r.errors {
        writeln!(out, "cannot read {file}: {e}")?;
    }
    writeln!(
        out,
        "{}",
        if r.applied {
            "written"
        } else {
            "nothing written (use --apply)"
        }
    )?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_config(home: Option<&std::path::Path>, op: ConfigOp) -> Result<ExitCode> {
    let path = Stores::settings_path(home).ok_or_else(|| anyhow!("no settings directory"))?;
    let mut out = std::io::stdout().lock();
    let load = || -> Result<freekerf_settings::Settings> {
        match std::fs::read_to_string(&path) {
            Ok(t) => Ok(freekerf_settings::Settings::from_toml(&t)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(freekerf_settings::Settings::default()),
            Err(e) => Err(e.into()),
        }
    };
    match op {
        ConfigOp::Path => writeln!(out, "{}", path.display())?,
        ConfigOp::Schema => write!(out, "{}", freekerf_settings::json_schema())?,
        ConfigOp::Get { key } => {
            let s = load()?;
            match key {
                Some(k) => writeln!(out, "{}", s.get(&k)?)?,
                None => write!(out, "{}", s.to_toml()?)?,
            }
        }
        ConfigOp::Set { key, value } => {
            let mut s = load()?;
            let v: Value = serde_json::from_str(&value).unwrap_or(Value::String(value));
            // the user may change every key, automation included
            s.set(&key, v, true)?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("toml.tmp");
            std::fs::write(&tmp, s.to_toml()?)?;
            std::fs::rename(&tmp, &path)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
