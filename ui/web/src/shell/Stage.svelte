<script lang="ts">
  // The canvas area: canvas, zoom buttons, the large progress readout of the
  // Produce mode. The camera is local while a gesture runs; when it settles it is
  // committed to the view state (`view.set_viewport`), and viewports or fit
  // requests coming from the view state (scripts, agents) are applied here.
  import { onDestroy } from 'svelte';

  import Canvas from '../canvas/Canvas.svelte';
  import { Camera, type CameraState } from '../canvas/camera';
  import type { CanvasView } from '../canvas/view';
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { formatDuration } from '../lib/format';
  import ContextMenu from '../design/ContextMenu.svelte';
  import { DesignEditor, type EditCommand } from '../design/editor';
  import Palette from '../design/Palette.svelte';
  import TextEditor from '../design/TextEditor.svelte';
  import AlarmBanner from '../produce/AlarmBanner.svelte';

  let { mode = 'produce', onzoom }: { mode?: 'design' | 'produce'; onzoom?: (realSize: number) => void } =
    $props();
  const { view, preview, machine, settings, service, doc, env, run } = app();
  const design = $derived(mode === 'design' && !!doc);

  // the camera as last reported (the selection handles are drawn in screen space)
  let cam: CameraState | null = $state.raw(null);
  // bumped while a handle drag changes its preview
  let tick = $state(0);
  const editor = doc
    ? new DesignEditor({
        doc: () => doc.summary,
        tool: () => view.tool,
        camera: () => cam,
        geometry: () => doc.paths,
        coarse: () => env.input === 'touch',
        service: doc.service,
        run,
        selectTool: () => void run(() => service.dispatch('view.set_tool', { tool: 'select' })),
        // a new text is typed right where it was placed
        textPlaced: (id) => (editing = id),
        changed: () => tick++,
      })
    : null;
  const selection = $derived.by(() => {
    void tick;
    void doc?.summary;
    void cam;
    return design && editor ? editor.overlay() : null;
  });
  // the work area from the board ($130/$131), 400×400 mm until it is known
  const workArea = $derived(
    machine.info?.settings_known
      ? { x: 0, y: 0, width: machine.info.table_width_mm, height: machine.info.table_height_mm }
      : { x: 0, y: 0, width: 400, height: 400 },
  );
  /** CSS pixels per mm at 100 % (96 dpi). */
  const REAL_SIZE = 96 / 25.4;
  const COMMIT_MS = 250;

  let canvas: CanvasView | null = $state(null);
  let sent: { center: [number, number]; zoom: number } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastFit = -1;

  function camera(c: CameraState) {
    cam = c;
    onzoom?.(c.scale / REAL_SIZE);
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      sent = { center: [c.cx, c.cy], zoom: c.scale };
      void service.dispatch('view.set_viewport', sent);
    }, COMMIT_MS);
  }

  const near = (a: number, b: number) => Math.abs(a - b) <= 1e-6 * Math.max(1, Math.abs(a), Math.abs(b));

  $effect(() => {
    const v = view.state?.viewport;
    if (!canvas || !v) return;
    const [cx, cy] = v.center;
    if (sent && near(cx, sent.center[0]) && near(cy, sent.center[1]) && near(v.zoom, sent.zoom)) return;
    const c = canvas.camera;
    if (near(cx, c.cx) && near(cy, c.cy) && near(v.zoom, c.scale)) return;
    sent = { center: [cx, cy], zoom: v.zoom };
    canvas.setCamera({ ...c, cx, cy, scale: v.zoom });
  });

  $effect(() => {
    const f = view.state?.fit_request ?? 0;
    if (!canvas) return;
    if (lastFit >= 0 && f !== lastFit) canvas.fit();
    lastFit = f;
  });

  // the canvas reads its colours from the tokens: read them again when the theme
  // changes (after App.svelte has updated the <html> attributes)
  $effect(() => {
    void settings.ui.theme;
    void settings.ui.high_contrast;
    const c = canvas;
    if (c) requestAnimationFrame(() => c.refreshTheme());
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
  });

  /** Moves the selection (arrow keys). */
  export function nudge(dx: number, dy: number, mods: { shift: boolean; alt: boolean }) {
    return editor?.nudge(dx, dy, mods);
  }

  /** Copy, cut, paste, duplicate or delete (keys). */
  export function edit(cmd: EditCommand) {
    return editor?.edit(cmd);
  }

  // the context menu: where it opened (stage pixels) and that point in the world
  let menu: { x: number; y: number; at: [number, number] } | null = $state(null);

  function contextMenu(x: number, y: number, id: string | null) {
    if (!design || !editor || !cam) return;
    // a right click on an object outside the selection selects it first
    if (id !== null) editor.select(id, false, true);
    menu = { x, y, at: Camera.toWorld(cam, x, y) };
  }

  // the text object being edited on the canvas
  let editing: number | null = $state(null);
  const editingText = $derived(
    design && editing !== null
      ? (doc?.objects.find((o) => o.id === editing && o.kind === 'text') ?? null)
      : null,
  );

  /** Edits the selected text on the canvas (F2), when exactly one text is selected. */
  export function editText() {
    const sel = doc?.selected ?? [];
    if (design && sel.length === 1 && sel[0]!.kind === 'text') editing = sel[0]!.id;
  }

  function doubleClick(_x: number, _y: number, id: string | null) {
    const o = id === null ? null : doc?.objects.find((x) => x.id === Number(id));
    if (o?.kind === 'text') editing = o.id;
  }

  /** Zoom by a factor around the centre (buttons and keys). */
  export function zoom(factor: number) {
    canvas?.zoom(factor);
  }

  const job = $derived(machine.state?.machine);
  const pct = $derived(
    job && job.in_program && job.job_target ? Math.floor((100 * job.job_executed) / job.job_target) : null,
  );
