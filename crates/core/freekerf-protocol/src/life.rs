//! Laser module life counters (`LaserLifeHandler`): run time and power-weighted
//! emission time of each laser module, measured from status reports.
//!
//! The counters are data; storing them is the job of the application (the service
//! saves them with the settings). Dates are set by the application too, since the
//! core has no wall clock. Nothing is uploaded (no telemetry: DIV-027).

use serde::{Deserialize, Serialize};

use freekerf_gcode::Decimal;
use freekerf_gcode::number::f64_from_decimal;

use crate::status::MachineStatus;

/// Deltas of 600 s or more between two samples are ignored (safety check).
const MAX_DELTA_MS: u64 = 600_000;

/// Usage of one laser module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaserLifeCounter {
    /// Unique id.
    pub id: String,
    /// Name given by the user (`Default` for the first one).
    pub name: String,
    /// Brand.
    pub brand: String,
    /// Model.
    pub model: String,
    /// Optical power in watts.
    pub optical_power_w: Option<f64>,
    /// Purchase date (`YYYY-MM-DD`).
    pub purchase_date: Option<String>,
    /// Date monitoring started.
    pub monitoring_date: Option<String>,
    /// Date the module died (no longer selected automatically).
    pub death_date: Option<String>,
    /// Last day it was used.
    pub last_usage: Option<String>,
    /// Time in `Run`, ms.
    pub run_ms: u64,
    /// Emission time weighted by power, ms.
    pub normalized_power_ms: u64,
    /// Emission time with power above 3 %, ms.
    pub non_zero_ms: u64,
    /// Emission time per 10 % power class (1-10 %, ..., 91-100 %), ms.
    pub classes_ms: [u64; 10],
}

impl LaserLifeCounter {
    /// A new counter.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            brand: "Unknown".into(),
            model: "Unknown".into(),
            optical_power_w: None,
            purchase_date: None,
            monitoring_date: None,
            death_date: None,
            last_usage: None,
            run_ms: 0,
            normalized_power_ms: 0,
            non_zero_ms: 0,
            classes_ms: [0; 10],
        }
    }

    /// Time at 91-100 % power.
    #[must_use]
    pub const fn stress_ms(&self) -> u64 {
        self.classes_ms[9]
    }

    /// Average power factor while emitting (0..1).
    #[must_use]
    pub fn average_power_factor(&self) -> f64 {
        if self.non_zero_ms == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let f = self.normalized_power_ms as f64 / self.non_zero_ms as f64;
            f
        }
    }

    /// True after more than one hour of run time.
    #[must_use]
    pub const fn has_worked(&self) -> bool {
        self.run_ms > 3_600_000
    }

    fn add_power_time(&mut self, elapsed_ms: f64, fraction: f64) {
        if fraction > 0.03 {
            self.normalized_power_ms += round_ms(elapsed_ms * fraction);
            self.non_zero_ms += round_ms(elapsed_ms);
        }
        if fraction > 0.01 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=9
            let class = ((fraction * 100.0 - 1.0) / 10.0).floor().clamp(0.0, 9.0) as usize;
            self.classes_ms[class] += round_ms(elapsed_ms);
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // non-negative, small
fn round_ms(v: f64) -> u64 {
    (v + 0.5).max(0.0) as u64
}

/// The counters and the one of the connected module.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LaserLife {
    /// All modules.
    pub counters: Vec<LaserLifeCounter>,
    /// Index of the module in use, if known.
    #[serde(skip)]
    pub current: Option<usize>,
    #[serde(skip)]
    last_status_ms: Option<u64>,
    #[serde(skip)]
    last_power_ms: Option<u64>,
}

/// What [`LaserLife::on_connect`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// A counter was selected automatically.
    Selected(usize),
    /// Several alive modules: the user must choose ([`LaserLife::select`]).
    AskUser,
    /// No counters: nothing is tracked.
    None,
}

impl LaserLife {
    /// Counters loaded from storage (none selected yet).
    #[must_use]
    pub fn with_counters(counters: Vec<LaserLifeCounter>) -> Self {
        Self {
            counters,
            ..Self::default()
        }
    }

    /// Picks the counter on connect: the only one, or the only alive one. Without
    /// counters nothing is tracked (FINDINGS F-07 fixed: DIV-007).
    pub fn on_connect(&mut self) -> Selection {
        let alive: Vec<usize> = self
            .counters
            .iter()
            .enumerate()
            .filter(|(_, c)| c.death_date.is_none())
            .map(|(i, _)| i)
            .collect();
        let sel = match (self.counters.len(), alive.as_slice()) {
            (0, _) => Selection::None,
            (1, _) => Selection::Selected(0),
            (_, [only]) => Selection::Selected(*only),
            _ => Selection::AskUser,
        };
        self.current = match sel {
            Selection::Selected(i) => Some(i),
            _ => None,
        };
        sel
    }

    /// Selects a counter by id; false when not found.
    pub fn select(&mut self, id: &str) -> bool {
        self.current = self.counters.iter().position(|c| c.id == id);
        self.current.is_some()
    }

    /// No module in use.
    pub fn on_disconnect(&mut self) {
        self.current = None;
    }

