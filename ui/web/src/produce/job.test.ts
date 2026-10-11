import { describe, expect, it } from 'vitest';

import type { HostSnapshot, ResumeSuggestion } from '../gen/api';
import { causeId, LINES_BEFORE, progress, resumeOptions } from './job';

const snap = (p: Partial<HostSnapshot>) =>
  ({
    in_program: true,
    job_target: 200,
    job_executed: 50,
    job_time_ms: 30_000,
    projected_ms: 120_000,
    ...p,
  }) as HostSnapshot;

describe('job helpers', () => {
  it('progress and remaining time', () => {
    expect(progress(snap({}))).toEqual({
      pct: 25,
      executed: 50,
      target: 200,
      elapsedMs: 30_000,
      remainingMs: 90_000,
    });
    expect(progress(snap({ in_program: false }))).toBeNull();
    expect(progress(snap({ job_target: 0 }))).toBeNull();
    expect(progress(undefined)).toBeNull();
    expect(progress(snap({ job_executed: 300, projected_ms: 10 }))).toMatchObject({
      pct: 100,
      remainingMs: 0,
    });
  });

  it('recovery options follow LaserGRBL', () => {
    const s: ResumeSuggestion = {
      executed: 100,
      sent: 130,
      target: 500,
      issue: 'MachineAlarm',
      homing: true,
      set_wco: false,
    };
    const alarm = resumeOptions(s);
    expect(alarm.map((o) => [o.from, o.position, o.recommended])).toEqual([
      ['start', 0, false],
      ['before', 100 - LINES_BEFORE, true],
      ['sent', 130, false],
      ['line', 100, false],
    ]);
    const lost = resumeOptions({ ...s, issue: 'UnexpectedDisconnect' });
    expect(lost.find((o) => o.recommended)?.from).toBe('sent');
    expect(resumeOptions({ ...s, executed: 3 })[1]!.position).toBe(0);
    expect(resumeOptions({ ...s, sent: 600 })[2]!.position).toBe(499);
  });

  it('causes', () => {
    expect(causeId('MachineAlarm')).toBe('cause-alarm');
    expect(causeId('StopResponding')).toBe('cause-disc');
    expect(causeId('ManualReset')).toBe('cause-estop');
    expect(causeId('Unknown')).toBe('cause-other');
  });
});
