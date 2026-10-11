// The settings as the UI reads them (`settings.get`); they change only through
// `settings.set` and come back with the `settings_changed` event. The UI never
// reads or writes `settings.toml` (PLAN §14).

import type { Settings } from '../gen/api';
import type { AppService } from '../service';

export class SettingsProjection {
  #settings: Settings | null = $state(null);

  constructor(private readonly service: AppService) {
    service.subscribe((e) => {
      if (e.event === 'settings_changed') void this.refresh();
    });
  }

  get settings(): Settings | null {
    return this.#settings;
  }

  /** Appearance, with the defaults until the settings are read. */
  get ui(): Required<NonNullable<Settings['ui']>> {
    const ui = this.#settings?.ui ?? {};
    return {
      language: ui.language ?? 'auto',
      theme: ui.theme ?? 'dark',
      high_contrast: ui.high_contrast ?? false,
      density: ui.density ?? 'auto',
      units: ui.units ?? 'mm',
    };
  }

  async refresh(): Promise<void> {
    this.#settings = (await this.service.dispatch('settings.get', {})) as Settings;
  }
}
