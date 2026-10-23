import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import App from './App.svelte';
import { FakeService } from './dev/fake-service';
import { projections } from './start';

async function mountApp(service = new FakeService()) {
  const props = projections(service);
  await Promise.all([props.machine.refresh(), props.view.refresh(), props.settings.refresh()]);
  render(App, props);
  return { service, props };
}

describe('App shell', () => {
  it('shows the machine state and STOP', async () => {
    const { service } = await mountApp();
    expect(screen.getByTestId('state-chip').textContent).toContain('Disconnected');
    await fireEvent.click(screen.getByTestId('estop'));
    expect(service.calls.map((c) => c.action)).toContain('machine.stop');
  });

  it('connects to the emulator from the machine panel', async () => {
    const { service, props } = await mountApp();
    await fireEvent.click(screen.getByTestId('connect'));
    await props.machine.refresh();
    await waitFor(() => expect(screen.getByTestId('state-chip').textContent).toContain('Ready'));
    expect(service.calls.some((c) => c.action === 'machine.connect')).toBe(true);
  });

  it('switches modes through view.set_mode', async () => {
    const { service } = await mountApp();
    await fireEvent.click(screen.getByTestId('mode-design'));
    await waitFor(() => expect(document.documentElement.dataset.mode).toBe('design'));
    expect(service.view.mode).toBe('design');
  });

  it('opens settings and changes the theme through settings.set', async () => {
    const { service } = await mountApp();
    await fireEvent.click(screen.getByTestId('open-settings'));
    await screen.findByTestId('dialog-settings');
    await fireEvent.click(screen.getByTestId('theme-light'));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('light'));
    await fireEvent.click(screen.getByTestId('high-contrast'));
    await waitFor(() => expect(document.documentElement.dataset.contrast).toBe('high'));
    expect(service.settings.ui).toMatchObject({ theme: 'light', high_contrast: true });
  });

  it('keeps STOP clickable with a dialog open and Shift+Esc stops from a text field', async () => {
    const { service } = await mountApp();
    await service.dispatch('view.open_dialog', { name: 'settings' });
    await screen.findByTestId('dialog-settings');
    await fireEvent.click(screen.getByTestId('estop'));
    expect(service.calls.filter((c) => c.action === 'machine.stop')).toHaveLength(1);
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();
    await fireEvent.keyDown(input, { key: 'Escape', shiftKey: true });
    expect(service.calls.filter((c) => c.action === 'machine.stop')).toHaveLength(2);
    input.remove();
  });

  it('Esc closes the dialog on top; the backdrop too', async () => {
    const { service } = await mountApp();
    await service.dispatch('view.open_dialog', { name: 'settings' });
    await service.dispatch('view.open_dialog', { name: 'shortcuts' });
    await screen.findByTestId('dialog-shortcuts');
    await fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => expect(service.view.dialogs).toEqual(['settings']));
    await fireEvent.click(screen.getByTestId('dialog-backdrop'));
    await waitFor(() => expect(service.view.dialogs).toEqual([]));
  });

  it('the fire confirmation is not closed by the backdrop', async () => {
    const { service } = await mountApp();
    await service.dispatch('view.open_dialog', { name: 'fire_confirmation' });
    await screen.findByTestId('dialog-fire_confirmation');
    await fireEvent.click(screen.getByTestId('dialog-backdrop'));
    expect(service.view.dialogs).toEqual(['fire_confirmation']);
  });

  it('shows toasts and action failures as toasts', async () => {
    const service = new FakeService();
    service.on('machine.connect', () => {
      throw new Error('no emulator');
    });
    await mountApp(service);
    await fireEvent.click(screen.getByTestId('connect'));
    await waitFor(() => expect(screen.getByTestId('toasts').textContent).toContain('no emulator'));
  });

  it('keyboard: Ctrl+2 / Ctrl+1 modes, F1 shortcuts', async () => {
    const { service } = await mountApp();
    await fireEvent.keyDown(window, { key: '1', ctrlKey: true });
    await waitFor(() => expect(service.view.mode).toBe('design'));
    await fireEvent.keyDown(window, { key: '2', ctrlKey: true });
    await waitFor(() => expect(service.view.mode).toBe('produce'));
    await fireEvent.keyDown(window, { key: 'F1' });
    await screen.findByTestId('dialog-shortcuts');
  });

  it('the machine and the console are Produce tabs, the layers and the inspector Design ones', async () => {
    const { service } = await mountApp();
    const tabs = () =>
      [...screen.getByTestId('dock').querySelectorAll<HTMLElement>('[role="tab"]')].map(
        (b) => b.dataset.panel,
      );
    await waitFor(() => expect(tabs()).toEqual(['layers', 'object']));
    await service.dispatch('view.set_mode', { mode: 'produce' });
    await waitFor(() => expect(tabs()).toEqual(['machine', 'console']));
  });

  it('F5 and F7 in Design go to Produce and start nothing', async () => {
    const service = new FakeService();
    await service.dispatch('machine.connect', { emulator: true });
    await service.dispatch('job.load_text', { name: 'part.nc', gcode: 'G1 X1' });
    const { props } = await mountApp(service);
    await props.machine.refresh();
    for (const key of ['F5', 'F7']) {
      await service.dispatch('view.set_mode', { mode: 'design' });
      await waitFor(() => expect(document.documentElement.dataset.mode).toBe('design'));
      await fireEvent.keyDown(window, { key });
      await waitFor(() => expect(service.view.mode).toBe('produce'));
    }
    const asked = service.calls.map((c) => c.action);
    expect(asked).not.toContain('fire.request');
    expect(asked).not.toContain('job.frame');
    // in Produce, F5 asks for the fire confirmation
    await waitFor(() => expect(document.documentElement.dataset.mode).toBe('produce'));
    await fireEvent.keyDown(window, { key: 'F5' });
    await screen.findByTestId('dialog-fire_confirmation');
  });

  it('a running job keeps the Produce mode', async () => {
    const service = new FakeService();
    await service.dispatch('machine.connect', { emulator: true });
    const { props } = await mountApp(service);
    service.startJob();
    await props.view.refresh();
    const design = screen.getByTestId('mode-design') as HTMLButtonElement;
    await waitFor(() => expect(design.disabled).toBe(true));
    await fireEvent.keyDown(window, { key: '1', ctrlKey: true });
    await waitFor(() => expect(screen.getByTestId('toasts').textContent).toContain('A job is running'));
    expect(service.view.mode).toBe('produce');
    await service.dispatch('job.abort', {});
    await waitFor(() => expect(design.disabled).toBe(false));
  });
});

