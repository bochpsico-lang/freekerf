// A fake implementation of the app contract, for development in a browser and for
// component tests. It must never reach a production build: nothing outside
// `src/dev/` and the tests imports it (lint), and `npm run check:bundle` fails if
// the marker below is found in `dist/`.

import type {
  ActionName,
  ActionSpec,
  EventMessage,
  MachineStatus,
  Settings,
  State,
  ViewState,
} from '../gen/api';
import { ACTION_TIERS } from '../gen/api';
import { ActionFailure, type AppService, type Params, type Result } from '../service';
import { encodeToolpath, type SegmentRecord } from '../canvas/toolpath';

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
  private headListeners = new Set<(frame: ArrayBuffer) => void>();
  view: ViewState = {
    mode: 'produce',
    panel: 'machine',
    drawer_open: false,
    dialogs: [],
    toasts: [],
    viewport: null,
    fit_request: 0,
    focus: null,
  };
  settings: Settings = {
    version: 1,
    ui: { theme: 'dark', high_contrast: false, density: 'auto', units: 'mm' },
  };
  private nextToast = 1;
  /** Tool path returned by `toolpath()` (set by `job.load_text`, or by tests). */
  toolpathSegments: SegmentRecord[] = [];
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
    'job.load_text': (p) => {
      const { name, gcode } = p as Params<'job.load_text'>;
      // a stand-in for the Rust analysis: one 50 mm square per 10 lines
      const lines = gcode.split('\n').filter((l) => l.trim()).length;
      this.toolpathSegments = squares(Math.max(1, Math.ceil(lines / 10)));
      this.state.program = {
        name,
        lines,
        estimated_ms: lines * 100,
        drawing_range: [0, 0, 50, 50],
        quadrant: 'Q1',
      };
      this.emit({ event: 'program_loaded' });
      return this.state.program;
    },
    'view.get': () => structuredClone(this.view),
    'view.set_mode': (p) =>
      this.changeView((v) => {
        v.mode = (p as Params<'view.set_mode'>).mode;
        if (v.mode === 'produce') v.panel = 'machine';
      }),
    'view.show_panel': (p) =>
      this.changeView((v) => {
        v.panel = (p as Params<'view.show_panel'>).panel;
        v.drawer_open = true;
      }),
    'view.set_drawer': (p) =>
      this.changeView((v) => {
        v.drawer_open = (p as Params<'view.set_drawer'>).open;
      }),
    'view.open_dialog': (p) =>
      this.changeView((v) => {
        const d = (p as Params<'view.open_dialog'>).dialog;
        v.dialogs = [...v.dialogs.filter((x) => x !== d), d];
      }),
    'view.close_dialog': (p) =>
      this.changeView((v) => {
        const d = (p as Params<'view.close_dialog'>).dialog;
        v.dialogs = d ? v.dialogs.filter((x) => x !== d) : v.dialogs.slice(0, -1);
      }),
    'view.toast': (p) => {
      const t = p as Params<'view.toast'>;
      const id = this.nextToast++;
      this.changeView((v) => {
        v.toasts = [
          ...v.toasts,
          { id, level: t.level ?? 'info', message: t.message, args: t.args ?? {} },
        ].slice(-3);
      });
      return id;
    },
    'view.dismiss_toast': (p) =>
      this.changeView((v) => {
        v.toasts = v.toasts.filter((x) => x.id !== (p as Params<'view.dismiss_toast'>).id);
      }),
    'view.set_viewport': (p) =>
      this.changeView((v) => {
        v.viewport = p as Params<'view.set_viewport'>;
      }),
    'view.fit': () =>
      this.changeView((v) => {
        v.fit_request += 1;
      }),
    'view.focus': (p) =>
      this.changeView((v) => {
        v.focus = (p as Params<'view.focus'>).target ?? null;
      }),
    'settings.get': () => structuredClone(this.settings),
    'settings.set': (p) => {
      const { key, value } = p as Params<'settings.set'>;
      const parts = key.split('.');
      let slot = this.settings as Record<string, unknown>;
      for (const part of parts.slice(0, -1)) slot = (slot[part] ??= {}) as Record<string, unknown>;
      slot[parts.at(-1)!] = value;
      this.emit({ event: 'settings_changed' });
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

  async toolpath(): Promise<ArrayBuffer> {
    return encodeToolpath(this.toolpathSegments, this.toolpathSegments.length);
  }

  subscribeHead(listener: (frame: ArrayBuffer) => void): () => void {
    this.headListeners.add(listener);
    return () => this.headListeners.delete(listener);
  }

  /** Sends a head frame to the subscribers. */
  emitHead(x: number, y: number, executed: number, target: number, running: boolean): void {
    const b = new ArrayBuffer(24);
    const v = new DataView(b);
    v.setFloat32(0, x, true);
    v.setFloat32(4, y, true);
    v.setUint32(12, executed, true);
    v.setUint32(16, target, true);
    v.setUint32(20, (running ? 1 : 0) | (this.state.machine.connected ? 2 : 0), true);
    for (const l of this.headListeners) l(b);
  }

  /** Sends an event to the subscribers. */
  emit(event: EventMessage): void {
    for (const l of this.listeners) l(event);
  }

  private changeView(f: (v: ViewState) => void): null {
    const before = JSON.stringify(this.view);
    f(this.view);
    if (JSON.stringify(this.view) !== before) this.emit({ event: 'view', view: structuredClone(this.view) });
    return null;
  }

  private setStatus(status: MachineStatus, connected: boolean): void {
    const old = this.state.machine.status;
    this.state.machine.status = status;
    this.state.machine.connected = connected;
    this.emit({ event: 'host', data: { StatusChanged: { old, new: status } } });
  }
}

function squares(n: number): SegmentRecord[] {
  const out: SegmentRecord[] = [];
  for (let i = 0; i < n; i++) {
    const o = i * 60;
    const c: [number, number][] = [
      [o, 0],
      [o + 50, 0],
      [o + 50, 50],
      [o, 50],
      [o, 0],
    ];
    for (let k = 0; k < 4; k++) out.push({ from: c[k]!, to: c[k + 1]!, power: 1000, command: i * 4 + k });
  }
  return out;
}