    /// The counter in use.
    #[must_use]
    pub fn current(&self) -> Option<&LaserLifeCounter> {
        self.current.and_then(|i| self.counters.get(i))
    }

    /// A status sample: time spent since the previous sample counts as run time when
    /// the previous state was `Run`. The delta is measured from the previous *status*
    /// sample (FINDINGS F-26 fixed: DIV-026).
    pub fn on_status(&mut self, previous: MachineStatus, now: u64) {
        if let Some(last) = self.last_status_ms {
            let delta = now.saturating_sub(last);
            if delta > 0
                && delta < MAX_DELTA_MS
                && previous == MachineStatus::Run
                && let Some(c) = self.current.and_then(|i| self.counters.get_mut(i))
            {
                c.run_ms += delta;
            }
        }
        self.last_status_ms = Some(now);
    }

    /// A power sample: time since the previous one is weighted by the previous power
    /// (`power` as reported by `FS:`) over `max_pwm` (`$30`, ignored outside 10..=100000).
    pub fn on_power(&mut self, previous_power: f32, max_pwm: Decimal, now: u64) {
        if let Some(last) = self.last_power_ms {
            let delta = now.saturating_sub(last);
            let max = f64_from_decimal(max_pwm);
            if delta > 0 && delta < MAX_DELTA_MS && (10.0..=100_000.0).contains(&max) {
                let fraction = (f64::from(previous_power) / max).clamp(0.0, 1.0);
                if let Some(c) = self.current.and_then(|i| self.counters.get_mut(i)) {
                    #[allow(clippy::cast_precision_loss)]
                    c.add_power_time(delta as f64, fraction);
                }
            }
        }
        self.last_power_ms = Some(now);
    }

    /// Removes a counter unless it is the one in use; returns an error message otherwise.
    pub fn delete(&mut self, id: &str) -> Result<(), &'static str> {
        if self.current().is_some_and(|c| c.id == id) {
            return Err("You cannot delete currently connected Laser module!");
        }
        let current_id = self.current().map(|c| c.id.clone());
        self.counters.retain(|c| c.id != id);
        self.current = current_id.and_then(|cid| self.counters.iter().position(|c| c.id == cid));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn life(n: usize) -> LaserLife {
        LaserLife {
            counters: (0..n)
                .map(|i| LaserLifeCounter::new(format!("id{i}"), format!("L{i}")))
                .collect(),
            ..LaserLife::default()
        }
    }

    #[test]
    fn selection_on_connect() {
        assert_eq!(life(0).on_connect(), Selection::None);
        assert_eq!(life(1).on_connect(), Selection::Selected(0));
        let mut l = life(2);
        assert_eq!(l.on_connect(), Selection::AskUser);
        l.counters[0].death_date = Some("2026-01-01".into());
        assert_eq!(l.on_connect(), Selection::Selected(1));
        assert!(l.select("id0"));
        assert_eq!(l.current().unwrap().name, "L0");
        assert!(!l.select("nope"));
        l.on_disconnect();
        assert!(l.current().is_none());
    }

    #[test]
    fn run_time_only_in_run_and_plausible_deltas() {
        let mut l = life(1);
        l.on_connect();
        l.on_status(MachineStatus::Idle, 1000);
        l.on_status(MachineStatus::Run, 1500);
        l.on_status(MachineStatus::Idle, 2000);
        assert_eq!(l.current().unwrap().run_ms, 500);
        l.on_status(MachineStatus::Run, 2000 + MAX_DELTA_MS);
        assert_eq!(l.current().unwrap().run_ms, 500);
        assert!(!l.current().unwrap().has_worked());
    }

    #[test]
    fn power_classes() {
        let mut l = life(1);
        l.on_connect();
        let max = Decimal::from(1000);
        // each call passes the power of the interval that just ended
        l.on_power(0.0, max, 0);
        l.on_power(0.0, max, 100); // 100 ms at 0: nothing
        l.on_power(1000.0, max, 1100); // 1000 ms at full power
        l.on_power(500.0, max, 2100); // 1000 ms at 50 %
        l.on_power(20.0, max, 3100); // 1000 ms at 2 %: class only
        let c = l.current().unwrap();
        assert_eq!(c.classes_ms[9], 1000);
        assert_eq!(c.classes_ms[4], 1000);
        assert_eq!(c.classes_ms[0], 1000);
        assert_eq!(c.non_zero_ms, 2000);
        assert_eq!(c.normalized_power_ms, 1500);
        assert_eq!(c.stress_ms(), 1000);
        assert!((c.average_power_factor() - 0.75).abs() < 1e-9);
        // invalid $30: ignored
        let before = c.clone();
        l.on_power(10.0, Decimal::from(5), 4000);
        l.on_power(10.0, Decimal::from(5), 5000);
        assert_eq!(l.current().unwrap(), &before);
        assert_eq!(LaserLifeCounter::new("a", "b").average_power_factor(), 0.0);
    }

    #[test]
    fn delete_rules() {
        let mut l = life(3);
        l.select("id2");
        assert!(l.delete("id2").is_err());
        assert!(l.delete("id0").is_ok());
        assert_eq!(l.current().unwrap().id, "id2");
        assert_eq!(l.counters.len(), 2);
    }
}
