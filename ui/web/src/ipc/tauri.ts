// The only module that talks to Tauri (lint rule `no-restricted-imports`): the
// rest of the UI sees an `AppService`.

import { Channel, invoke } from '@tauri-apps/api/core';

import type { ActionError, ActionName, ActionSpec, EventMessage } from '../gen/api';
import { ActionFailure, type AppService, type Params, type Result } from '../service';

function isActionError(e: unknown): e is ActionError {
  return typeof e === 'object' && e !== null && 'kind' in e;
}

/** The desktop app's service, over Tauri commands and a channel for events. */
export class TauriService implements AppService {
  private listeners = new Set<(event: EventMessage) => void>();
  private channel: Channel<EventMessage> | null = null;

  async dispatch<A extends ActionName>(action: A, params: Params<A>): Promise<Result<A>> {
    try {
      return await invoke<Result<A>>('dispatch', { action, params });
    } catch (e) {
      if (isActionError(e)) throw new ActionFailure(action, e);
      throw e;
    }
  }

  catalog(): Promise<ActionSpec[]> {
    return invoke<ActionSpec[]>('catalog');
  }

  subscribe(listener: (event: EventMessage) => void): () => void {
    this.listeners.add(listener);
    if (!this.channel) {
      const channel = new Channel<EventMessage>();
      channel.onmessage = (event) => {
        for (const l of this.listeners) l(event);
      };
      this.channel = channel;
      void invoke('subscribe', { events: channel });
    }
    return () => this.listeners.delete(listener);
  }
}
