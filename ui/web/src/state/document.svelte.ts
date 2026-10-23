// The Design mode document, as the UI reads it: the session snapshot's document
// part (layers, objects, selection, current layer, undo/redo), the drawable
// geometry (raw, "FKTP") and the images as they will be engraved
// (`document.raster_preview`). Read-only: it changes through `document.*` actions
// and comes back as a session event.

import { decodeGreyPng, type GreyImage } from '../canvas/raster';
import { parseColor } from '../canvas/theme';
import { decodeToolpath } from '../canvas/toolpath';
import type { PathBatch, RasterImage, Rgba } from '../canvas/webgl2';
import type { EventMessage, LayerSummary, ObjectSummary, Snapshot } from '../gen/api';
import type { AppService } from '../service';
import { ImagePreviews } from './images';

/** A layer as the Design mode shows it. */
export type Layer = LayerSummary;
export type { ObjectSummary };

/** The document part of the session snapshot. */
export interface DocumentSummary {
  revision: number;
  /** Revision of the last document change (the geometry changes only then). */
  document_revision: number;
  dirty: boolean;
  layers: Layer[];
  objects: ObjectSummary[];
  selection: number[];
  /** The layer new objects go to. */
  current_layer: number | null;
  can_undo: boolean;
  can_redo: boolean;
  /** Objects on the clipboard (`document.copy`, `document.cut`). */
  clipboard: number;
}

/** The summary of a snapshot. */
export function documentSummary(s: Snapshot): DocumentSummary {
  return {
    revision: s.revision,
    document_revision: s.document_revision,
    dirty: s.dirty,
    layers: s.document.layers,
    objects: s.document.objects,
    selection: s.selection,
    current_layer: s.current_layer ?? null,
    can_undo: s.undo.length > 0,
    can_redo: s.redo.length > 0,
    clipboard: s.clipboard,
  };
}

export class DocumentProjection {
  #summary: DocumentSummary | null = $state.raw(null);
  #paths: PathBatch | null = $state.raw(null);
  #error = $state('');
  #geometryRevision = -1;
  #pending: Promise<void> | null = null;
  #again = false;
  #images: RasterImage[] = $state.raw([]);
  #previews: ImagePreviews;

  constructor(
    readonly service: AppService,
    decode: (png: string) => Promise<GreyImage> = decodeGreyPng,
  ) {
    this.#previews = new ImagePreviews(service, decode);
    service.subscribe((e: EventMessage) => {
      if (e.event === 'session' && e.change !== 'view') void this.refresh();
    });
  }

  get summary(): DocumentSummary | null {
    return this.#summary;
  }

  get layers(): Layer[] {
    return this.#summary?.layers ?? [];
  }

  get objects(): ObjectSummary[] {
    return this.#summary?.objects ?? [];
  }

  get selection(): number[] {
    return this.#summary?.selection ?? [];
  }

  /** The selected objects, in drawing order. */
  get selected(): ObjectSummary[] {
    const ids = this.selection;
    return this.objects.filter((o) => ids.includes(o.id));
  }

  /** Drawable segments: `power` = layer index, `command` = object index. */
  get paths(): PathBatch | null {
    return this.#paths;
  }

  /** The image objects as they will be engraved, on their bounds (`layer` = layer index). */
  get images(): RasterImage[] {
    return this.#images;
  }

  /** Layer colours by layer index (for the canvas). */
  get layerColors(): Rgba[] {
    return this.layers.map((l) => parseColor(l.color));
  }

  get error(): string {
    return this.#error;
  }

  /** Reads the snapshot again, and the geometry when the document changed (coalesced). */
  refresh(): Promise<void> {
    if (this.#pending) {
      this.#again = true;
      return this.#pending;
    }
    this.#pending = (async () => {
      do {
        this.#again = false;
        try {
          const s = documentSummary(await this.service.dispatch('session.snapshot', {}));
          if (s.document_revision !== this.#geometryRevision) {
            const t = decodeToolpath(await this.service.documentGeometry());
            this.#paths = { segments: t.segments, segmentCount: t.segmentCount };
            this.#geometryRevision = s.document_revision;
            // after the geometry, in the background: previews can take a while
            void this.#previews.read(s).then((images) => {
              if (images) this.#images = images;
            });
          }
          this.#summary = s;
          this.#error = '';
        } catch (e) {
          this.#error = e instanceof Error ? e.message : String(e);
        }
      } while (this.#again);
      this.#pending = null;
    })();
    return this.#pending;
  }
}
