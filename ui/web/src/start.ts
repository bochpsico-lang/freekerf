import { mount } from 'svelte';

import App from './App.svelte';
import type { BenchReport } from './canvas/bench';
import type { AppService } from './service';
import { ConsoleProjection } from './state/console.svelte';
import { DocumentProjection } from './state/document.svelte';
import { Environment } from './state/environment.svelte';
import { MachineState } from './state/machine.svelte';
import { PreviewState } from './state/preview.svelte';
import { SettingsProjection } from './state/settings.svelte';
import { ViewProjection } from './state/view.svelte';
import './styles/app.css';

/** The projections of app state for a service (also used by component tests). */
export function projections(service: AppService) {
  return {
    service,
    machine: new MachineState(service),
    preview: new PreviewState(service),
    view: new ViewProjection(service),
    settings: new SettingsProjection(service),
    env: new Environment(),
    console: new ConsoleProjection(service),
    doc: service.document ? new DocumentProjection(service.document) : null,
  };
}

/** Mounts the app on `#app` with a service (Tauri or fake). */
export function start(service: AppService): void {
  const target = document.getElementById('app');
  if (!target) return;
  const props = projections(service);
  void Promise.all([
    props.machine.refresh(),
    props.view.refresh(),
    props.settings.refresh(),
    props.preview.reload(),
    props.doc?.refresh(),
  ]);
  mount(App, { target, props });
}

/** Runs the canvas benchmark full window (loaded on demand: not in the normal start-up). */
export async function startBench(): Promise<BenchReport> {
  const host = document.getElementById('app') ?? document.body;
  Object.assign(host.style, { position: 'fixed', inset: '0', display: 'block' });
  const { runCanvasBench, benchOptions } = await import('./canvas/bench');
  return runCanvasBench(host, benchOptions(new URLSearchParams(location.search)));
}
