// Machine state as the UI shows it: a read-only projection of `state.get` and of
// the service events. Nothing here changes app state; actions go through
// `service.dispatch`.

import type { EventMessage, MachineStatus, State } from '../gen/api';
import type { AppService } from '../service';

/** The six visual states of `design/ux.md` ("Estados da máquina"). */
export type VisualState = 'offline' | 'connecting' | 'idle' | 'run' | 'hold' | 'alarm';

/** Groups the 14 firmware states into the 6 visual states. */
export function visualState(status: MachineStatus | undefined): VisualState {
  switch (status) {
    case undefined:
    case 'Disconnected':
      return 'offline';
    case 'Connecting':
      return 'connecting';
    case 'Idle':
    case 'Check':
    case 'Queue':
      return 'idle';
    case 'Run':
    case 'Jog':
    case 'Home':
    case 'Cooling':
    case 'Tool':
      return 'run';
    case 'Hold':
    case 'Door':
    case 'AutoHold':
      return 'hold';
    case 'Alarm':
      return 'alarm';
  }
}

/** Reactive machine state fed by the service. */
export class MachineState {
  #state: State | null = $state(null);
  #visual: VisualState = $derived(visualState(this.#state?.machine.status));
  private pending: Promise<void> | null = null;

  constructor(private readonly service: AppService) {
    service.subscribe((e) => this.onEvent(e));
  }

  /** The last `state.get` (patched by events until the next read). */
  get state(): State | null {
    return this.#state;
  }

  /** One of the six visual states. */
  get visual(): VisualState {
    return this.#visual;
  }

  /** Reads `state.get` again (coalesced). */
  refresh(): Promise<void> {
    this.pending ??= this.service
      .dispatch('state.get', {})
      .then((s) => {
        this.#state = s;
      })
      .finally(() => {
        this.pending = null;
      });
    return this.pending;
  }

  private onEvent(e: EventMessage): void {
    switch (e.event) {
      case 'host':
        if (typeof e.data === 'object' && 'StatusChanged' in e.data && this.#state) {
          this.#state.machine.status = e.data.StatusChanged.new;
        }
        void this.refresh();
        break;
      default:
        void this.refresh();
    }
  }
}