describe('layouts', () => {
  it('narrow: machine only, bottom bar with STOP, no Design mode', async () => {
    window.innerWidth = 600;
    const service = new FakeService();
    service.view.mode = 'design';
    await mountApp(service);
    expect(document.documentElement.dataset.layout).toBe('narrow');
    expect(document.documentElement.dataset.mode).toBe('produce');
    expect(screen.getByTestId('bottombar').querySelector('[data-testid="estop"]')).toBeTruthy();
    window.innerWidth = 1300;
  });
});

describe('dock sizes', () => {
  const sets = (service: FakeService) =>
    service.calls.filter((c) => c.action === 'settings.set').map((c) => c.params);

  it('drags the dock width and the Layers height, and keeps them in the settings', async () => {
    window.innerWidth = 1600;
    const service = new FakeService();
    service.view.mode = 'design';
    await mountApp(service);
    const dock = screen.getByTestId('dock');
    dock.getBoundingClientRect = () => new DOMRect(1200, 100, 400, 400);
    const edge = screen.getByTestId('dock-resize-width');
    await fireEvent.pointerDown(edge, { button: 0, pointerId: 1, clientX: 1200 });
    await fireEvent.pointerMove(edge, { pointerId: 1, clientX: 1100 });
    expect(dock.style.width).toContain('500px');
    await fireEvent.pointerUp(edge, { pointerId: 1, clientX: 1100 });
    const split = screen.getByTestId('dock-resize-layers');
    await fireEvent.pointerDown(split, { button: 0, pointerId: 2, clientY: 300 });
    await fireEvent.pointerMove(split, { pointerId: 2, clientY: 340 });
    await fireEvent.pointerUp(split, { pointerId: 2, clientY: 340 });
    await waitFor(() =>
      expect(sets(service)).toEqual([
        { key: 'ui.dock_width_px', value: 500 },
        { key: 'ui.dock_layers_pct', value: 60 },
      ]),
    );
    expect(service.settings.ui).toMatchObject({ dock_width_px: 500, dock_layers_pct: 60 });
    const layers = dock.querySelector<HTMLElement>('.panel[data-panel="layers"]');
    expect(layers?.style.flexBasis).toBe('60%');
    window.innerWidth = 1300;
  });

  it('resizes with the keyboard and a double click goes back to the default size', async () => {
    window.innerWidth = 1600;
    const service = new FakeService();
    service.view.mode = 'design';
    service.settings.ui = { ...service.settings.ui, dock_width_px: 400, dock_layers_pct: 46 };
    await mountApp(service);
    const edge = screen.getByTestId('dock-resize-width');
    await fireEvent.keyDown(edge, { key: 'ArrowLeft' });
    await fireEvent.keyDown(screen.getByTestId('dock-resize-layers'), { key: 'ArrowDown', shiftKey: true });
    await fireEvent.dblClick(edge);
    await waitFor(() =>
      expect(sets(service)).toEqual([
        { key: 'ui.dock_width_px', value: 416 },
        { key: 'ui.dock_layers_pct', value: 54 },
        { key: 'ui.dock_width_px', value: 0 },
      ]),
    );
    expect(screen.getByTestId('dock').style.width).toBe('');
    window.innerWidth = 1300;
  });

  it('the Layers separator is only in the wide Design dock', async () => {
    const service = new FakeService();
    service.view.mode = 'produce';
    await mountApp(service);
    expect(screen.queryByTestId('dock-resize-layers')).toBeNull();
    expect(screen.getByTestId('dock-resize-width')).toBeTruthy();
  });
});

