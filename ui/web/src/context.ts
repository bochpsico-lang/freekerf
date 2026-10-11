// What every shell component reads: the service (to dispatch) and the read-only
// projections of app state. Set once by App.svelte.

import { getContext, setContext } from 'svelte';

import type { Actions } from './gen/api';
import type { AppService } from './service';
import type { AutomationProjection } from './state/automation.svelte';
import type { ConsoleProjection } from './state/console.svelte';
import type { DocumentProjection } from './state/document.svelte';
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
  console: ConsoleProjection;
  /** Agents, scripts and permission requests. */
  automation: AutomationProjection;
  /** The Design mode document. */
  doc: DocumentProjection;
  /**
   * Asks to run an action that can turn the laser on: it only runs after the human
   * confirmation on screen (checklist and countdown, PLAN §9.5).
   */
  laser(request: LaserRequest): Promise<void>;
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

/** The actions of tier `laser` and their parameters. */
export type LaserRequest =
  | { action: 'job.run'; params: Actions['job.run']['params'] }
  | { action: 'job.resume'; params: Actions['job.resume']['params'] }
  | { action: 'machine.send'; params: Actions['machine.send']['params'] }
  | { action: 'machine.custom_code'; params: Actions['machine.custom_code']['params'] };
