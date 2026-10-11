/* eslint-disable svelte/prefer-svelte-reactivity -- sets here are local lookups, not state */
// UI text through Fluent (`ui/i18n/<locale>/*.ftl`). Components call `t(id, args)`;
// the locale is reactive, so text updates when it changes. Missing messages fall
// back to English, then to the id (never an empty string).

import { FluentBundle, FluentResource, type FluentVariable } from '@fluent/bundle';

const sources = import.meta.glob('../../../i18n/*/*.ftl', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** A BCP 47 locale with a folder in `ui/i18n`. */
export type Locale = string;

const FALLBACK: Locale = 'en';

/** Every locale with a translation: `en` first, then by tag. */
export const LOCALES: Locale[] = [...new Set(Object.keys(sources).map((p) => p.split('/').at(-2)!))].sort(
  (a, b) => (a === FALLBACK ? -1 : b === FALLBACK ? 1 : a.localeCompare(b)),
);

function bundle(locale: Locale): FluentBundle {
  const b = new FluentBundle(locale, { useIsolating: false });
  // the hand-written freekerf.ftl wins over lasergrbl.ftl (resx2ftl skips its ids anyway)
  const files = Object.entries(sources)
    .filter(([p]) => p.split('/').at(-2) === locale)
    .sort(([a], [b]) =>
      a.endsWith('/freekerf.ftl') ? -1 : b.endsWith('/freekerf.ftl') ? 1 : a.localeCompare(b),
    );
  for (const [, text] of files) {
    const errors = b.addResource(new FluentResource(text));
    for (const e of errors) console.warn(`i18n ${locale}: ${String(e)}`);
  }
  return b;
}

const bundles = new Map<Locale, FluentBundle>(LOCALES.map((l) => [l, bundle(l)]));

/** The best supported locale for a BCP 47 tag (`pt-PT` → `pt-BR`, `de-AT` → `de`). */
export function matchLocale(tag: string | undefined): Locale {
  if (!tag) return FALLBACK;
  const exact = LOCALES.find((l) => l.toLowerCase() === tag.toLowerCase());
  if (exact) return exact;
  const lang = tag.split('-')[0]!.toLowerCase();
  return LOCALES.find((l) => l.split('-')[0]!.toLowerCase() === lang) ?? FALLBACK;
}

const current = $state({
  locale: matchLocale(typeof navigator === 'undefined' ? undefined : navigator.language),
});

/** The current locale. */
export function locale(): Locale {
  return current.locale;
}

/** Changes the locale (from the settings; `auto` = the system's). */
export function setLocale(l: Locale): void {
  if (l === 'auto') l = matchLocale(typeof navigator === 'undefined' ? undefined : navigator.language);
  current.locale = LOCALES.includes(l) ? l : matchLocale(l);
  if (typeof document !== 'undefined') document.documentElement.lang = l;
}

/** Message ids of a locale (tests: completeness). */
export function messageIds(l: Locale): string[] {
  const ids: string[] = [];
  for (const [path, text] of Object.entries(sources)) {
    if (path.split('/').at(-2) !== l) continue;
    for (const m of text.matchAll(/^([a-zA-Z][\w-]*)\s*=/gm)) ids.push(m[1]!);
  }
  return ids;
}

/** The text of message `id` in the current locale. */
export function t(id: string, args?: Record<string, FluentVariable>): string {
  for (const l of [current.locale, FALLBACK]) {
    const b = bundles.get(l);
    const m = b?.getMessage(id);
    if (b && m?.value) return b.formatPattern(m.value, args);
  }
  return id;
}

/** Share of the English messages a locale translates, 0..1 (language picker). */
export function completeness(l: Locale): number {
  const en = new Set(messageIds(FALLBACK));
  if (!en.size) return 0;
  const ids = new Set(messageIds(l));
  return [...en].filter((id) => ids.has(id)).length / en.size;
}

/** The name of a locale in that language (`Deutsch`, `português (Brasil)`). */
export function localeName(l: Locale): string {
  try {
    return new Intl.DisplayNames([l], { type: 'language' }).of(l) ?? l;
  } catch {
    return l;
  }
}
