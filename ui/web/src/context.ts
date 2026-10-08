// What every shell component reads: the service (to dispatch) and the read-only
// projections of app state. Set once by App.svelte.

import { getContext, setContext } from 'svelte';

import type { AppService } from './service';
import type { Environment } from './state/environment.svelte';
import type { MachineState } from './state/machine.svelte';
import type { PreviewState } from './state/preview.svelte';
import type { SettingsProjection } from './state/settings.svelte';
import type { ViewProjection } from './state/view.svelte';

export interface AppContext {
  service: AppService;
  machine: MachineState;
  preview: PreviewState;
  view: ViewProjection;
  settings: SettingsProjection;
  env: Environment;
  /** Runs an action and shows a failure as a toast (through `view.toast`). */
  run(action: () => Promise<unknown>): Promise<void>;
}

const KEY = Symbol('freekerf-app');

export function setApp(ctx: AppContext): void {
  setContext(KEY, ctx);
}

export function app(): AppContext {
  return getContext<AppContext>(KEY);
}
