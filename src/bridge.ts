import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { isNoticeCode, localizeNotice, t } from './i18n';
import {
  type AppState,
  defaultSettings,
  disconnectedState,
  PROVIDER_IDS,
  type ProviderId,
  REFRESH_MINUTES,
  type Settings,
  validateAppState,
  validateSettings,
} from './model';

export const isNative = isTauri();
const STORAGE_KEY = 'knowyourlimits.settings.v1';
let browserState: AppState | undefined;
let latest: AppState | undefined;
let listening: Promise<void> | undefined;
let bridgeNotice: string | null = null;
const subscribers = new Set<{
  state: (state: AppState) => void;
  notice: (notice: string) => void;
}>();

function publishNotice(text: string) {
  bridgeNotice = text;
  for (const subscriber of subscribers) subscriber.notice(text);
}

function applyIfNewer(payload: unknown): AppState {
  const next = validateAppState(payload);
  if (!latest || next.revision > latest.revision) {
    latest = next;
    for (const subscriber of subscribers) subscriber.state(structuredClone(next));
  }
  return structuredClone(latest);
}

function ensureListening(): Promise<void> {
  if (!isNative) return Promise.resolve();
  listening ??= listen<unknown>('state-changed', (event) => {
    try {
      applyIfNewer(event.payload);
    } catch {
      publishNotice('kyl:invalidState');
    }
  }).then(
    () => {},
    () => {
      publishNotice('kyl:errorSubscription');
    },
  );
  return listening;
}

// Subscription starts before the first state read. Registration errors are handled once,
// without retries; a read-only focus/get_state remains available.
export function subscribeState(
  onState: (state: AppState) => void,
  onNotice: (notice: string) => void,
): () => void {
  const subscriber = { state: onState, notice: onNotice };
  subscribers.add(subscriber);
  if (latest) onState(structuredClone(latest));
  if (bridgeNotice) onNotice(bridgeNotice);
  void ensureListening();
  return () => subscribers.delete(subscriber);
}

async function nativeState(command: string, args?: Record<string, unknown>): Promise<AppState> {
  await ensureListening();
  let payload: unknown;
  try {
    payload = await invoke<unknown>(command, args);
  } catch (error) {
    throw new Error(isNoticeCode(error) ? localizeNotice(error) : t('errorAction'));
  }
  return applyIfNewer(payload);
}

function previewState(): AppState {
  if (browserState) return browserState;
  let settings = defaultSettings();
  let notice: string | null = null;
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved !== null) settings = validateSettings(JSON.parse(saved));
  } catch {
    notice = 'kyl:defaultSettingsNotice';
  }
  browserState = disconnectedState(settings);
  browserState.notice = notice;
  return browserState;
}

export async function getState(): Promise<AppState> {
  return isNative ? nativeState('get_state') : applyIfNewer(previewState());
}

export async function saveSettings(settings: Settings): Promise<AppState> {
  const valid = validateSettings(settings);
  if (isNative) return nativeState('save_settings', { settings: valid });
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(valid));
  } catch {
    throw new Error(t('errorStorage'));
  }
  const state = previewState();
  state.settings = valid;
  state.notice = null;
  state.revision += 1;
  return applyIfNewer(state);
}

export async function setRefreshMinutes(provider: ProviderId, minutes: number): Promise<AppState> {
  if (!REFRESH_MINUTES.some((allowed) => allowed === minutes))
    throw new Error(t('invalidInterval'));
  if (isNative) return nativeState('set_refresh_minutes', { provider, minutes });
  const settings = structuredClone(previewState().settings);
  settings.preferences[provider].refreshMinutes = minutes;
  return saveSettings(settings);
}

export function readQuota(provider: ProviderId): Promise<AppState> {
  if (!isNative) return Promise.reject(new Error(t('appOnly')));
  return nativeState('read_quota', { provider });
}
export function signInClaude(): Promise<AppState> {
  if (!isNative) return Promise.reject(new Error(t('appOnly')));
  return nativeState('sign_in_claude');
}
export function cancelRead(): Promise<AppState> {
  if (!isNative) return Promise.reject(new Error(t('appOnly')));
  return nativeState('cancel_read');
}

export async function requestNotificationPermission(): Promise<boolean> {
  if (!isNative) throw new Error(t('appOnly'));
  try {
    const value = await invoke<unknown>('request_notification_permission');
    if (typeof value === 'boolean') return value;
  } catch {
    throw new Error(t('errorPermission'));
  }
  throw new Error(t('errorPermission'));
}
export async function testNotification(provider: ProviderId, sound: boolean): Promise<void> {
  if (!isNative) throw new Error(t('appOnly'));
  try {
    await invoke('test_notification', { provider, sound });
  } catch {
    throw new Error(t('errorNotification'));
  }
}

export async function hidePanel(): Promise<void> {
  if (!isNative) return;
  try {
    await invoke<unknown>('hide_panel');
  } catch {
    throw new Error(t('errorAction'));
  }
}

export const SETTINGS_TABS = ['menubar', ...PROVIDER_IDS] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];
// Must match the window width in src-tauri/src/lib.rs show_settings.
const SETTINGS_WIDTH = 420;

export function isSettingsTab(value: unknown): value is SettingsTab {
  return SETTINGS_TABS.some((tab) => tab === value);
}

export async function openSettings(tab: SettingsTab): Promise<void> {
  if (!isNative) {
    window.open(`settings.html#${tab}`, 'knowyourlimits-settings', 'popup,width=420,height=600');
    return;
  }
  try {
    await invoke('open_settings', { tab });
  } catch {
    throw new Error(t('errorOpenSettings'));
  }
}

// Reopening an open settings window switches its tab: the backend emits an event,
// the browser preview changes the hash of the same popup.
export function onSettingsTab(handler: (tab: SettingsTab) => void): void {
  const apply = (value: unknown) => {
    if (isSettingsTab(value)) handler(value);
  };
  window.addEventListener('hashchange', () => apply(location.hash.slice(1)));
  if (isNative)
    listen<unknown>('settings-tab', (event) => apply(event.payload)).catch(() => {
      publishNotice('kyl:errorTab');
    });
}

export async function fitPanel(height: number): Promise<void> {
  if (isNative) await invoke('fit_panel', { height });
}

export async function fitSettingsWindow(height: number): Promise<void> {
  if (isNative) await getCurrentWindow().setSize(new LogicalSize(SETTINGS_WIDTH, height));
}

export async function closeSettingsWindow(): Promise<void> {
  if (isNative) await getCurrentWindow().close();
  else window.close();
}
