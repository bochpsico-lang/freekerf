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

describe('settings dialog', () => {
  it('changes the language through settings.set and shows user-only settings read-only', async () => {
    const { service } = await mountApp();
    await service.dispatch('view.open_dialog', { name: 'settings' });
    await screen.findByTestId('dialog-settings');
    await fireEvent.change(screen.getByTestId('language'), { target: { value: 'pt-BR' } });
    await waitFor(() => expect(document.documentElement.lang).toBe('pt-BR'));
    expect(service.settings.ui?.language).toBe('pt-BR');
    expect(screen.getByTestId('state-chip').textContent).toContain('Desconectada');
    await fireEvent.click(screen.getByTestId('settings-tab-safety'));
    expect(screen.getByTestId('settings-safety').querySelector('input')).toBeNull();
    await fireEvent.click(screen.getByTestId('settings-tab-automation'));
    expect(screen.getByTestId('settings-automation').querySelector('input')).toBeNull();
    await fireEvent.click(screen.getByTestId('settings-tab-machine'));
    await fireEvent.change(screen.getByLabelText('Velocidade (baud)'), { target: { value: '250000' } });
    expect(
      service.calls.find(
        (c) => c.action === 'settings.set' && (c.params as { key: string }).key === 'machine.baud',
      )?.params,
    ).toEqual({ key: 'machine.baud', value: 250000 });
  });
});
