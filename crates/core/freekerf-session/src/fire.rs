//! The fire confirmation (PLAN §9.5, `design/ux.md` "Segurança" 3): an action of
//! tier `laser` never runs when it is asked for. It becomes a *pending request*
//! that a person confirms on screen after a countdown; only then does it run.
//!
//! This module is the pure gate (time is a parameter). The service registers the
//! request and cancel actions; confirming is **not** an action of the registry, so
//! scripts and agents can ask but never confirm: only a front end with a person in
//! front of it (the desktop window) calls the confirm method.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Default countdown before the confirm button works (LaserGRBL had 5 s).
pub const DEFAULT_COUNTDOWN_MS: u64 = 3000;
/// Shortest countdown the settings may ask for.
pub const MIN_COUNTDOWN_MS: u64 = 1000;
/// Longest countdown the settings may ask for.
pub const MAX_COUNTDOWN_MS: u64 = 10_000;

/// What the dialog shows about the job.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FireSummary {
    /// Program name, if a program is involved.
    pub program: Option<String>,
    /// Commands of the program.
    pub lines: Option<usize>,
    /// Estimated time, ms.
    pub estimated_ms: Option<i64>,
    /// `[xmin, ymin, xmax, ymax]` where the laser burns, mm.
    pub bounds: Option<[f64; 4]>,
    /// For single lines and custom code: the text that will be sent.
    pub code: Option<String>,
}

/// A laser action waiting for the person's confirmation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FireRequest {
    /// Id (increasing).
    pub id: u64,
    /// The action (`job.run`, `job.resume`, `machine.send`, `machine.custom_code`).
    pub action: String,
    /// Its parameters.
    pub params: Value,
    /// Who asked (`desktop`, an agent's name...).
    pub requested_by: String,
    /// What it will do.
    pub summary: FireSummary,
    /// Countdown before it can be confirmed, ms.
    pub countdown_ms: u64,
    /// When it was asked (clock of the gate, ms).
    #[serde(skip)]
    pub requested_at_ms: u64,
}

/// Why a confirmation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FireError {
    /// Nothing pending, or another request than the one shown.
    #[error("no pending laser request with this id")]
    NoSuchRequest,
    /// The countdown is still running.
    #[error("wait {remaining_ms} ms more before confirming")]
    TooEarly {
        /// Time left, ms.
        remaining_ms: u64,
    },
}

/// The gate: at most one pending request (a new one replaces the old one).
#[derive(Debug, Clone, Default)]
pub struct FireGate {
    pending: Option<FireRequest>,
    next_id: u64,
}

impl FireGate {
    /// Registers a request; returns it (its id is new).
    pub fn request(
        &mut self,
        action: &str,
        params: Value,
        requested_by: &str,
        summary: FireSummary,
        countdown_ms: u64,
        now_ms: u64,
    ) -> FireRequest {
        self.next_id += 1;
        let r = FireRequest {
            id: self.next_id,
            action: action.to_owned(),
            params,
            requested_by: requested_by.to_owned(),
            summary,
            countdown_ms: countdown_ms.clamp(MIN_COUNTDOWN_MS, MAX_COUNTDOWN_MS),
            requested_at_ms: now_ms,
        };
        self.pending = Some(r.clone());
        r
    }

    /// The pending request.
    #[must_use]
    pub fn pending(&self) -> Option<&FireRequest> {
        self.pending.as_ref()
    }

    /// Drops the pending request (`id`: only if it is that one). Returns true when
    /// something was dropped.
    pub fn cancel(&mut self, id: Option<u64>) -> bool {
        match (&self.pending, id) {
            (Some(p), Some(id)) if p.id != id => false,
            (Some(_), _) => {
                self.pending = None;
                true
            }
            (None, _) => false,
        }
    }

    /// Takes the request `id` for running if its countdown is over.
    ///
    /// # Errors
    /// See [`FireError`]; the request stays pending when it is too early.
    pub fn confirm(&mut self, id: u64, now_ms: u64) -> Result<FireRequest, FireError> {
        let p = self.pending.as_ref().ok_or(FireError::NoSuchRequest)?;
        if p.id != id {
            return Err(FireError::NoSuchRequest);
        }
        let elapsed = now_ms.saturating_sub(p.requested_at_ms);
        if elapsed < p.countdown_ms {
            return Err(FireError::TooEarly {
                remaining_ms: p.countdown_ms - elapsed,
            });
        }
        self.pending.take().ok_or(FireError::NoSuchRequest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate_with_request(now: u64) -> (FireGate, u64) {
        let mut g = FireGate::default();
        let r = g.request(
            "job.run",
            serde_json::json!({}),
            "desktop",
            FireSummary::default(),
            DEFAULT_COUNTDOWN_MS,
            now,
        );
        (g, r.id)
    }

    #[test]
    fn confirms_only_after_the_countdown() {
        let (mut g, id) = gate_with_request(1000);
        assert_eq!(
            g.confirm(id, 1000),
            Err(FireError::TooEarly { remaining_ms: 3000 })
        );
        assert_eq!(g.confirm(id, 3999), Err(FireError::TooEarly { remaining_ms: 1 }));
        assert!(g.pending().is_some());
        let r = g.confirm(id, 4000).unwrap();
        assert_eq!(r.action, "job.run");
        assert!(g.pending().is_none());
        // a request is confirmed once
        assert_eq!(g.confirm(id, 9000), Err(FireError::NoSuchRequest));
    }

    #[test]
    fn only_the_shown_request_can_be_confirmed() {
        let (mut g, old) = gate_with_request(0);
        let new = g
            .request(
                "machine.send",
                serde_json::json!({"line": "M3"}),
                "agent",
                FireSummary::default(),
                0,
                0,
            )
            .id;
        assert_ne!(old, new);
        assert_eq!(g.confirm(old, 10_000), Err(FireError::NoSuchRequest));
        // the countdown is never shorter than the minimum
        assert_eq!(g.pending().unwrap().countdown_ms, MIN_COUNTDOWN_MS);
        assert!(g.confirm(new, MIN_COUNTDOWN_MS).is_ok());
    }

    #[test]
    fn cancel() {
        let (mut g, id) = gate_with_request(0);
        assert!(!g.cancel(Some(id + 1)));
        assert!(g.cancel(Some(id)));
        assert!(!g.cancel(None));
        let (mut g, _) = gate_with_request(0);
        assert!(g.cancel(None));
        assert_eq!(g.confirm(id, 99_999), Err(FireError::NoSuchRequest));
        let big = FireGate::default().request("a.b", Value::Null, "x", FireSummary::default(), 60_000, 0);
        assert_eq!(big.countdown_ms, MAX_COUNTDOWN_MS);
    }
}
