// Automation in the window: the person answers permission requests, sees agents
// and running scripts, cancels a script, revokes grants and reads the action log.
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import App from '../App.svelte';
import { FakeService } from '../dev/fake-service';
import { projections } from '../start';

async function mount() {
  const service = new FakeService();
  const props = projections(service);
  await Promise.all([props.view.refresh(), props.settings.refresh(), props.automation.refresh()]);
  render(App, props);
  return { service, props };
}

describe('automation in the window', () => {
  it('asks the person and sends their answer, never as an action', async () => {
    const { service } = await mount();
    expect(screen.queryByTestId('dialog-permission')).toBeNull();
    service.automation.requests = [
      { id: 7, actor: { kind: 'agent', name: 'claude' }, action: 'machine.home', tier: 'machine' },
    ];
    service.emitAutomation();
    const dialog = await screen.findByTestId('dialog-permission');
    expect(within(dialog).getByTestId('perm-text').textContent).toContain('claude');
    expect(within(dialog).getByTestId('perm-text').textContent).toContain('machine.home');
    await fireEvent.click(within(dialog).getByTestId('perm-session'));
    await waitFor(() => expect(screen.queryByTestId('dialog-permission')).toBeNull());
    expect(service.answers).toEqual([{ id: 7, scope: 'session' }]);
    // a macro, denied
    service.automation.requests = [
      { id: 8, actor: { kind: 'macro', name: 'frame.js' }, action: 'machine.jog', tier: 'machine' },
    ];
    service.emitAutomation();
    await fireEvent.click(await screen.findByTestId('perm-deny'));
    await waitFor(() => expect(service.answers.at(-1)).toEqual({ id: 8, scope: null }));
    expect(service.calls.some((c) => String(c.action).startsWith('permission'))).toBe(false);
  });

  it('shows agents and the running script, and cancels it', async () => {
    const { service } = await mount();
    service.automation.agents = ['claude'];
    service.automation.scripts = [{ id: 1, name: 'frame.js', actor: { kind: 'macro', name: 'frame.js' } }];
    service.emitAutomation();
    await waitFor(() => expect(screen.getByTestId('agents-chip').textContent).toContain('Agent connected'));
    expect(screen.getByTestId('script-chip').textContent).toContain('frame.js');
    await fireEvent.click(screen.getByTestId('script-cancel'));
    await waitFor(() => expect(screen.queryByTestId('script-chip')).toBeNull());
    expect(service.scriptsCancelled).toBe(1);
  });

  it('lists grants with revoke, and the recent actions', async () => {
    const { service } = await mount();
    service.grants = [{ scope: 'always', caller: 'agent', name: 'claude', tier: 'machine' }];
    service.automationLog = [
      { seq: 1, actor: { kind: 'agent', name: 'claude' }, action: 'view.set_mode', error: null },
      {
        seq: 2,
        actor: { kind: 'agent', name: 'claude' },
        action: 'machine.home',
        error: 'permission denied',
      },
    ];
    await service.dispatch('view.open_dialog', { name: 'settings' });
    await fireEvent.click(await screen.findByTestId('settings-tab-automation'));
    const log = await screen.findByTestId('settings-log');
    await waitFor(() => expect(log.querySelectorAll('li')).toHaveLength(2));
    expect(log.textContent).toContain('machine.home (refused)');
    const grants = screen.getByTestId('settings-grants');
    expect(grants.textContent).toContain('claude · machine · Always');
    await fireEvent.click(within(grants).getByText('Revoke'));
    await waitFor(() => expect(grants.textContent).toContain('None.'));
    expect(service.grants).toEqual([]);
  });
});
