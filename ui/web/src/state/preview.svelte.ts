// The tool path of the loaded program and the head position, for the canvas.
// Read-only projections of the service (raw IPC payloads, ADR 0006).

import { decodeHead, decodeToolpath, type HeadFrame, type Toolpath } from '../canvas/toolpath';
import type { AppService } from '../service';

export class PreviewState {
  toolpath: Toolpath | null = $state.raw(null);
  head: HeadFrame | null = $state.raw(null);
  error = $state('');

  constructor(private readonly service: AppService) {
    service.subscribe((e) => {
      if (e.event === 'program_loaded') void this.reload();
    });
    service.subscribeHead((f) => {
      try {
        this.head = decodeHead(f);
      } catch (e) {
        this.error = String(e);
      }
    });
  }

  /** Reads the tool path again. */
  async reload(): Promise<void> {
    try {
      const buf = await this.service.toolpath();
      this.toolpath = buf.byteLength ? decodeToolpath(buf) : null;
      this.error = '';
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    }
  }
}
