//! The external `autotrace` program behind centerline vectorization (ADR 0010):
//! an implementation of `freekerf_raster::centerline::Tracer` that writes the PNG
//! to a temporary file, runs the program without a shell and returns the SVG it
//! prints. The conversion itself is the planner's (`freekerf_planner::image`).

use std::path::{Path, PathBuf};

use freekerf_raster::centerline::{INPUT_PLACEHOLDER, Tracer};

/// The real `autotrace` executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAutotrace {
    /// The program (`autotrace` on the `PATH` by default).
    pub program: PathBuf,
    /// Where the temporary PNG goes.
    pub temp_dir: PathBuf,
}

impl Default for ExternalAutotrace {
    fn default() -> Self {
        Self {
            program: PathBuf::from(if cfg!(windows) {
                "autotrace.exe"
            } else {
                "autotrace"
            }),
            temp_dir: std::env::temp_dir(),
        }
    }
}

impl Tracer for ExternalAutotrace {
    fn trace(&self, png: &[u8], args: &[String]) -> Result<String, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let file = self
            .temp_dir
            .join(format!("freekerf-centerline-{}-{nanos}.png", std::process::id()));
        freekerf_platform::write_atomic(&file, png).map_err(|e| e.to_string())?;
        let name = file.display().to_string();
        let args: Vec<String> = args
            .iter()
            .map(|a| {
                if a == INPUT_PLACEHOLDER {
                    name.clone()
                } else {
                    a.clone()
                }
            })
            .collect();
        let result = freekerf_platform::run_capture(Path::new(&self.program), &args);
        let _ = freekerf_platform::remove_file(&file);
        Ok(String::from_utf8_lossy(&result.map_err(|e| e.to_string())?).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn external_program() {
        let dir = tempfile::tempdir().unwrap();
        // a stand-in that prints the arguments it was given
        let fake = ExternalAutotrace {
            program: PathBuf::from("echo"),
            temp_dir: dir.path().to_path_buf(),
        };
        let out = fake
            .trace(b"png", &["-x".into(), INPUT_PLACEHOLDER.into()])
            .unwrap();
        assert!(out.starts_with("-x ") && out.contains("freekerf-centerline-"));
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "temporary file removed"
        );
        let missing = ExternalAutotrace {
            program: PathBuf::from("/no/such/autotrace"),
            temp_dir: dir.path().to_path_buf(),
        };
        assert!(
            missing
                .trace(b"png", &[])
                .unwrap_err()
                .contains("/no/such/autotrace")
        );
        let unwritable = ExternalAutotrace {
            program: PathBuf::from("echo"),
            temp_dir: PathBuf::from("/proc/no-such-dir"),
        };
        assert!(unwritable.trace(b"png", &[]).is_err());
        assert!(
            ExternalAutotrace::default()
                .program
                .to_string_lossy()
                .contains("autotrace")
        );
    }
}
