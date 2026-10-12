import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ContextMenu from './ContextMenu.svelte';

const item = (cmd: string) =>
  screen.getByTestId('context-menu').querySelector<HTMLButtonElement>(`[data-cmd="${cmd}"]`)!;

describe('context menu', () => {
  it('enables the commands that apply and runs the chosen one', async () => {
    const onpick = vi.fn();
    const onclose = vi.fn();
    render(ContextMenu, { x: 10, y: 20, selected: false, canPaste: true, onpick, onclose });
    expect(item('copy').disabled).toBe(true);
    expect(item('delete').disabled).toBe(true);
    expect(item('paste').disabled).toBe(false);
    // the first enabled item has the focus
    expect(document.activeElement).toBe(item('paste'));
    await fireEvent.click(item('paste'));
    expect(onpick).toHaveBeenCalledWith('paste');
    expect(onclose).toHaveBeenCalled();
    // the command runs while the menu (and where it opened) still exists
    expect(onpick.mock.invocationCallOrder[0]).toBeLessThan(onclose.mock.invocationCallOrder[0]!);
  });

  it('keys move between the items; Esc and a press outside close it', async () => {
    const onclose = vi.fn();
    render(ContextMenu, { x: 0, y: 0, selected: true, canPaste: false, onpick: vi.fn(), onclose });
    const menu = screen.getByTestId('context-menu');
    expect(document.activeElement).toBe(item('copy'));
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(item('cut'));
    // paste is disabled: skipped
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(item('duplicate'));
    await fireEvent.keyDown(menu, { key: 'End' });
    expect(document.activeElement).toBe(item('delete'));
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(item('copy'));
    await fireEvent.keyDown(menu, { key: 'ArrowUp' });
    expect(document.activeElement).toBe(item('delete'));
    await fireEvent.keyDown(menu, { key: 'Home' });
    expect(document.activeElement).toBe(item('copy'));
    await fireEvent.keyDown(menu, { key: 'Escape' });
    expect(onclose).toHaveBeenCalledTimes(1);
    await fireEvent.pointerDown(menu);
    expect(onclose).toHaveBeenCalledTimes(1);
    await fireEvent.pointerDown(document.body);
    expect(onclose).toHaveBeenCalledTimes(2);
  });
});
