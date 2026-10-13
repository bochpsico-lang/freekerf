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
  | 'shortcuts'
  // Design mode
  | 'tool-select'
  | 'tool-rect'
  | 'tool-ellipse'
  | 'tool-text'
  | 'import'
  | 'delete'
  | 'duplicate'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'combine'
  | 'split'
  | 'select-all'
  | 'deselect'
  | 'undo'
  | 'redo'
  | 'nudge-left'
  | 'nudge-right'
  | 'nudge-up'
  | 'nudge-down';

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
  /** The Design mode is shown (editing keys). */
  design?: boolean;
}

const DESIGN_KEYS: Record<string, Command> = {
  v: 'tool-select',
  r: 'tool-rect',
  e: 'tool-ellipse',
  t: 'tool-text',
  Delete: 'delete',
  Backspace: 'delete',
  ArrowLeft: 'nudge-left',
  ArrowRight: 'nudge-right',
  ArrowUp: 'nudge-up',
  ArrowDown: 'nudge-down',
};

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
    return c.dialogOpen ? 'close-dialog' : c.design ? 'deselect' : null;
  }
  if (mod && e.key === '1') return 'mode-design';
  if (mod && e.key === '2') return 'mode-produce';
  if (mod && e.key === '0') return 'fit';
  if (c.design && !c.dialogOpen && mod && !e.altKey) {
    const k = e.key.toLowerCase();
    if (k === 'z') return e.shiftKey ? 'redo' : 'undo';
    if (k === 'y') return 'redo';
    if (k === 'd') return 'duplicate';
    // text fields keep their own copy and paste (typing returns above)
    if (k === 'c') return 'copy';
    if (k === 'x') return 'cut';
    if (k === 'v') return 'paste';
    if (k === 'g') return e.shiftKey ? 'split' : 'combine';
    if (k === 'a') return 'select-all';
    if (k === 'i') return 'import';
  }
  // arrows nudge (Shift: 10 mm, Alt: 0.1 mm: the caller reads the modifiers)
  if (c.design && !c.dialogOpen && !mod && e.key.startsWith('Arrow')) return DESIGN_KEYS[e.key] ?? null;
  if (mod || e.altKey) return null;
  if (c.design && !c.dialogOpen) {
    const d = DESIGN_KEYS[e.key.length === 1 ? e.key.toLowerCase() : e.key];
    if (d) return d;
  }
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
  ['kb-g-tools', 'V', 'tool-select'],
  ['kb-g-tools', 'R', 'tool-rect'],
  ['kb-g-tools', 'E', 'tool-ellipse'],
  ['kb-g-tools', 'T', 'tool-text'],
  ['kb-g-tools', 'Ctrl+I', 'tool-import'],
  ['kb-g-edit', 'Ctrl+Z', 'kb-undo'],
  ['kb-g-edit', 'Ctrl+Y · Ctrl+Shift+Z', 'kb-redo'],
  ['kb-g-edit', 'Ctrl+C · Ctrl+X · Ctrl+V', 'kb-clipboard'],
  ['kb-g-edit', 'Ctrl+D', 'kb-dup'],
  ['kb-g-edit', 'Ctrl+G · Ctrl+Shift+G', 'kb-combine-split'],
  ['kb-g-edit', 'Del', 'kb-del'],
  ['kb-g-edit', 'Ctrl+A', 'kb-all'],
  ['kb-g-edit', '← → ↑ ↓', 'kb-nudge'],
  ['kb-g-edit', 'Shift · Alt', 'kb-constrain'],
  ['kb-g-edit', 'Esc', 'kb-esc'],
];
