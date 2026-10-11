import { describe, expect, it } from 'vitest';

import { FakeService } from '../dev/fake-service';
import { ConsoleProjection, consoleLine } from './console.svelte';

describe('console', () => {
  it('formats rows', () => {
    expect(consoleLine({ Command: { id: 1, text: 'G0 X1' } })).toEqual({ kind: 'tx', text: 'G0 X1' });
    expect(consoleLine({ Result: { id: 1, result: 'OK', status: 'ResponseGood' } })).toEqual({
      kind: 'rx',
      text: 'OK',
    });
    expect(
      consoleLine({ Result: { id: 1, result: 'ERROR:22', status: 'ResponseBad', decoded: 'Feed rate' } }),
    ).toEqual({
      kind: 'err',
      text: 'ERROR:22 (Feed rate)',
    });
    expect(consoleLine({ Message: { kind: 'Alarm', text: 'ALARM:1', decoded: 'Hard limit' } })).toEqual({
      kind: 'err',
      text: 'Hard limit',
    });
    expect(consoleLine({ Message: { kind: 'Warning', text: 'w' } }).kind).toBe('sys');
    expect(consoleLine({ Message: { kind: 'Feedback', text: '[MSG]' } }).kind).toBe('rx');
  });

  it('follows the events', async () => {
    const s = new FakeService();
    const c = new ConsoleProjection(s);
    await c.refresh();
    s.emit({ event: 'host', data: { Console: { Command: { id: 1, text: '$$' } } } });
    expect(c.rows).toHaveLength(1);
    s.emit({ event: 'host', data: 'ConsoleCleared' });
    expect(c.rows).toHaveLength(0);
  });
});
