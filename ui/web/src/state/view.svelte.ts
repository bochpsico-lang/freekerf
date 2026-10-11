// The view state of the app (Rust `freekerf_session::view`), as the UI reads it.
// Read-only: it changes through `view.*` actions and arrives back as events.

import type { EventMessage, ViewState } from '../gen/api';
import type { AppService } from '../service';

export class ViewProjection {
  #state: ViewState | null = $state(null);

  constructor(private readonly service: AppService) {
    service.subscribe((e: EventMessage) => {
      if (e.event === 'view') this.#state = e.view;
    });
  }

  /** The view state (`null` until the first read). */
  get state(): ViewState | null {
    return this.#state;
  }

  get mode(): ViewState['mode'] {
    return this.#state?.mode ?? 'produce';
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

  async refresh(): Promise<void> {
    this.#state = await this.service.dispatch('view.get', {});
  }
}
