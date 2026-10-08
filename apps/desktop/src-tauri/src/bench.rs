//! `freekerf-desktop --bench-canvas[=<file.json>]`: runs the canvas performance check
//! (PLAN M4a: 20,000 paths + an 8000×6000 image, pan with the mouse) inside the real
//! window, prints the report as JSON (and writes it to the file when given), adds
//! the process memory, and exits with 0 when the frame rate target is met, 1 when
//! not. `--bench-query=paths=0&image=16x16` passes diagnostics options to the page
//! (see `ui/web/src/canvas/bench.ts`). Normal runs ignore both.

use std::path::PathBuf;

use serde_json::Value;

/// Frame rate the check requires (PLAN M4a acceptance).
pub const TARGET_FPS: f64 = 50.0;

/// The benchmark request, if the app was started with `--bench-canvas`.
#[derive(Debug, Clone, Default)]
pub struct Bench {
    enabled: bool,
    out: Option<PathBuf>,
    query: Option<String>,
}

impl Bench {
    /// From the command line arguments (without the program name).
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        let mut b = Self::default();
        for a in args {
            if a == "--bench-canvas" {
                b.enabled = true;
            } else if let Some(path) = a.strip_prefix("--bench-canvas=") {
                b.enabled = true;
                b.out = Some(PathBuf::from(path));
            } else if let Some(q) = a.strip_prefix("--bench-query=") {
                b.query = Some(q.to_owned());
            }
        }
        b
    }

    /// The URL fragment the window opens to run the benchmark.
    #[must_use]
    pub fn start_url(&self) -> Option<String> {
        self.enabled.then(|| match &self.query {
            Some(q) => format!("?{q}#bench-canvas"),
            None => "#bench-canvas".to_owned(),
        })
    }

    /// Adds the memory of the process, prints and saves the report; returns the
    /// exit code.
    #[must_use]
    pub fn finish(&self, report: &Value) -> i32 {
        let mut report = report.clone();
        if let (Some(obj), Some(rss)) = (report.as_object_mut(), peak_rss_kib()) {
            obj.insert("process_peak_rss_mib".into(), Value::from(rss / 1024));
        }
        let text = serde_json::to_string_pretty(&report).unwrap_or_default();
        tracing::info!("canvas benchmark: {text}");
        #[allow(clippy::print_stdout)] // the report is the output of this mode
        {
            println!("{text}");
        }
        if let Some(path) = &self.out
            && let Err(e) = std::fs::write(path, &text)
        {
            tracing::error!("cannot write {}: {e}", path.display());
        }
        let fps = report.get("pan_fps_mean").and_then(Value::as_f64).unwrap_or(0.0);
        i32::from(fps < TARGET_FPS)
    }
}

/// Peak resident memory of this process (WebKitGTK renders in child processes:
/// the report also carries the web process numbers when the page can read them).
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        assert!(Bench::from_args(Vec::<String>::new()).start_url().is_none());
        let b = Bench::from_args(["--bench-canvas=/tmp/x.json".to_owned()]);
        assert_eq!(b.start_url().as_deref(), Some("#bench-canvas"));
        assert_eq!(b.out, Some(PathBuf::from("/tmp/x.json")));
        let q = Bench::from_args(["--bench-canvas".to_owned(), "--bench-query=paths=0".to_owned()]);
        assert_eq!(q.start_url().as_deref(), Some("?paths=0#bench-canvas"));
    }

    #[test]
    fn exit_code_follows_the_target() {
        let b = Bench::default();
        assert_eq!(b.finish(&serde_json::json!({"pan_fps_mean": 60.0})), 0);
        assert_eq!(b.finish(&serde_json::json!({"pan_fps_mean": 20.0})), 1);
        assert_eq!(b.finish(&serde_json::json!({})), 1);
    }
}
