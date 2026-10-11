import { localizeNotice, t } from './i18n';
export function element<T extends HTMLElement>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`Missing element ${selector}`);
  return found;
}

export function node<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text = '') {
  const result = document.createElement(tag);
  result.className = className;
  result.textContent = text;
  return result;
}

export function message(error: unknown): string {
  return error instanceof Error
    ? localizeNotice(error.message)
    : typeof error === 'string'
      ? localizeNotice(error)
      : t('unknownError');
}
