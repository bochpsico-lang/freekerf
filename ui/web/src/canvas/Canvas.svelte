<script lang="ts">
  // Hosts the canvas (CanvasView): the WebGL2 layer and the overlay. All app data
  // comes in as props; gestures only change the view.
  import { onMount } from 'svelte';

  import type { CameraState, Rect } from './camera';
  import type { HeadFrame, Toolpath } from './toolpath';
  import { CanvasView, NoWebGL2Error } from './view';

  let {
    mode = 'produce',
    workArea,
    toolpath = null,
    head = null,
    executed = 0,
    units = 'mm',
    oncamera,
    view = $bindable(null),
  }: {
    mode?: 'design' | 'produce';
    workArea: Rect;
    toolpath?: Toolpath | null;
    head?: HeadFrame | null;
    executed?: number;
    units?: 'mm' | 'in';
    oncamera?: (c: CameraState) => void;
    view?: CanvasView | null;
  } = $props();

  let host: HTMLDivElement;
  let failure = $state('');

  onMount(() => {
    try {
      view = new CanvasView(host, { camera: (c) => oncamera?.(c) });
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
