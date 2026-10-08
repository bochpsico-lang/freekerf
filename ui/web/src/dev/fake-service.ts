// A fake implementation of the app contract, for development in a browser and for
// component tests. It must never reach a production build: nothing outside
// `src/dev/` and the tests imports it (lint), and `npm run check:bundle` fails if
// the marker below is found in `dist/`.

import type { ActionName, ActionSpec, EventMessage, MachineStatus, State } from '../gen/api';
import { ACTION_TIERS } from '../gen/api';
import { ActionFailure, type AppService, type Params, type Result } from '../service';

/** Found in the bundle only if the fake service leaked into it. */
export const FAKE_SERVICE_MARKER = 'freekerf-fake-service-do-not-ship';

function initialState(): State {
  return {
    machine: {
      firmware: 'Grbl',
      status: 'Disconnected',
      connected: false,
      ready: false,
      version: null,
      in_program: false,
      machine_position: { x: 0, y: 0, z: 0 },
      work_offset: { x: 0, y: 0, z: 0 },
      work_position: { x: 0, y: 0, z: 0 },
      feed: 0,
      power: 0,
      overrides: [100, 100, 100],
      override_targets: [100, 100, 100],
      buffer_size: 127,
      used_buffer: 0,
      grbl_blocks: -1,
      grbl_buffer: -1,
      queued: 0,
      pending: 0,
      job_target: 0,
      job_sent: 0,
      job_executed: 0,
      job_errors: 0,
      job_time_ms: 0,
      loop_count: 0,
      jog_enabled: false,
      failed_connections: 0,
      detected_ip: null,
      last_issue: 'Unknown',
      program_estimated_ms: 0,
      program_lines: 0,
      projected_ms: 0,
    },
    firmware_version: null,
    issues: [],
    program: null,
    resume: null,
  };
}

type Handler = (params: unknown) => unknown;

/** In-memory service with a tiny machine model (connect, stop, jog). */
export class FakeService implements AppService {
  readonly marker = FAKE_SERVICE_MARKER;
  readonly calls: { action: ActionName; params: unknown }[] = [];
  state: State = initialState();
  private listeners = new Set<(event: EventMessage) => void>();
  private handlers: Partial<Record<ActionName, Handler>> = {
    'state.get': () => structuredClone(this.state),
    'machine.connect': () => {
      this.setStatus('Idle', true);
      this.emit({ event: 'connected', transport: 'emulator' });
      return null;
    },
    'machine.disconnect': () => {
      this.setStatus('Disconnected', false);
      this.emit({ event: 'disconnected', user: true });
      return null;
    },
    'machine.stop': () => {
      if (this.state.machine.connected) this.setStatus('Idle', true);
      return null;
    },
  };

  /** Overrides or adds the behaviour of an action (tests). */
  on<A extends ActionName>(action: A, handler: (params: Params<A>) => Result<A>): void {
    this.handlers[action] = handler as Handler;
  }

  async dispatch<A extends ActionName>(action: A, params: Params<A>): Promise<Result<A>> {
    this.calls.push({ action, params });
    const handler = this.handlers[action];
    if (!handler) {
      throw new ActionFailure(action, { kind: 'refused', message: `${action} is not simulated` });
    }
    return handler(params) as Result<A>;
  }

  async catalog(): Promise<ActionSpec[]> {
    return Object.entries(ACTION_TIERS).map(([name, tier]) => ({
      name,
      tier,
      description: `${name} (fake)`,
      params: {},
    }));
  }

  subscribe(listener: (event: EventMessage) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Sends an event to the subscribers. */
  emit(event: EventMessage): void {
    for (const l of this.listeners) l(event);
  }

  private setStatus(status: MachineStatus, connected: boolean): void {
    const old = this.state.machine.status;
    this.state.machine.status = status;
    this.state.machine.connected = connected;
    this.emit({ event: 'host', data: { StatusChanged: { old, new: status } } });
  }
}
