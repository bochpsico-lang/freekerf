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
use freekerf_service::{Caller, Service, Stores};
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
    /// Runs one action and prints its JSON result.
    Run {
        /// Action name (`state.get`).
        action: String,
        /// Parameters as JSON.
        #[arg(default_value = "{}")]
        params: String,
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
        Command::Run { action, params } => {
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

fn open_service(home: Option<&std::path::Path>) -> Result<Service> {
    let stores = Stores::files(home).map_err(|e| anyhow!(e))?;
    Ok(Service::new(stores)?)
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
