// Purely visual facts of this window (PLAN §3: allowed to stay local): the layout
// from the window width and the kind of the last pointer, which drives the
// automatic density (design/interaction.md §1, §7). Nothing here is app state.

export type Layout = 'wide' | 'medium' | 'narrow';
export type InputKind = 'mouse' | 'pen' | 'touch';

/** Layout for a window width (design/ux.md: ≥ 1180 wide, 820–1179 medium, < 820 narrow). */
export function layoutFor(width: number): Layout {
  return width >= 1180 ? 'wide' : width >= 820 ? 'medium' : 'narrow';
}

/** Density in effect: the setting, or from the last input when automatic. */
export function resolveDensity(setting: string, input: InputKind): 'compact' | 'comfortable' {
  if (setting === 'compact' || setting === 'comfortable') return setting;
  return input === 'touch' ? 'comfortable' : 'compact';
}

export class Environment {
  layout: Layout = $state('wide');
  input: InputKind = $state('mouse');
  private readonly detach: (() => void)[] = [];

  constructor(win: Window | null = typeof window === 'undefined' ? null : window) {
    if (!win) return;
    const resize = () => {
      this.layout = layoutFor(win.innerWidth);
    };
    // the input kind changes on a new press, never in the middle of a drag
    const down = (e: PointerEvent) => {
      if (e.pointerType === 'touch' || e.pointerType === 'pen' || e.pointerType === 'mouse')
        this.input = e.pointerType;
    };
    resize();
    if (win.matchMedia?.('(pointer: coarse)').matches) this.input = 'touch';
    win.addEventListener('resize', resize);
    win.addEventListener('pointerdown', down, { capture: true });
    this.detach.push(
      () => win.removeEventListener('resize', resize),
      () => win.removeEventListener('pointerdown', down, { capture: true }),
    );
  }

  dispose(): void {
    for (const d of this.detach) d();
  }
}
