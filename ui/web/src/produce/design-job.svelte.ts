// The job made from the design (`job.from_document`), shared by the machine panel
// and the banner over the Produce canvas: how it is placed (the panel's choices)
// and whether it is being planned (a large image takes seconds).

import type { Anchor, JobStart, ProgramInfo } from '../gen/api';
import type { AppService } from '../service';

class DesignJob {
  /** Where the job starts. */
  start: JobStart = $state('absolute');
  /** Which point of the job's box goes to the start point. */
  anchor: Anchor = $state('bottom_left');
  #busy = $state(false);

  /** True while the design is being planned. */
  get busy(): boolean {
    return this.#busy;
  }

  /**
   * Plans the design into the job, placed as `placement` says (default: the
   * panel's choices). One at a time.
   */
  async make(
    service: AppService,
    run: (action: () => Promise<unknown>) => Promise<void>,
    placement: { start: JobStart; anchor: Anchor } = { start: this.start, anchor: this.anchor },
  ): Promise<void> {
    if (this.#busy) return;
    this.#busy = true;
    try {
      await run(() => service.dispatch('job.from_document', placement));
    } finally {
      this.#busy = false;
    }
  }
}

export const designJob = new DesignJob();

/**
 * What the Produce banner offers for the design: `none` (no job: make it),
 * `stale` (the job came from an older design: update it) or nothing.
 */
export function designJobOffer(
  objects: number,
  documentRevision: number | undefined,
  program: ProgramInfo | null,
): 'none' | 'stale' | null {
  if (!objects) return null;
  if (!program) return 'none';
  const from = program.from_document;
  return from && documentRevision !== undefined && from.document_revision !== documentRevision
    ? 'stale'
    : null;
}
