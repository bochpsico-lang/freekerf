// What scripts and agents are doing, as the window shows it (`automation.status`,
// read again on every automation event): agents attached, scripts running,
// permission requests waiting for the person.

import type { AutomationStatus, EventMessage, PendingPermission } from '../gen/api';
import type { AppService } from '../service';

export class AutomationProjection {
  #status: AutomationStatus = $state.raw({ agents: [], scripts: [], requests: [] });

  constructor(private readonly service: AppService) {
    service.subscribe((e: EventMessage) => {
      if (e.event === 'automation' && e.data.kind !== 'logged') void this.refresh();
    });
  }

  get status(): AutomationStatus {
    return this.#status;
  }

  /** The oldest request waiting for an answer. */
  get request(): PendingPermission | null {
    return this.#status.requests[0] ?? null;
  }

  async refresh(): Promise<void> {
    this.#status = await this.service.dispatch('automation.status', {});
  }
}
