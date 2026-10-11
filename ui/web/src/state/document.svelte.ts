// The Design mode document, as the UI reads it: the session snapshot's document
// part (layers, objects, selection, current layer, undo/redo) and the drawable
// geometry (raw, "FKTP"). Read-only: it changes through `document.*` actions and
// comes back as a session event.

import { parseColor } from '../canvas/theme';
import { decodeToolpath } from '../canvas/toolpath';
import type { Rgba, PathBatch } from '../canvas/webgl2';
import type { EventMessage, LayerSummary, ObjectSummary, Snapshot } from '../gen/api';
import type { AppService } from '../service';

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
  };
}

export class DocumentProjection {
  #summary: DocumentSummary | null = $state.raw(null);
  #paths: PathBatch | null = $state.raw(null);
  #error = $state('');
  #geometryRevision = -1;
  #pending: Promise<void> | null = null;
  #again = false;

  constructor(readonly service: AppService) {
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
