//! End-to-end tests of the `freekerf` binary.
#![allow(missing_docs)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use freekerf_testkit::fixtures::gcode_fixture;
use predicates::prelude::*;
use serde_json::{Value, json};

fn freekerf(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("freekerf").unwrap();
    c.arg("--home").arg(home);
    c
}

#[test]
fn analyze_json_matches_the_golden_files() {
    let home = tempfile::tempdir().unwrap();
    for name in ["arcs", "comments_spacing", "malformed", "square", "relative"] {
        let out = freekerf(home.path())
            .args(["analyze", "--json", "--max-rate", "6000"])
            .arg(gcode_fixture(&format!("gcode/{name}.nc")))
            .output()
            .unwrap();
        assert!(out.status.success());
        let got: Value = serde_json::from_slice(&out.stdout).unwrap();
        let golden: Value =
            serde_json::from_str(&std::fs::read_to_string(gcode_fixture(&format!("golden/{name}.json"))).unwrap()).unwrap();
        assert_eq!(got, golden, "{name}");
    }
}

#[test]
fn analyze_summary_and_errors() {
    let home = tempfile::tempdir().unwrap();
    freekerf(home.path())
        .arg("analyze")
        .arg(gcode_fixture("gcode/square.nc"))
        .assert()
        .success()
        .stdout(predicate::str::contains("square.nc: 11 commands").and(predicate::str::contains("quadrant I")));
    freekerf(home.path())
        .arg("analyze")
        .arg(gcode_fixture("gcode/malformed.nc"))
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing burns"));
    freekerf(home.path())
        .args(["analyze", "/nonexistent/file.nc"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot read"));
}

#[test]
fn send_streams_a_file_to_the_emulator() {
    let home = tempfile::tempdir().unwrap();
    freekerf(home.path())
        .args(["send", "--emulator"])
        .arg(gcode_fixture("gcode/square.nc"))
        .timeout(Duration::from_secs(60))
        .assert()
        .success()
        .stderr(predicate::str::contains("done: 11 lines, 0 errors"));
}

#[test]
fn send_without_a_port_fails() {
    let home = tempfile::tempdir().unwrap();
    freekerf(home.path())
        .arg("send")
        .arg(gcode_fixture("gcode/square.nc"))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no port"));
}

#[test]
fn config_get_set_path_schema() {
    let home = tempfile::tempdir().unwrap();
    freekerf(home.path()).args(["config", "path"]).assert().success().stdout(predicate::str::contains("settings.toml"));
    freekerf(home.path()).args(["config", "get", "machine.baud"]).assert().success().stdout("115200\n");
    freekerf(home.path()).args(["config", "set", "machine.port", "COM7"]).assert().success();
    freekerf(home.path()).args(["config", "set", "automation.mcp.enabled", "true"]).assert().success();
    freekerf(home.path())
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("port = \"COM7\"").and(predicate::str::contains("enabled = true")));
    freekerf(home.path()).args(["config", "set", "machine.nope", "1"]).assert().code(2);
    freekerf(home.path()).args(["config", "schema"]).assert().success().stdout(predicate::str::contains("\"$schema\""));
}

#[test]
fn actions_and_run() {
    let home = tempfile::tempdir().unwrap();
    freekerf(home.path()).arg("actions").assert().success().stdout(predicate::str::contains("machine.stop"));
    freekerf(home.path()).args(["actions", "--markdown"]).assert().success().stdout(predicate::str::starts_with("# Actions"));
    let out = freekerf(home.path()).args(["actions", "--json"]).output().unwrap();
    let specs: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(specs.as_array().unwrap().len() >= 25);
    freekerf(home.path())
        .args(["run", "state.get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"Disconnected\""));
    freekerf(home.path()).args(["run", "job.run"]).assert().code(2).stderr(predicate::str::contains("not connected"));
    freekerf(home.path()).args(["run", "state.get", "{"]).assert().code(2);
    freekerf(home.path()).args(["host"]).assert().code(2);
    freekerf(home.path()).args(["--log", "bogus", "run", "job.info"]).assert().success().stdout("null\n");
}

#[test]
fn console_session_on_the_emulator() {
    let home = tempfile::tempdir().unwrap();
    freekerf(home.path())
        .args(["console", "--emulator"])
        .write_stdin(":help\nG0 X1\n:state\n:jog N 2\n:run job.info\n:run bad {\n:nope\n:hold\n\n:quit\n")
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("> G0 X1").and(predicate::str::contains("\"firmware_version\": \"1.1f\"")));
}

struct Host {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl Host {
    fn start(home: &std::path::Path) -> Self {
        let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("freekerf"))
            .arg("--home")
            .arg(home)
            .args(["host", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdin, stdout, next: 1 }
    }

    fn call(&mut self, action: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(self.stdin, "{}", json!({"id": id, "action": action, "params": params})).unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], id);
        v
    }

    fn wait(&mut self, pred: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            let st = self.call("state.get", Value::Null)["result"].clone();
            if pred(&st) {
                return st;
            }
            assert!(Instant::now() < end, "timeout: {st}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(unix)]
#[test]
fn host_stdio_drives_the_emulator_on_a_pty() {
    let home = tempfile::tempdir().unwrap();
    let mut emu = StdCommand::new(assert_cmd::cargo::cargo_bin("freekerf"))
        .args(["emulate", "--pty", "--instant"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut path = String::new();
    BufReader::new(emu.stdout.take().unwrap()).read_line(&mut path).unwrap();
    let path = path.trim().to_owned();
    assert!(path.starts_with("/dev/"));

    let mut h = Host::start(home.path());
    assert_eq!(h.call("machine.connect", json!({"port": path}))["ok"], true);
    h.wait(|st| st["machine"]["ready"] == true && st["machine"]["status"] == "Idle");
    let job = home.path().join("job.nc");
    std::fs::write(&job, "G0 X1\nM3 S100\nG1 X2 F600\nM5\n").unwrap();
    assert_eq!(h.call("job.load", json!({"path": job.to_string_lossy()}))["result"]["lines"], 4);
    assert_eq!(h.call("job.run", Value::Null)["ok"], true);
    let st = h.wait(|st| st["machine"]["in_program"] == false && st["machine"]["status"] == "Idle");
    assert_eq!(st["firmware_version"], "1.1f");
    let bad = h.call("nope.nope", Value::Null);
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["kind"], "unknown");
    drop(h.stdin);
    assert!(h.child.wait().unwrap().success());
    drop(emu.stdin.take());
    assert!(emu.wait().unwrap().success());
}
