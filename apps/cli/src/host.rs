//! `freekerf host --stdio`: the action registry over JSON Lines.
//!
//! Each request is one line: `{"id": 1, "action": "machine.connect", "params": {...}}`.
//! Each response is one line: `{"id": 1, "ok": true, "result": ...}` or
//! `{"id": 1, "ok": false, "error": {"kind": "refused", "message": "..."}}`.
//! Requests are handled in order. The caller is a test harness with every tier
//! (see `doc/features/host-protocol.md`).

use std::io::{BufRead, Write};

use anyhow::Result;
use freekerf_service::{ActionError, Caller, CallerKind, Service, Tier};
use serde_json::{Value, json};

/// Serves requests from `input` until it closes, then disconnects the machine.
pub fn serve(service: &Service, input: impl BufRead, mut output: impl Write) -> Result<()> {
    let caller = Caller::new(CallerKind::Test, "host-stdio", Tier::Laser);
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(service, &caller, &req),
            Err(e) => json!({"id": Value::Null, "ok": false, "error": {"kind": "invalid_request", "message": e.to_string()}}),
        };
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    let _ = service.dispatch(&caller, "machine.disconnect", Value::Null);
    Ok(())
}

fn handle(service: &Service, caller: &Caller, req: &Value) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(action) = req.get("action").and_then(Value::as_str) else {
        return json!({"id": id, "ok": false, "error": {"kind": "invalid_request", "message": "missing `action`"}});
    };
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    match service.dispatch(caller, action, params) {
        Ok(result) => json!({"id": id, "ok": true, "result": result}),
        Err(e) => json!({"id": id, "ok": false, "error": error_json(&e)}),
    }
}

fn error_json(e: &ActionError) -> Value {
    // `kind` from the error variant, `message` always the full text
    let mut v = serde_json::to_value(e).unwrap_or_else(|_| json!({}));
    v["message"] = Value::String(e.to_string());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_errors() {
        let service = Service::in_memory().unwrap();
        let input = b"{\"id\":1,\"action\":\"job.info\"}\n\nnot json\n{\"id\":2}\n{\"id\":3,\"action\":\"job.run\"}\n";
        let mut out = Vec::new();
        serve(&service, &input[..], &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], json!({"id": 1, "ok": true, "result": null}));
        assert_eq!(lines[1]["error"]["kind"], "invalid_request");
        assert_eq!(lines[2]["error"]["message"], "missing `action`");
        assert_eq!(lines[3]["error"]["kind"], "refused");
        assert!(lines[3]["error"]["message"].as_str().unwrap().contains("not connected"));
    }
}
