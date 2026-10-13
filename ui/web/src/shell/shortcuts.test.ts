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

  it('editing keys only in the Design mode, never while typing or over a dialog', () => {
    const design: KeyContext = { ...idle, design: true };
    expect(shortcut(key('r'), design)).toBe('tool-rect');
    expect(shortcut(key('V'), design)).toBe('tool-select');
    expect(shortcut(key('e'), design)).toBe('tool-ellipse');
    expect(shortcut(key('t'), design)).toBe('tool-text');
    expect(shortcut(key('Delete'), design)).toBe('delete');
    expect(shortcut(key('Backspace'), design)).toBe('delete');
    expect(shortcut(key('d', { ctrl: true }), design)).toBe('duplicate');
    expect(shortcut(key('c', { ctrl: true }), design)).toBe('copy');
    expect(shortcut(key('X', { ctrl: true }), design)).toBe('cut');
    expect(shortcut(key('v', { meta: true }), design)).toBe('paste');
    expect(shortcut(key('g', { ctrl: true }), design)).toBe('combine');
    expect(shortcut(key('G', { ctrl: true, shift: true }), design)).toBe('split');
    expect(shortcut(key('v', { ctrl: true }), { ...design, typing: true })).toBeNull();
    expect(shortcut(key('v', { ctrl: true }), idle)).toBeNull();
    expect(shortcut(key('F2'), design)).toBe('edit-text');
    expect(shortcut(key('F2'), idle)).toBeNull();
    expect(shortcut(key('F2'), { ...design, typing: true })).toBeNull();
    expect(shortcut(key('Enter'), design)).toBeNull(); // buttons keep Enter
    expect(shortcut(key('a', { meta: true }), design)).toBe('select-all');
    expect(shortcut(key('i', { ctrl: true }), design)).toBe('import');
    expect(shortcut(key('z', { ctrl: true }), design)).toBe('undo');
    expect(shortcut(key('Z', { ctrl: true, shift: true }), design)).toBe('redo');
    expect(shortcut(key('y', { ctrl: true }), design)).toBe('redo');
    expect(shortcut(key('ArrowLeft', { alt: true }), design)).toBe('nudge-left');
    expect(shortcut(key('ArrowUp', { shift: true }), design)).toBe('nudge-up');
    expect(shortcut(key('Escape'), design)).toBe('deselect');
    expect(shortcut(key('r'), idle)).toBeNull();
    expect(shortcut(key('Delete'), { ...design, typing: true })).toBeNull();
    expect(shortcut(key('Delete'), { ...design, dialogOpen: true })).toBeNull();
    expect(shortcut(key('z', { ctrl: true }), { ...design, dialogOpen: true })).toBeNull();
    // STOP still wins
    expect(shortcut(key('Escape', { shift: true }), design)).toBe('stop');
  });
});
