<script lang="ts">
  // Hosts the canvas (CanvasView): the WebGL2 layer and the overlay. All app data
  // comes in as props; gestures only change the view.
  import { onMount } from 'svelte';

  import type { CameraState, Rect } from './camera';
  import type { OverlayScene } from './overlay';
  import type { HeadFrame, Toolpath } from './toolpath';
  import { CanvasView, NoWebGL2Error, type CanvasViewEvents } from './view';
  import type { PathBatch, RasterImage, Rgba } from './webgl2';

  let {
    mode = 'produce',
    workArea,
    toolpath = null,
    head = null,
    executed = 0,
    units = 'mm',
    oncamera,
    paths = null,
    images = null,
    layerColors = null,
    selection = null,
    design = {},
    view = $bindable(null),
  }: {
    mode?: 'design' | 'produce';
    workArea: Rect;
    toolpath?: Toolpath | null;
    head?: HeadFrame | null;
    executed?: number;
    units?: 'mm' | 'in';
    oncamera?: (c: CameraState) => void;
    /** Design mode: the document geometry, its layer colours and the selection. */
    paths?: PathBatch | null;
    /** Design mode: the images as they will be engraved. */
    images?: RasterImage[] | null;
    layerColors?: Rgba[] | null;
    selection?: OverlayScene['selection'];
    /** Design mode: hit tests, selection and editing gestures. */
    design?: Omit<CanvasViewEvents, 'camera'>;
    view?: CanvasView | null;
  } = $props();

  let host: HTMLDivElement;
  let failure = $state('');

  onMount(() => {
    try {
      // forwarders: the handlers are read when called, so they follow the props
      view = new CanvasView(host, {
        camera: (c) => oncamera?.(c),
        hitTest: (x, y) => design.hitTest?.(x, y) ?? null,
        select: (id, add, keep) => design.select?.(id, add, keep),
        selectArea: (r, crossing, add) => design.selectArea?.(r, crossing, add),
        contextMenu: (x, y, id) => design.contextMenu?.(x, y, id),
        doubleClick: (x, y, id) => design.doubleClick?.(x, y, id),
        hitHandle: (x, y) => design.hitHandle?.(x, y) ?? null,
        drag: (phase, h, w, m) => design.drag?.(phase, h, w, m),
        placing: () => design.placing?.() ?? false,
        place: (r, m) => design.place?.(r, m),
      });
      view.fit(workArea);
    } catch (e) {
      failure = e instanceof NoWebGL2Error ? e.message : String(e);
    }
    return () => {
      view?.dispose();
      view = null;
    };
  });

  $effect(() => view?.setMode(mode));
  $effect(() => view?.setWorkArea(workArea));
  $effect(() => view?.setToolpath(toolpath));
  $effect(() => view?.setProgress(executed));
  $effect(() => view?.setPaths(paths));
  $effect(() => view?.setImages(images ?? []));
  $effect(() => view?.setLayerColors(layerColors));
  $effect(() => view?.setSelection(selection));
  $effect(() => view?.setUnits(units));
  $effect(() =>
    view?.setHead(head ? { x: head.x, y: head.y, burning: head.running && head.power > 0 } : null),
  );
</script>

<div class="canvas-host" bind:this={host} data-testid="canvas">
  {#if failure}<p class="canvas-error" role="alert">{failure}</p>{/if}
</div>

<style>
  .canvas-host {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--fk-color-surface-canvas);
  }
  .canvas-error {
    position: absolute;
    inset: auto 0 0 0;
    margin: 0;
    padding: var(--fk-space-3);
    color: var(--fk-color-danger);
  }
</style>
