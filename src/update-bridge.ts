import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { isNative } from './bridge';
import strings from './locales/updates.en.json' with { type: 'json' };

// Updater copy deliberately falls back to English until reviewed translations exist.
// Keep it separate from the 25 complete quota/settings locale catalogs.
export const updateText = strings;
export type UpdateNotice = keyof typeof strings.notices;
const statuses = [
  'idle',
  'checking',
  'upToDate',
  'available',
  'downloading',
  'ready',
  'installing',
] as const;
export type UpdateStatus = (typeof statuses)[number];
export interface UpdateState {
  revision: number;
  status: UpdateStatus;
  supported: boolean;
  installation: 'managed' | 'manual' | 'development';
  version: string | null;
  downloaded: number;
  total: number | null;
  notice: UpdateNotice | null;
}
export function isUpdateNotice(value: unknown): value is UpdateNotice {
  return typeof value === 'string' && Object.hasOwn(strings.notices, value);
}
export function validateUpdateState(value: unknown): UpdateState {
  const invalid = () => new Error(strings.notices.requestFailed);
  if (!value || typeof value !== 'object') throw invalid();
  const s = value as Record<string, unknown>;
  if (
    !Number.isSafeInteger(s.revision) ||
    typeof s.revision !== 'number' ||
    s.revision < 0 ||
    !statuses.some((status) => status === s.status) ||
    typeof s.supported !== 'boolean' ||
    !['managed', 'manual', 'development'].includes(String(s.installation)) ||
    !(s.version === null || (typeof s.version === 'string' && s.version.length <= 128)) ||
    !Number.isSafeInteger(s.downloaded) ||
    typeof s.downloaded !== 'number' ||
    s.downloaded < 0 ||
    !(
      s.total === null ||
      (typeof s.total === 'number' && Number.isSafeInteger(s.total) && s.total > 0)
    ) ||
    !(s.notice === null || isUpdateNotice(s.notice))
  )
    throw invalid();
  // Only validated display fields enter the UI; installer bytes/URLs are never accepted.
  return {
    revision: s.revision,
    status: s.status as UpdateStatus,
    supported: s.supported,
    installation: s.installation as UpdateState['installation'],
    version: s.version,
    downloaded: s.downloaded,
    total: s.total,
    notice: s.notice,
  };
}
let latest: UpdateState | undefined;
let listening: Promise<boolean> | undefined;
let connected = false;
let polling = false;
const subscribers = new Set<{
  state: (state: UpdateState) => void;
  notice?: (notice: string) => void;
}>();
function apply(payload: unknown): UpdateState {
  const next = validateUpdateState(payload);
  if (!latest || next.revision > latest.revision) {
    latest = next;
    for (const subscriber of subscribers) subscriber.state(structuredClone(next));
  }
  if (!connected && isInFlight(latest)) startPolling();
  return structuredClone(latest);
}
function isInFlight(state: UpdateState | undefined): boolean {
  return (
    state?.status === 'checking' ||
    state?.status === 'downloading' ||
    state?.status === 'installing'
  );
}
function reportTransportFailure() {
  for (const subscriber of subscribers) subscriber.notice?.(strings.notices.requestFailed);
}
function startPolling() {
  if (polling || subscribers.size === 0) return;
  polling = true;
  void (async () => {
    let failures = 0;
    // A request is bounded by native HTTP timeouts; stop polling after three minutes.
    for (let attempt = 0; attempt < 180 && isInFlight(latest) && subscribers.size > 0; attempt++) {
      await new Promise<void>((resolve) => setTimeout(resolve, 1000));
      try {
        if (attempt % 3 === 0) await ensureListening();
        apply(await invoke<unknown>('get_update_state'));
        failures = 0;
        if (connected) return;
      } catch {
        failures++;
        if (failures >= 3) {
          reportTransportFailure();
          return;
        }
      }
    }
    if (isInFlight(latest) && subscribers.size > 0) reportTransportFailure();
  })().finally(() => {
    polling = false;
  });
}
function ensureListening(): Promise<boolean> {
  listening ??= listen<unknown>('updates-changed', (event) => {
    try {
      apply(event.payload);
    } catch {
      /* Never display malformed native payloads. */
    }
  }).then(
    () => {
      connected = true;
      return true;
    },
    () => {
      connected = false;
      listening = undefined;
      return false;
    },
  );
  return listening;
}
export function subscribeUpdates(
  fn: (state: UpdateState) => void,
  notice?: (notice: string) => void,
): () => void {
  const subscriber = { state: fn, notice };
  subscribers.add(subscriber);
  if (latest) fn(structuredClone(latest));
  if (isNative) void ensureListening();
  return () => subscribers.delete(subscriber);
}
async function updateCommand(
  command: string,
  args?: Record<string, unknown>,
): Promise<UpdateState> {
  if (!isNative) throw new Error(strings.notices.manualInstall);
  await ensureListening();
  let payload: unknown;
  try {
    payload = await invoke<unknown>(command, args);
  } catch (error) {
    throw new Error(isUpdateNotice(error) ? strings.notices[error] : strings.notices.requestFailed);
  }
  return apply(payload);
}
export const getUpdateState = () => updateCommand('get_update_state');
export const checkForUpdates = () => updateCommand('check_for_updates');
export const downloadUpdate = () => updateCommand('download_update');
export const cancelUpdateDownload = () => updateCommand('cancel_update_download');
export const installUpdate = () => updateCommand('install_update', { confirmed: true });
