import { mount } from 'svelte';

import App from './App.svelte';
import type { AppService } from './service';
import { MachineState } from './state/machine.svelte';
import './styles/app.css';

/** Mounts the app on `#app` with a service (Tauri or fake). */
export function start(service: AppService): void {
  const target = document.getElementById('app');
  if (!target) return;
  const machine = new MachineState(service);
  void machine.refresh();
  mount(App, { target, props: { service, machine } });
}
