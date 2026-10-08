// The console rows (sent commands, answers, messages), as the UI reads them:
// `console.get` once, then the host's Console events.

import type { ConsoleRow, EventMessage } from '../gen/api';
import type { AppService } from '../service';

/** Rows kept in the UI (the app keeps more). */
export const CONSOLE_UI_ROWS = 400;

export class ConsoleProjection {
  #rows: ConsoleRow[] = $state.raw([]);

  constructor(private readonly service: AppService) {
    service.subscribe((e: EventMessage) => {
      if (e.event !== 'host') return;
      if (e.data === 'ConsoleCleared') this.#rows = [];
      else if (typeof e.data === 'object' && 'Console' in e.data) {
        this.#rows = [...this.#rows, e.data.Console].slice(-CONSOLE_UI_ROWS);
      }
    });
  }

  get rows(): ConsoleRow[] {
    return this.#rows;
  }

  async refresh(): Promise<void> {
    this.#rows = await this.service.dispatch('console.get', { last: CONSOLE_UI_ROWS });
  }
}

/** A row as the console shows it: kind (prefix and colour) and text. */
export function consoleLine(row: ConsoleRow): { kind: 'tx' | 'rx' | 'err' | 'sys'; text: string } {
  if ('Command' in row) return { kind: 'tx', text: row.Command.text };
  if ('Result' in row) {
    const r = row.Result;
    const bad = r.status === 'ResponseBad' || r.status === 'InvalidResponse';
    return { kind: bad ? 'err' : 'rx', text: r.decoded ? `${r.result} (${r.decoded})` : r.result };
  }
  const m = row.Message;
  const text = m.decoded ?? m.text;
  if (m.kind === 'Alarm') return { kind: 'err', text };
  if (m.kind === 'Warning' || m.kind === 'Diagnostic') return { kind: 'sys', text };
  return { kind: 'rx', text };
}