describe('settings dialog', () => {
  it('changes the language through settings.set; user-only settings only as the person', async () => {
    const { service } = await mountApp();
    await service.dispatch('view.open_dialog', { name: 'settings' });
    await screen.findByTestId('dialog-settings');
    await fireEvent.change(screen.getByTestId('language'), { target: { value: 'pt-BR' } });
    await waitFor(() => expect(document.documentElement.lang).toBe('pt-BR'));
    expect(service.settings.ui?.language).toBe('pt-BR');
    expect(screen.getByTestId('state-chip').textContent).toContain('Desconectada');
    // safety and automation: never through settings.set (an action scripts could use)
    await fireEvent.click(screen.getByTestId('settings-tab-safety'));
    await fireEvent.change(screen.getByTestId('settings-countdown'), { target: { value: '5' } });
    await fireEvent.click(screen.getByTestId('settings-tab-automation'));
    await fireEvent.click(screen.getByTestId('settings-mcp'));
    await waitFor(() => expect(service.settings.automation?.mcp?.enabled).toBe(true));
    expect(service.settings.safety?.fire_countdown_s).toBe(5);
    expect(
      service.calls.some(
        (c) =>
          c.action === 'settings.set' && /^(safety|automation)\./.test((c.params as { key: string }).key),
      ),
    ).toBe(false);
    await fireEvent.click(screen.getByTestId('settings-tab-machine'));
    await fireEvent.change(screen.getByLabelText('Velocidade (baud)'), { target: { value: '250000' } });
    expect(
      service.calls.find(
        (c) => c.action === 'settings.set' && (c.params as { key: string }).key === 'machine.baud',
      )?.params,
    ).toEqual({ key: 'machine.baud', value: 250000 });
  });
});
