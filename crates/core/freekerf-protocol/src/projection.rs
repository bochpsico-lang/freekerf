//! Job progress and time projection (`TimeProjection`).
//!
//! Times are milliseconds of the injected monotonic clock.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::status::Point3;

/// Why a job stopped before its end (`DetectedIssue`). Negative codes are caused by
/// the user and are not reported as problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Issue {
    /// No issue.
    Unknown,
    /// The user reset the board.
    ManualReset,
    /// The user disconnected.
    ManualDisconnect,
    /// The user aborted the job.
    ManualAbort,
    /// No status report for too long during a job.
    StopResponding,
    /// The board restarted (welcome banner) during a job.
    UnexpectedReset,
    /// The connection dropped during a job.
    UnexpectedDisconnect,
    /// `ALARM:` during a job.
    MachineAlarm,
}

impl Issue {
    /// True for issues the user caused (not reported as problems).
    #[must_use]
    pub const fn is_user_action(self) -> bool {
        matches!(
            self,
            Self::ManualReset | Self::ManualDisconnect | Self::ManualAbort
        )
    }

    /// Name as in LaserGRBL (`MachineAlarm`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::ManualReset => "ManualReset",
            Self::ManualDisconnect => "ManualDisconnect",
            Self::ManualAbort => "ManualAbort",
            Self::StopResponding => "StopResponding",
            Self::UnexpectedReset => "UnexpectedReset",
            Self::UnexpectedDisconnect => "UnexpectedDisconnect",
            Self::MachineAlarm => "MachineAlarm",
        }
    }
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Counters and timers of the running job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeProjection {
    estimated_target_ms: i64,
    estimated_progress_ms: i64,
    start: u64,
    end: u64,
    global_start: u64,
    global_end: u64,
    pause_begin: u64,
    cumulated_pause: u64,
    in_pause: bool,
    completed: bool,
    started: bool,
    target: usize,
    executed: usize,
    sent: usize,
    errors: usize,
    continue_correction: usize,
    last_issue: Issue,
    last_known_wco: Point3,
}

impl Default for TimeProjection {
    fn default() -> Self {
        Self {
            estimated_target_ms: 0,
            estimated_progress_ms: 0,
            start: 0,
            end: 0,
            global_start: 0,
            global_end: 0,
            pause_begin: 0,
            cumulated_pause: 0,
            in_pause: false,
            completed: false,
            started: false,
            target: 0,
            executed: 0,
            sent: 0,
            errors: 0,
            continue_correction: 0,
            last_issue: Issue::Unknown,
            last_known_wco: Point3::ZERO,
        }
    }
}

impl TimeProjection {
    /// Clears everything; `global` also clears the multi-pass start/end.
    pub fn reset(&mut self, global: bool) {
        let (gs, ge) = (self.global_start, self.global_end);
        *self = Self::default();
        if !global {
            self.global_start = gs;
            self.global_end = ge;
        }
    }

    /// A job (or a pass) starts with `target` queued commands.
    pub fn job_start(&mut self, estimated_ms: i64, target: usize, global: bool, now: u64) {
        if !self.started {
            self.estimated_target_ms = estimated_ms;
            self.target = target;
            self.estimated_progress_ms = 0;
            self.start = now;
            if global {
                self.global_start = now;
            }
            self.pause_begin = 0;
            self.in_pause = false;
            self.completed = false;
            self.started = true;
            self.executed = 0;
            self.sent = 0;
            self.errors = 0;
            self.continue_correction = 0;
            self.last_issue = Issue::Unknown;
            self.last_known_wco = Point3::ZERO;
        }
    }

