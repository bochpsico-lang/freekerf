// UI text through Fluent (`ui/i18n/<locale>/*.ftl`). Components call `t(id, args)`;
// the locale is reactive, so text updates when it changes. Missing messages fall
// back to English, then to the id (never an empty string).

import { FluentBundle, FluentResource, type FluentVariable } from '@fluent/bundle';

const sources = import.meta.glob('../../../i18n/*/*.ftl', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** Locales with a translation, in the order of the language picker. */
export const LOCALES = ['en', 'pt-BR', 'de'] as const;
export type Locale = (typeof LOCALES)[number];

const FALLBACK: Locale = 'en';

function bundle(locale: Locale): FluentBundle {
  const b = new FluentBundle(locale, { useIsolating: false });
  for (const [path, text] of Object.entries(sources)) {
    if (path.split('/').at(-2) !== locale) continue;
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

/** Changes the locale (from the settings). */
export function setLocale(l: Locale): void {
  current.locale = l;
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
