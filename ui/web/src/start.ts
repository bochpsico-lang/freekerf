import { mount } from 'svelte';

import App from './App.svelte';
import type { BenchReport } from './canvas/bench';
import type { AppService } from './service';
import { MachineState } from './state/machine.svelte';
import { PreviewState } from './state/preview.svelte';
import './styles/app.css';

/** Mounts the app on `#app` with a service (Tauri or fake). */
export function start(service: AppService): void {
  const target = document.getElementById('app');
  if (!target) return;
  const machine = new MachineState(service);
  const preview = new PreviewState(service);
  void machine.refresh();
  void preview.reload();
  mount(App, { target, props: { service, machine, preview } });
}

/** Runs the canvas benchmark full window (loaded on demand: not in the normal start-up). */
export async function startBench(): Promise<BenchReport> {
  const host = document.getElementById('app') ?? document.body;
  Object.assign(host.style, { position: 'fixed', inset: '0' });
  const { runCanvasBench, benchOptions } = await import('./canvas/bench');
  return runCanvasBench(host, benchOptions(new URLSearchParams(location.search)));
}
