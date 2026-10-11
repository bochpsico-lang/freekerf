// The contract between the UI and the app (PLAN §9, principle 7): the UI reads
// state and events, and changes app state only through `dispatch`. Two
// implementations exist: `ipc/tauri.ts` (the desktop app) and `dev/fake-service.ts`
// (browser development and component tests; never in production builds).

import type { ActionError, ActionName, ActionSpec, Actions, EventMessage } from './gen/api';

/** Parameters of an action. */
export type Params<A extends ActionName> = Actions[A]['params'];
/** Result of an action. */
export type Result<A extends ActionName> = Actions[A]['result'];

/** An action error from the app (refused, invalid parameters, permission...). */
export class ActionFailure extends Error {
  readonly error: ActionError;

  constructor(action: string, error: ActionError) {
    super(`${action}: ${describeError(error)}`);
    this.name = 'ActionFailure';
    this.error = error;
  }
}

/** One line for an action error. */
export function describeError(error: ActionError): string {
  switch (error.kind) {
    case 'permission_denied':
      return `permission denied (needs tier ${error.message.needed})`;
    default:
      return error.message;
  }
}

/** What the UI may call. */
export interface AppService {
  /** Runs an action (rejects with {@link ActionFailure}). */
  dispatch<A extends ActionName>(action: A, params: Params<A>): Promise<Result<A>>;
  /** The action catalogue. */
  catalog(): Promise<ActionSpec[]>;
  /** Service events from now on; returns the unsubscribe function. */
  subscribe(listener: (event: EventMessage) => void): () => void;
  /**
   * Tool path of the loaded program, raw (`canvas/toolpath.ts`, format "FKTP");
   * empty when no program is loaded.
   */
  toolpath(): Promise<ArrayBuffer>;
  /** Head frames (raw, 24 bytes, up to 60 Hz while they change); returns the unsubscribe function. */
  subscribeHead(listener: (frame: ArrayBuffer) => void): () => void;
  /**
   * The person pressed "Fire laser": runs the pending laser request `id` if its
   * countdown is over. The only way the window runs a `laser` action (PLAN §9.5);
   * not an action of the registry, so scripts and agents cannot confirm.
   */
  confirmFire(id: number): Promise<unknown>;
  /**
   * The document's drawable geometry, raw (format "FKTP" with `power` = layer
   * index and `command` = object index; doc/features/canvas.md).
   */
  documentGeometry(): Promise<ArrayBuffer>;
}