    /// A job resumes from `position`; `added` commands were queued to rebuild the state.
    pub fn job_continue(
        &mut self,
        estimated_ms: i64,
        file_len: usize,
        position: usize,
        added: usize,
        now: u64,
    ) {
        if !self.started {
            if self.estimated_target_ms == 0 {
                self.estimated_target_ms = estimated_ms;
            }
            if self.target == 0 {
                self.target = file_len;
            }
            if self.start == 0 {
                self.start = now;
                self.global_start = now;
            }
            self.pause_begin = 0;
            self.in_pause = false;
            self.completed = false;
            self.started = true;
            self.executed = position;
            self.sent = position;
            self.last_issue = Issue::Unknown;
            self.continue_correction = added;
        }
    }

    /// One command sent.
    pub fn job_sent(&mut self) {
        if self.in_program() {
            self.sent += 1;
        }
    }

    /// One command answered with an error.
    pub fn job_error(&mut self) {
        if self.in_program() {
            self.errors += 1;
        }
    }

    /// One command executed; `progress_ms` is its estimated time offset.
    pub fn job_executed(&mut self, progress_ms: i64) {
        if self.in_program() {
            self.executed += 1;
            self.estimated_progress_ms = progress_ms;
        }
    }

    /// The machine paused.
    pub fn job_pause(&mut self, now: u64) {
        if self.in_program() && !self.in_pause {
            self.in_pause = true;
            self.pause_begin = now;
        }
    }

    /// The machine resumed.
    pub fn job_resume(&mut self, now: u64) {
        if self.in_program() && self.in_pause {
            self.cumulated_pause += now.saturating_sub(self.pause_begin);
            self.in_pause = false;
        }
    }

    /// The job (or one pass of it) ended; true if it was running. The global end is
    /// recorded for every pass (FINDINGS F-14 fixed: DIV-014).
    pub fn job_end(&mut self, now: u64) -> bool {
        if self.in_program() {
            self.job_resume(now);
            self.end = now;
            self.global_end = now;
            self.completed = true;
            self.started = false;
            true
        } else {
            false
        }
    }

    /// Records the issue that interrupted the job.
    pub fn job_issue(&mut self, issue: Issue) {
        self.last_issue = issue;
    }

    /// Remembers the work offset while in a job (used to offer a resume).
    pub fn set_last_known_wco(&mut self, wco: Point3) {
        if self.in_program() {
            self.last_known_wco = wco;
        }
    }

    /// Last work offset seen during the job.
    #[must_use]
    pub const fn last_known_wco(&self) -> Point3 {
        self.last_known_wco
    }

    /// True between start and end.
    #[must_use]
    pub const fn in_program(&self) -> bool {
        self.started && !self.completed
    }

    /// Commands queued for the job.
    #[must_use]
    pub const fn target(&self) -> usize {
        self.target
    }

    /// Commands sent.
    #[must_use]
    pub const fn sent(&self) -> usize {
        self.sent.saturating_sub(self.continue_correction)
    }

    /// Commands executed.
    #[must_use]
    pub const fn executed(&self) -> usize {
        self.executed.saturating_sub(self.continue_correction)
    }

    /// Errors.
    #[must_use]
    pub const fn errors(&self) -> usize {
        self.errors
    }

    /// The issue that stopped the job.
    #[must_use]
    pub const fn last_issue(&self) -> Issue {
        self.last_issue
    }

    /// Estimated total of the job.
    #[must_use]
    pub const fn estimated_target_ms(&self) -> i64 {
        self.estimated_target_ms
    }

    /// Time of this pass.
    #[must_use]
    pub fn total_job_ms(&self, now: u64) -> u64 {
        if self.completed {
            self.end.saturating_sub(self.start)
        } else if self.started {
            now.saturating_sub(self.start)
        } else {
            0
        }
    }

    /// Time since the first pass started.
    #[must_use]
    pub fn total_global_ms(&self, now: u64) -> u64 {
        if self.completed {
            self.global_end.saturating_sub(self.global_start)
        } else if self.started {
            now.saturating_sub(self.global_start)
        } else {
            0
        }
    }

    fn total_pauses(&self, now: u64) -> u64 {
        if self.in_pause {
            self.cumulated_pause + now.saturating_sub(self.pause_begin)
        } else {
            self.cumulated_pause
        }
    }

