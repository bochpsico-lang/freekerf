import { describe, expect, it } from 'vitest';

import { isTyping, shortcut, type KeyContext } from './shortcuts';

const key = (
  k: string,
  mods: Partial<{ shift: boolean; ctrl: boolean; alt: boolean; meta: boolean }> = {},
) => ({
  key: k,
  shiftKey: !!mods.shift,
  ctrlKey: !!mods.ctrl,
  metaKey: !!mods.meta,
  altKey: !!mods.alt,
});
const idle: KeyContext = { typing: false, dialogOpen: false, running: false };

describe('shortcuts', () => {
  it('STOP works everywhere', () => {
    for (const c of [
      idle,
      { ...idle, typing: true },
      { ...idle, dialogOpen: true },
      { ...idle, running: true },
    ]) {
      expect(shortcut(key('Escape', { shift: true }), c)).toBe('stop');
    }
  });

  it('function keys for the machine', () => {
    expect(shortcut(key('F5'), { ...idle, typing: true })).toBe('start');
    expect(shortcut(key('F6'), idle)).toBe('pause');
    expect(shortcut(key('F7'), idle)).toBe('frame');
    expect(shortcut(key('F1'), idle)).toBe('shortcuts');
  });

  it('Esc pauses a running job, else closes the dialog on top', () => {
    expect(shortcut(key('Escape'), { ...idle, running: true, dialogOpen: true })).toBe('pause');
    expect(shortcut(key('Escape'), { ...idle, dialogOpen: true })).toBe('close-dialog');
    expect(shortcut(key('Escape'), idle)).toBeNull();
  });

  it('view and mode keys, not while typing', () => {
    expect(shortcut(key('1', { ctrl: true }), idle)).toBe('mode-design');
    expect(shortcut(key('2', { meta: true }), idle)).toBe('mode-produce');
    expect(shortcut(key('0', { ctrl: true }), idle)).toBe('fit');
    expect(shortcut(key('+'), idle)).toBe('zoom-in');
    expect(shortcut(key('='), idle)).toBe('zoom-in');
    expect(shortcut(key('-'), idle)).toBe('zoom-out');
    expect(shortcut(key('F'), idle)).toBe('fit');
    expect(shortcut(key('?'), idle)).toBe('shortcuts');
    expect(shortcut(key('f'), { ...idle, typing: true })).toBeNull();
    expect(shortcut(key('f', { alt: true }), idle)).toBeNull();
    expect(shortcut(key('x'), idle)).toBeNull();
  });

  it('knows text fields', () => {
    expect(isTyping(document.createElement('input'))).toBe(true);
    expect(isTyping(document.createElement('textarea'))).toBe(true);
    expect(isTyping(document.createElement('button'))).toBe(false);
    expect(isTyping(null)).toBe(false);
  });
});
