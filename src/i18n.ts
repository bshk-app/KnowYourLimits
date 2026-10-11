import { catalogs } from './locales';
import en from './locales/en.json' with { type: 'json' };

export type MessageKey = keyof typeof en;
export const LANGUAGES = {
  en: 'English',
  ru: 'Русский',
  bg: 'Български',
  cs: 'Čeština',
  da: 'Dansk',
  de: 'Deutsch',
  el: 'Ελληνικά',
  es: 'Español',
  et: 'Eesti',
  fi: 'Suomi',
  fr: 'Français',
  ga: 'Gaeilge',
  hr: 'Hrvatski',
  hu: 'Magyar',
  it: 'Italiano',
  lt: 'Lietuvių',
  lv: 'Latviešu',
  mt: 'Malti',
  nl: 'Nederlands',
  pl: 'Polski',
  pt: 'Português',
  ro: 'Română',
  sk: 'Slovenčina',
  sl: 'Slovenščina',
  sv: 'Svenska',
} as const;
export type Locale = keyof typeof LANGUAGES;
export type Language = Locale | 'system';
export function isLanguage(value: unknown): value is Language {
  return typeof value === 'string' && (value === 'system' || Object.hasOwn(LANGUAGES, value));
}
export function resolveLocale(
  language: Language,
  preferred: readonly string[] = typeof navigator === 'undefined' ? ['en'] : navigator.languages,
): Locale {
  if (language !== 'system') return language;
  for (const candidate of preferred) {
    const base = candidate.toLowerCase().split(/[-_]/)[0];
    if (base && isLanguage(base) && base !== 'system') return base;
  }
  return 'en';
}
let locale = resolveLocale('system');
export function getLocale(): Locale {
  return locale;
}
export function setLanguage(language: Language): void {
  locale = resolveLocale(language);
  if (typeof document !== 'undefined') document.documentElement.lang = locale;
}
export function t(key: MessageKey, params: Record<string, string | number> = {}): string {
  const template = catalogs[locale]?.[key] || en[key];
  return template.replace(/\{(\w+)\}/g, (match, name: string) => String(params[name] ?? match));
}
// Static template text is localized without replacing controls or losing focus/drafts.
export function text(key: MessageKey): string {
  return `<span data-i18n="${key}"></span>`;
}
export function localizeDOM(root: ParentNode = document): void {
  for (const element of root.querySelectorAll<HTMLElement>('[data-i18n]')) {
    const key = element.dataset.i18n;
    if (key && Object.hasOwn(en, key)) element.textContent = t(key as MessageKey);
  }
  for (const element of root.querySelectorAll<HTMLElement>('[data-i18n-aria]')) {
    const key = element.dataset.i18nAria;
    if (key && Object.hasOwn(en, key)) element.setAttribute('aria-label', t(key as MessageKey));
  }
}
export function percent(value: number): string {
  return new Intl.NumberFormat(locale, { style: 'percent', maximumFractionDigits: 0 }).format(
    value / 100,
  );
}
export function unit(
  value: number,
  name: 'day' | 'hour' | 'minute' | 'second',
  display: 'short' | 'long' = 'short',
): string {
  return new Intl.NumberFormat(locale, { style: 'unit', unit: name, unitDisplay: display }).format(
    value,
  );
}
export function duration(minutes: number): string {
  if (minutes >= 1440) {
    const hours = Math.floor((minutes % 1440) / 60);
    return [unit(Math.floor(minutes / 1440), 'day'), hours ? unit(hours, 'hour') : '']
      .filter(Boolean)
      .join(' ');
  }
  if (minutes >= 60) {
    const rest = minutes % 60;
    return [unit(Math.floor(minutes / 60), 'hour'), rest ? unit(rest, 'minute') : '']
      .filter(Boolean)
      .join(' ');
  }
  return unit(minutes, 'minute');
}
export function windowLabel(window: { durationMins: number; label: string }): string {
  if (window.durationMins === 10080) return t('week');
  if (window.durationMins > 0)
    return window.durationMins % 60 === 0
      ? unit(window.durationMins / 60, 'hour', 'long')
      : unit(window.durationMins, 'minute', 'long');
  return t('usageWindow');
}
// Native state carries message identifiers, so an open notice follows language changes.
export function localizeNotice(value: string): string {
  return value.replace(/kyl:([A-Za-z]+)/g, (_match, key: string) =>
    Object.hasOwn(en, key) ? t(key as MessageKey) : t('unknownError'),
  );
}

export function isNoticeCode(value: unknown): value is string {
  return typeof value === 'string' && value.startsWith('kyl:') && Object.hasOwn(en, value.slice(4));
}