</script>

<section class="stage" id="stage" aria-label="Canvas" tabindex="-1">
  <Canvas
    {workArea}
    {mode}
    toolpath={design ? null : preview.toolpath}
    paths={design ? (doc?.paths ?? null) : null}
    layerColors={design ? (doc?.layerColors ?? null) : null}
    {selection}
    design={design && editor
      ? {
          hitTest: editor.hitTest,
          select: editor.select,
          selectArea: editor.selectArea,
          hitHandle: editor.hitHandle,
          drag: editor.drag,
          placing: editor.placing,
          place: editor.place,
          contextMenu,
          doubleClick,
        }
      : {}}
    head={preview.head}
    executed={preview.head?.executed ?? 0}
    units={settings.ui.units}
    oncamera={camera}
    bind:view={canvas}
  />
  {#if pct !== null}
    <div class="readout show" data-testid="readout">
      <div class="readout-pct">{pct}%</div>
      <div class="readout-sub">
        {t('machine-remaining')}
        {formatDuration(Math.max(0, (job?.projected_ms ?? 0) - (job?.job_time_ms ?? 0)))}
      </div>
    </div>
  {/if}
  {#if menu && design && editor && doc}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      selected={doc.selection.length > 0}
      canPaste={(doc.summary?.clipboard ?? 0) > 0}
      onpick={(cmd) => void editor.edit(cmd, menu?.at)}
      onclose={() => (menu = null)}
    />
  {/if}
  {#if editingText && cam}
    <TextEditor
      object={editingText}
      camera={cam}
      onsubmit={(text) => {
        const id = editing;
        editing = null;
        if (id !== null) void run(() => service.dispatch('document.set_text', { id, text }));
      }}
      oncancel={() => (editing = null)}
    />
  {/if}
  <AlarmBanner />
  {#if design && doc}<Palette {doc} />{/if}
  <div class="canvas-tools" aria-label="View">
    <button
      class="icon-btn"
      title={t('view-zoomOut')}
      aria-label={t('view-zoomOut')}
      onclick={() => zoom(0.8)}><Icon name="zoomOut" /></button
    >
    <button
      class="icon-btn"
      title={t('view-fit')}
      aria-label={t('view-fit')}
      onclick={() => void service.dispatch('view.fit', {})}><Icon name="fit" /></button
    >
    <button class="icon-btn" title={t('view-zoomIn')} aria-label={t('view-zoomIn')} onclick={() => zoom(1.25)}
      ><Icon name="zoomIn" /></button
    >
  </div>
  <div class="narrow-note">{t('mode-narrowNote')}</div>
</section>