    /// Projected total time of the job, from the real speed so far.
    #[must_use]
    pub fn projected_ms(&self, now: u64) -> i64 {
        if !self.started {
            return 0;
        }
        let pauses = self.total_pauses(now);
        let real = self.total_job_ms(now).saturating_sub(pauses);
        if self.estimated_progress_ms == 0 {
            return self.estimated_target_ms;
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // ms values far below 2^53
        let projected = (real as f64 * self.estimated_target_ms as f64 / self.estimated_progress_ms as f64)
            .round() as i64;
        projected.saturating_add(i64::try_from(pauses).unwrap_or(i64::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_projection() {
        let mut tp = TimeProjection::default();
        assert!(!tp.in_program());
        assert_eq!(tp.projected_ms(100), 0);
        tp.job_start(10_000, 5, true, 1000);
        assert!(tp.in_program());
        assert_eq!(tp.projected_ms(1000), 10_000);
        tp.job_sent();
        tp.job_executed(2000);
        tp.job_error();
        assert_eq!((tp.sent(), tp.executed(), tp.errors(), tp.target()), (1, 1, 1, 5));
        // 1 s real for 2 s estimated: projected 5 s
        assert_eq!(tp.projected_ms(2000), 5000);
        tp.job_pause(2000);
        tp.job_pause(2100);
        assert_eq!(tp.projected_ms(3000), 6000);
        tp.job_resume(3000);
        assert_eq!(tp.total_job_ms(3000), 2000);
        assert!(tp.job_end(4000));
        assert!(!tp.job_end(5000));
        assert_eq!(tp.total_job_ms(9999), 3000);
        assert_eq!(tp.total_global_ms(9999), 3000);
        assert_eq!(tp.estimated_target_ms(), 10_000);
    }

    #[test]
    fn passes_keep_the_global_start() {
        let mut tp = TimeProjection::default();
        tp.job_start(100, 1, true, 1000);
        assert!(tp.job_end(2000));
        // F-14 fixed: the global time is valid between passes
        assert_eq!(tp.total_global_ms(2500), 1000);
        tp.reset(false);
        tp.job_start(100, 1, false, 3000);
        assert_eq!(tp.total_global_ms(3500), 2500);
        assert_eq!(tp.total_job_ms(3500), 500);
        tp.reset(true);
        assert_eq!(tp.total_global_ms(3500), 0);
    }

    #[test]
    fn continue_from_position() {
        let mut tp = TimeProjection::default();
        tp.job_continue(5000, 100, 40, 3, 10);
        assert_eq!(tp.target(), 100);
        assert_eq!(tp.executed(), 37);
        tp.job_executed(10);
        tp.job_sent();
        assert_eq!((tp.executed(), tp.sent()), (38, 38));
        // not restarted while running
        tp.job_continue(1, 1, 1, 1, 20);
        assert_eq!(tp.target(), 100);
    }

    #[test]
    fn wco_and_issues() {
        let mut tp = TimeProjection::default();
        tp.set_last_known_wco(Point3::new(1.0, 1.0, 1.0));
        assert_eq!(tp.last_known_wco(), Point3::ZERO);
        tp.job_start(0, 0, true, 0);
        tp.set_last_known_wco(Point3::new(1.0, 1.0, 1.0));
        assert_eq!(tp.last_known_wco(), Point3::new(1.0, 1.0, 1.0));
        tp.job_issue(Issue::MachineAlarm);
        assert_eq!(tp.last_issue(), Issue::MachineAlarm);
        assert!(Issue::ManualAbort.is_user_action() && !Issue::MachineAlarm.is_user_action());
        for i in [
            Issue::Unknown,
            Issue::ManualReset,
            Issue::ManualDisconnect,
            Issue::ManualAbort,
            Issue::StopResponding,
            Issue::UnexpectedReset,
            Issue::UnexpectedDisconnect,
            Issue::MachineAlarm,
        ] {
            assert_eq!(i.to_string(), i.as_str());
        }
    }
}
