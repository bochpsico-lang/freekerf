// Keyboard shortcuts (design/interaction.md §6) as a pure function: a key press and
// a little context in, a command out. The shell turns commands into actions.

export type Command =
  | 'stop'
  | 'start'
  | 'pause'
  | 'frame'
  | 'mode-design'
  | 'mode-produce'
  | 'fit'
  | 'zoom-in'
  | 'zoom-out'
  | 'close-dialog'
  | 'shortcuts';

/** The parts of a KeyboardEvent the shortcuts need. */
export interface KeyInput {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

export interface KeyContext {
  /** Focus in a text field. */
  typing: boolean;
  dialogOpen: boolean;
  /** A job is running (Esc pauses it). */
  running: boolean;
}

/** The command for a key press, or `null`. */
export function shortcut(e: KeyInput, c: KeyContext): Command | null {
  // STOP works everywhere, even in text fields and over dialogs
  if (e.key === 'Escape' && e.shiftKey) return 'stop';
  if (e.key === 'F5') return 'start';
  if (e.key === 'F6') return 'pause';
  if (e.key === 'F7') return 'frame';
  if (e.key === 'F1') return 'shortcuts';
  if (c.typing) return null;
  const mod = e.ctrlKey || e.metaKey;
  if (e.key === 'Escape') {
    if (c.running) return 'pause';
    return c.dialogOpen ? 'close-dialog' : null;
  }
  if (mod && e.key === '1') return 'mode-design';
  if (mod && e.key === '2') return 'mode-produce';
  if (mod && e.key === '0') return 'fit';
  if (mod || e.altKey) return null;
  if (e.key === '+' || e.key === '=') return 'zoom-in';
  if (e.key === '-') return 'zoom-out';
  if (e.key.toLowerCase() === 'f') return 'fit';
  if (e.key === '?') return 'shortcuts';
  return null;
}

/** True when the event target is a text field. */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target.isContentEditable === true;
}

/** The table of the shortcuts dialog: [group, keys, i18n id of the description]. */
export const SHORTCUT_TABLE: [string, string, string][] = [
  ['kb-g-view', '+ / −', 'kb-zoom'],
  ['kb-g-view', 'F · Ctrl+0', 'kb-fit'],
  ['kb-g-view', 'Space + drag', 'kb-pan'],
  ['kb-g-view', 'Wheel', 'kb-wheel'],
  ['kb-g-view', 'Ctrl+1 / Ctrl+2', 'kb-mode'],
  ['kb-g-machine', 'F5', 'kb-start'],
  ['kb-g-machine', 'F6', 'kb-pause'],
  ['kb-g-machine', 'F7', 'kb-frame'],
  ['kb-g-machine', 'Esc', 'kb-hold'],
  ['kb-g-machine', 'Shift+Esc', 'kb-estop'],
];
