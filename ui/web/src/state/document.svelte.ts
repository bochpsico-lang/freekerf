// The Design mode document (doc/features/ui-contract.md), as the UI reads it: the
// summary (layers, objects, selection) and the drawable geometry (raw, "FKTP").
// Read-only: it changes through `document.*` actions and comes back as an event.

import { parseColor } from '../canvas/theme';
import { decodeToolpath } from '../canvas/toolpath';
import type { Rgba, PathBatch } from '../canvas/webgl2';
import type { DocumentService, DocumentSummary, Layer, ObjectSummary } from '../contract/document';

export class DocumentProjection {
  #summary: DocumentSummary | null = $state.raw(null);
  #paths: PathBatch | null = $state.raw(null);
  #error = $state('');
  #geometryRevision = -1;
  #pending: Promise<void> | null = null;
  #again = false;

  constructor(readonly service: DocumentService) {
    service.subscribe(() => void this.refresh());
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

  /** Reads the summary again, and the geometry when it changed (coalesced). */
  refresh(): Promise<void> {
    if (this.#pending) {
      this.#again = true;
      return this.#pending;
    }
    this.#pending = (async () => {
      do {
        this.#again = false;
        try {
          const s = await this.service.dispatch('document.get', {});
          if (s.geometry_revision !== this.#geometryRevision) {
            const t = decodeToolpath(await this.service.geometry());
            this.#paths = { segments: t.segments, segmentCount: t.segmentCount };
            this.#geometryRevision = s.geometry_revision;
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
