// Pure helpers of the Produce mode: job progress, remaining time and the recovery
// options of an interrupted job (LaserGRBL's ResumeJobForm, design/ux.md flow 7).

import type { HostSnapshot, Issue, ResumeSuggestion } from '../gen/api';

/** Lines back from the last executed one when the board's buffer was lost. */
export const LINES_BEFORE = 17;

export interface Progress {
  pct: number;
  executed: number;
  target: number;
  elapsedMs: number;
  remainingMs: number;
}

/** Progress of the running job, or `null` when none runs. */
export function progress(m: HostSnapshot | undefined): Progress | null {
  if (!m || !m.in_program || m.job_target <= 0) return null;
  const executed = Math.min(m.job_executed, m.job_target);
  return {
    pct: Math.floor((100 * executed) / m.job_target),
    executed,
    target: m.job_target,
    elapsedMs: m.job_time_ms,
    remainingMs: Math.max(0, m.projected_ms - m.job_time_ms),
  };
}

/** Where to go on from. */
export type ResumeFrom = 'start' | 'before' | 'sent' | 'line';

export interface ResumeOption {
  from: ResumeFrom;
  /** 0-based command index to resume at. */
  position: number;
  recommended: boolean;
}

/** Interruptions where what was sent may already have run (keep going from the last sent line). */
const LINK_LOST: Issue[] = ['UnexpectedDisconnect', 'ManualDisconnect', 'StopResponding'];

/** The options of the recovery dialog for a suggestion. */
export function resumeOptions(s: ResumeSuggestion): ResumeOption[] {
  const disconnected = LINK_LOST.includes(s.issue);
  return [
    { from: 'start', position: 0, recommended: false },
    { from: 'before', position: Math.max(0, s.executed - LINES_BEFORE), recommended: !disconnected },
    { from: 'sent', position: Math.min(s.sent, Math.max(0, s.target - 1)), recommended: disconnected },
    { from: 'line', position: s.executed, recommended: false },
  ];
}

/** i18n id of the cause of an interruption. */
export function causeId(issue: Issue): string {
  if (issue === 'MachineAlarm') return 'cause-alarm';
  if (LINK_LOST.includes(issue)) return 'cause-disc';
  if (issue === 'ManualReset') return 'cause-estop';
  return 'cause-other';
}

/** Steps of the jog step selector, mm. */
export const JOG_STEPS = [0.1, 1, 10, 50] as const;
