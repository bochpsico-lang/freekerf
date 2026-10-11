// The view state of the app (Rust `freekerf_session::view`), as the UI reads it.
// Read-only: it changes through `view.*` actions and arrives back as events.

import type { EventMessage, ViewState } from '../gen/api';
import type { AppService } from '../service';

export class ViewProjection {
  #state: ViewState | null = $state(null);

  constructor(private readonly service: AppService) {
    // the view lives in the session: a session event says it changed, read it again
    service.subscribe((e: EventMessage) => {
      if (e.event === 'session' && e.change === 'view') void this.refresh();
    });
  }

  /** The view state (`null` until the first read). */
  get state(): ViewState | null {
    return this.#state;
  }

  get mode(): ViewState['mode'] {
    return this.#state?.mode ?? 'design';
  }

  get tool(): ViewState['tool'] {
    return this.#state?.tool ?? 'select';
  }

  get panel(): ViewState['panel'] {
    return this.#state?.panel ?? 'machine';
  }

  get dialogs(): ViewState['dialogs'] {
    return this.#state?.dialogs ?? [];
  }

  /** The dialog on top. */
  get topDialog(): ViewState['dialogs'][number] | null {
    return this.dialogs.at(-1) ?? null;
  }

  #reading: Promise<void> | null = null;
  #again = false;

  /** Reads the view again (coalesced: one read in flight, one more if asked meanwhile). */
  refresh(): Promise<void> {
    if (this.#reading) {
      this.#again = true;
      return this.#reading;
    }
    this.#reading = (async () => {
      do {
        this.#again = false;
        this.#state = await this.service.dispatch('view.get', {});
      } while (this.#again);
      this.#reading = null;
    })();
    return this.#reading;
  }
}
