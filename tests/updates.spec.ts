import { expect, type Page, test } from '@playwright/test';
import { disconnectedState } from '../src/model';
import type { UpdateState } from '../src/update-bridge';

declare global {
  interface Window {
    updaterMock: { calls: { command: string; confirmed?: boolean }[] };
  }
}
async function updater(
  page: Page,
  status: UpdateState['status'],
  installation: UpdateState['installation'] = 'managed',
) {
  await page.addInitScript(
    ({ quota, status, installation }) => {
      let updates = {
        revision: 1,
        status,
        supported: true,
        installation,
        version: '0.2.8',
        downloaded: 0,
        total: null as number | null,
        notice: null as string | null,
      };
      const calls: { command: string; confirmed?: boolean }[] = [];
      const callbacks = new Map<number, (event: unknown) => void>();
      const handlers = new Map<string, number>();
      let id = 0;
      const emit = () => {
        const handler = handlers.get('updates-changed');
        if (handler !== undefined)
          callbacks.get(handler)?.({ event: 'updates-changed', id: 1, payload: updates });
      };
      Object.defineProperty(window, 'isTauri', { value: true });
      Object.assign(window, {
        updaterMock: { calls },
        __TAURI_INTERNALS__: {
          metadata: {
            currentWindow: { label: 'main' },
            currentWebview: { windowLabel: 'main', label: 'main' },
          },
          transformCallback: (callback: (event: unknown) => void) => {
            callbacks.set(++id, callback);
            return id;
          },
          invoke: async (command: string, args: Record<string, unknown> = {}) => {
            calls.push({
              command,
              confirmed: typeof args.confirmed === 'boolean' ? args.confirmed : undefined,
            });
            if (command === 'plugin:event|listen') {
              if (typeof args.event === 'string' && typeof args.handler === 'number')
                handlers.set(args.event, args.handler);
              return 1;
            }
            if (command === 'get_state') return quota;
            if (command === 'get_update_state') return updates;
            if (command === 'check_for_updates') return updates;
            if (command === 'download_update')
              updates = {
                ...updates,
                revision: updates.revision + 1,
                status: 'downloading',
                downloaded: 20,
                total: 100,
              };
            if (command === 'cancel_update_download')
              updates = {
                ...updates,
                revision: updates.revision + 1,
                status: 'available',
                notice: 'downloadCancelled',
              };
            if (command === 'install_update')
              updates = { ...updates, revision: updates.revision + 1, status: 'installing' };
            if (['download_update', 'cancel_update_download', 'install_update'].includes(command)) {
              emit();
              return updates;
            }
            return null;
          },
        },
      });
    },
    { quota: { ...disconnectedState(), systemLocale: 'en' }, status, installation },
  );
  await page.goto('/index.html');
}

test('installation requires explicit confirmation and uses only the app-owned Rust command', async ({
  page,
}) => {
  await updater(page, 'ready');
  await page.getByRole('button', { name: 'Install and restart', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Install and restart' })).toBeVisible();
  expect(
    await page.evaluate(() => window.updaterMock.calls.some((c) => c.command === 'install_update')),
  ).toBe(false);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Install and restart' })).toBeHidden();
  await page.getByRole('button', { name: 'Install and restart', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm install and restart', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Installing update' })).toBeVisible();
  const calls = await page.evaluate(() => window.updaterMock.calls);
  expect(calls.filter((c) => c.command === 'install_update')).toEqual([
    { command: 'install_update', confirmed: true },
  ]);
  expect(
    calls.some(
      (c) => c.command.startsWith('plugin:updater') || c.command.startsWith('plugin:process'),
    ),
  ).toBe(false);
});

test('downloading is cancellable and does not disable provider connection controls', async ({
  page,
}) => {
  await updater(page, 'available');
  await page.getByRole('button', { name: 'Download update', exact: true }).click();
  await expect(page.getByRole('progressbar')).toHaveAttribute('value', '20');
  await expect(page.getByRole('button', { name: 'Connect Codex', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Cancel download', exact: true }).click();
  await expect(page.getByRole('progressbar')).toBeHidden();
  await expect(
    page.getByRole('status').filter({ hasText: 'Update download cancelled' }),
  ).toBeVisible();
});

test('portable/manual copy offers a truthful manual path without installation controls', async ({
  page,
}) => {
  await updater(page, 'available', 'manual');
  await expect(
    page.getByRole('status').filter({ hasText: 'Replace portable Windows copies manually' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download update', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Install and restart', exact: true })).toHaveCount(
    0,
  );
  await expect(page.getByRole('button', { name: 'Check for updates', exact: true })).toBeEnabled();
});
