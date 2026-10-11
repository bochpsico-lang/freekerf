// The only module that talks to Tauri (lint rule `no-restricted-imports`): the
// rest of the UI sees an `AppService`.

import { Channel, invoke } from '@tauri-apps/api/core';

import type { ActionError, ActionName, ActionSpec, CallerKind, EventMessage, GrantScope } from '../gen/api';
import { ActionFailure, type AppService, type Params, type Result } from '../service';

function isActionError(e: unknown): e is ActionError {
  return typeof e === 'object' && e !== null && 'kind' in e;
}

/** The desktop app's service, over Tauri commands and a channel for events. */
export class TauriService implements AppService {
  private listeners = new Set<(event: EventMessage) => void>();
  private channel: Channel<EventMessage> | null = null;
  private headListeners = new Set<(frame: ArrayBuffer) => void>();
  private headChannel: Channel<ArrayBuffer> | null = null;

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

  async confirmFire(id: number): Promise<unknown> {
    try {
      return await invoke('confirm_fire', { id });
    } catch (e) {
      if (isActionError(e)) throw new ActionFailure('fire', e);
      throw e;
    }
  }

  async toolpath(): Promise<ArrayBuffer> {
    return toArrayBuffer(await invoke<ArrayBuffer | number[]>('preview'));
  }

  answerPermission(id: number, scope: GrantScope | null): Promise<boolean> {
    return invoke<boolean>('answer_permission', { id, scope });
  }

  async setUserSetting(key: string, value: unknown): Promise<void> {
    try {
      await invoke('set_user_setting', { key, value });
    } catch (e) {
      if (isActionError(e)) throw new ActionFailure('settings', e);
      throw e;
    }
  }

  revokeGrants(kind: CallerKind, name: string): Promise<number> {
    return invoke<number>('revoke_grants', { kind, name });
  }

  cancelScripts(): Promise<void> {
    return invoke('cancel_scripts');
  }

  async documentGeometry(): Promise<ArrayBuffer> {
    return toArrayBuffer(await invoke<ArrayBuffer | number[]>('document_geometry'));
  }

  subscribeHead(listener: (frame: ArrayBuffer) => void): () => void {
    this.headListeners.add(listener);
    if (!this.headChannel) {
      const channel = new Channel<ArrayBuffer>();
      channel.onmessage = (frame) => {
        const buf = toArrayBuffer(frame);
        for (const l of this.headListeners) l(buf);
      };
      this.headChannel = channel;
      void invoke('subscribe_head', { frames: channel });
    }
    return () => this.headListeners.delete(listener);
  }
}

/** Raw IPC bodies arrive as ArrayBuffer (some platforms give a number array). */
function toArrayBuffer(data: ArrayBuffer | ArrayLike<number>): ArrayBuffer {
  if (data instanceof ArrayBuffer) return data;
  return Uint8Array.from(data).buffer;
}
