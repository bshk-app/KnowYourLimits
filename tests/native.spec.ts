import { expect, type Page, test } from '@playwright/test';
import { LANGUAGES, type Locale } from '../src/i18n';
import { catalogs } from '../src/locales';
import { type AppState, disconnectedState, type ProviderId, type Settings } from '../src/model';
import { sampleState } from './fixtures';

// The existing native scenarios exercise macOS layouts on every test host.
test.use({
  userAgent:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36',
});

declare global {
  interface Window {
    nativeMock: {
      emit: (state: AppState) => void;
      emitTab: (tab: string) => void;
      calls: {
        command: string;
        event?: string;
        provider?: ProviderId;
        tab?: string;
        height?: number;
      }[];
      hold: (command: string) => void;
      fail: (command: string) => void;
      release: (command: string) => void;
    };
  }
}
function codexState(revision = 2): AppState {
  const s = sampleState();
  s.connected = ['codex'];
  s.providers[0] = {
    id: 'claude',
    label: 'Claude',
    status: 'unavailable',
    fetchedAt: null,
    windows: [],
  };
  s.revision = revision;
  return s;
}
// Opens the panel (index.html) or the settings window (settings.html#tab) with a mocked backend.
async function mockNative(
  page: Page,
  initial = disconnectedState(),
  failListen = false,
  path = 'index.html',
) {
  // The mocked native locale is independent of the test runner's host language.
  initial = { ...initial, systemLocale: 'en' };
  await page.addInitScript(
    ({ initial, failListen }) => {
      let state = initial;
      const callbacks = new Map<number, (event: unknown) => void>();
      const handlers = new Map<string, number>();
      let callbackId = 0;
      const calls: {
        command: string;
        event?: string;
        provider?: ProviderId;
        tab?: string;
        height?: number;
      }[] = [];
      const held = new Set<string>();
      const failed = new Set<string>();
      const pending = new Map<string, () => void>();
      const dispatch = (event: string, payload: unknown) => {
        const handler = handlers.get(event);
        if (handler !== undefined) callbacks.get(handler)?.({ event, id: 1, payload });
      };
      Object.defineProperty(window, 'isTauri', { value: true });
      Object.assign(window, {
        nativeMock: {
          calls,
          hold: (command: string) => held.add(command),
          fail: (command: string) => failed.add(command),
          release: (command: string) => {
            pending.get(command)?.();
            pending.delete(command);
            held.delete(command);
          },
          emit: (next: AppState) => {
            state = structuredClone(next);
            dispatch('state-changed', next);
          },
          emitTab: (tab: string) => dispatch('settings-tab', tab),
        },
        __TAURI_INTERNALS__: {
          metadata: {
            currentWindow: { label: 'settings' },
            currentWebview: { windowLabel: 'settings', label: 'settings' },
          },
          transformCallback: (callback: (event: unknown) => void) => {
            callbacks.set(++callbackId, callback);
            return callbackId;
          },
          invoke: async (
            command: string,
            args: {
              event?: string;
              handler?: number;
              settings?: Settings;
              provider?: ProviderId;
              minutes?: number;
              tab?: string;
              height?: number;
            } = {},
          ) => {
            calls.push({
              command,
              event: args.event,
              provider: args.provider,
              tab: args.tab,
              height: args.height,
            });
            if (failed.has(command)) throw new Error('SYNTHETIC_PRIVATE');
            if (command === 'request_notification_permission') return true;
            if (['test_notification', 'open_settings', 'fit_panel'].includes(command)) return;
            if (command.startsWith('plugin:window|')) return;
            if (command === 'plugin:event|listen') {
              if (failListen) throw new Error('/private/auth/token');
              if (args.event && args.handler !== undefined) handlers.set(args.event, args.handler);
              return 1;
            }
            if (command === 'read_quota' || command === 'sign_in_claude') {
              const id = command === 'sign_in_claude' ? 'claude' : args.provider;
              if (!id) throw new Error('missing provider');
              state = {
                ...state,
                connected: [...new Set([...state.connected, id])],
                refreshing: id,
                signingIn: command === 'sign_in_claude',
                notice: null,
                revision: state.revision + 1,
                providers: state.providers.map((p) =>
                  p.id === id
                    ? { ...p, status: p.fetchedAt === null ? 'unavailable' : 'stale' }
                    : p,
                ),
              };
            } else if (command === 'cancel_read')
              state = {
                ...state,
                refreshing: null,
                signingIn: false,
                revision: state.revision + 1,
              };
            else if (command === 'save_settings' && args.settings)
              state = { ...state, settings: args.settings, revision: state.revision + 1 };
            else if (
              command === 'set_refresh_minutes' &&
              args.provider &&
              args.minutes !== undefined
            ) {
              state = structuredClone(state);
              state.settings.preferences[args.provider].refreshMinutes = args.minutes;
              state.revision += 1;
            }
            const response = structuredClone(state);
            if (held.has(command))
              await new Promise<void>((resolve) => pending.set(command, resolve));
            return response;
          },
        },
      });
    },
    { initial, failListen },
  );
  await page.goto(path);
  if (path.startsWith('settings.html'))
    await expect(page.locator('#settings-fields')).toBeEnabled();
  else await expect(page.locator('#loading')).toBeHidden();
}
function count(page: Page, command: string) {
  return page.evaluate(
    (name) => window.nativeMock.calls.filter((c) => c.command === name).length,
    command,
  );
}

test('Claude can be connected alongside Codex and there are no demo actions', async ({ page }) => {
  await mockNative(page, codexState());
  await expect(page.getByRole('button', { name: 'Connect Claude', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: /демо/i })).toHaveCount(0);
  await expect(page.locator('body')).not.toContainText(/демо/i);
  await page.getByRole('button', { name: 'Connect Claude', exact: true }).click();
  expect(
    await page.evaluate(() =>
      window.nativeMock.calls.some((c) => c.command === 'read_quota' && c.provider === 'claude'),
    ),
  ).toBe(true);
  await expect(page.locator('#tray-preview')).toHaveText('C — · X 38%');
  const complete = sampleState();
  complete.revision = 4;
  await page.evaluate((s) => window.nativeMock.emit(s), complete);
  await expect(page.locator('#tray-preview')).toHaveText('C 26% · X 38%');
  await expect(page.getByRole('button', { name: 'Update Claude', exact: true })).toBeEnabled();
});
test('startup has two real connections and empty counters', async ({ page }) => {
  await mockNative(page);
  await expect(page.locator('#tray-preview')).toHaveText('C — · X —');
  await expect(page.locator('.kyl-bar')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Connect Claude', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Connect Codex', exact: true })).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.nativeMock.calls
        .filter(
          (c) =>
            c.command === 'get_state' ||
            (c.command === 'plugin:event|listen' && c.event === 'state-changed'),
        )
        .slice(0, 2)
        .map((c) => c.command),
    ),
  ).toEqual(['plugin:event|listen', 'get_state']);
});
test('last percentages remain visible as the manual snapshot ages and after reopening', async ({
  page,
}) => {
  await page.clock.install();
  const initial = sampleState();
  await mockNative(page, initial);
  await page.clock.fastForward(900_000);
  await expect(page.locator('#tray-preview')).toHaveText('C ~26% · X ~38%');
  const codex = page.locator('.kyl-card').last();
  await expect(codex.locator('.kyl-left b')).toHaveText('38%');
  await expect(codex.locator('.kyl-left span')).toHaveText('Previous value');
  await expect(codex.locator('.kyl-card-foot')).toContainText('15 minutes ago');
  expect(await count(page, 'read_quota')).toBe(0);
  await page.reload();
  await expect(page.locator('#tray-preview')).toHaveText('C ~26% · X ~38%');
  await expect(page.locator('#connect-codex')).toHaveAccessibleName('Update Codex');
});
test('signed out Claude offers official browser login with cancellation and then quota', async ({
  page,
}) => {
  const initial = codexState();
  initial.connected.push('claude');
  initial.providers[0].status = 'signedout';
  initial.notice = 'Войдите в Claude Code, затем обновите квоту.';
  await mockNative(page, initial);
  await page.getByRole('button', { name: 'Sign in to Claude', exact: true }).click();
  await expect(page.locator('#subtitle')).toHaveText('Claude and Codex usage');
  await expect(page.getByRole('button', { name: 'Cancel sign-in', exact: true })).toBeEnabled();
  await expect(page.locator('#tray-preview')).toHaveText('C — · X 38%');
  expect(
    await page.evaluate(() => window.nativeMock.calls.some((c) => c.command === 'sign_in_claude')),
  ).toBe(true);
  const done = sampleState();
  done.revision = 4;
  await page.evaluate((s) => window.nativeMock.emit(s), done);
  await expect(page.locator('#tray-preview')).toHaveText('C 26% · X 38%');
  await expect(page.locator('#bridge-notice')).toBeHidden();
});
test('refresh keeps the target history and the same button cancels it', async ({ page }) => {
  await mockNative(page, sampleState());
  await page.locator('#connect-codex').click();
  await expect(page.locator('#connect-codex')).toHaveText('Cancel update');
  await expect(page.locator('#tray-preview')).toHaveText('C 26% · X ~38%');
  await expect(page.locator('.kyl-card').last().locator('.tone')).toHaveText(['', '']);
  await expect(page.locator('#connect-claude')).toBeDisabled();
  await page.locator('#connect-codex').click();
  await expect(page.locator('#connect-codex')).toHaveText('Update');
  expect(await count(page, 'read_quota')).toBe(1);
  expect(await count(page, 'cancel_read')).toBe(1);
});
test('cancel and delayed command or event cannot restore an older state', async ({ page }) => {
  await mockNative(page, codexState());
  await page.evaluate(() => window.nativeMock.hold('read_quota'));
  await page.locator('#connect-claude').click();
  await expect.poll(() => count(page, 'read_quota')).toBe(1);
  const reading = codexState(3);
  reading.connected.push('claude');
  reading.refreshing = 'claude';
  await page.evaluate((s) => window.nativeMock.emit(s), reading);
  await page.getByRole('button', { name: 'Cancel update', exact: true }).click();
  await expect(page.locator('#connect-claude')).toHaveText('Update');
  await page.evaluate(() => window.nativeMock.release('read_quota'));
  const late = sampleState();
  late.revision = 3;
  await page.evaluate((s) => window.nativeMock.emit(s), late);
  await expect(page.locator('#tray-preview')).toHaveText('C — · X 38%');
  await expect(page.locator('.kyl-card').first().locator('.kyl-window')).toHaveCount(0);
});
test('errors absent quotas and expired resets never invent current values', async ({ page }) => {
  await mockNative(page, sampleState());
  const error = sampleState('error');
  error.revision = 2;
  await page.evaluate((s) => window.nativeMock.emit(s), error);
  await expect(page.locator('#tray-preview')).toHaveText('C ~26% · X ~38%');
  await expect(page.locator('.kyl-bar').first()).toHaveAttribute('aria-label', /Previous value/);
  await expect(page.locator('.kyl-card').first().locator('.kyl-badge')).toHaveText('Update failed');
  const absent = sampleState('unavailable');
  absent.revision = 3;
  await page.evaluate((s) => window.nativeMock.emit(s), absent);
  await expect(page.locator('.kyl-window')).toHaveCount(0);
  await expect(page.locator('#tray-preview')).toHaveText('C — · X —');
  const expired = sampleState();
  expired.revision = 4;
  for (const p of expired.providers) for (const w of p.windows) w.resetsAt = Date.now() - 1000;
  await page.evaluate((s) => window.nativeMock.emit(s), expired);
  await expect(page.locator('#tray-preview')).toHaveText('C ~26% · X ~38%');
});
test('listener failures show static notice without raw diagnostics', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await mockNative(page, disconnectedState(), true);
  await expect(page.locator('#bridge-notice')).toContainText('Live updates unavailable');
  await expect(page.locator('#bridge-notice')).not.toContainText('/private');
  expect(errors).toEqual([]);
});
test('threshold crossings color the card and mark the quota bar', async ({ page }) => {
  const state = sampleState();
  state.providers[1].windows[0].usedPercent = 84;
  await mockNative(page, state);
  const codex = page.locator('.kyl-card').last();
  await expect(codex.locator('.kyl-badge')).toHaveText('Usage warning');
  await expect(codex.locator('.kyl-left')).toHaveClass(/warn/);
  await expect(codex.locator('.kyl-window').first()).toHaveClass(/warning/);
  await expect(codex.locator('.tone').first()).toHaveText('Usage warning');
  await expect(codex.locator('.kyl-bar').first().locator('s')).toHaveCount(2);
});
test('an expired window does not color the card next to the live remaining value', async ({
  page,
}) => {
  const state = sampleState();
  state.providers[0].windows[0].usedPercent = 100;
  state.providers[0].windows[0].resetsAt = Date.now() - 60_000;
  await mockNative(page, state);
  const claude = page.locator('.kyl-card').first();
  await expect(claude.locator('.kyl-left b')).toHaveText('82%');
  await expect(claude.locator('.kyl-badge')).toHaveText('Up to date');
  await expect(claude.locator('.kyl-left')).not.toHaveClass(/crit/);
  await expect(claude.locator('.kyl-window').first()).toHaveClass(/historical/);
});
for (const used of [42, 100]) {
  test(`blocked Codex usage keeps fresh ${used}% quota without a sign-in hint`, async ({
    page,
  }) => {
    const state = sampleState();
    state.providers[1].status = 'blocked';
    state.providers[1].windows = [{ ...state.providers[1].windows[1], usedPercent: used }];
    await mockNative(page, state);
    const codex = page.locator('.kyl-card').last();
    await expect(codex.locator('.kyl-badge')).toHaveText(
      used === 100 ? 'Limit reached' : 'Usage blocked',
    );
    await expect(codex.locator('.kyl-left b')).toHaveText(`${100 - used}%`);
    await expect(codex.locator('.kyl-left span')).toHaveText('remaining');
    await expect(codex.locator('.kyl-window')).not.toHaveClass(/historical/);
    await expect(page.locator('#tray-preview')).toHaveText(`C 26% · X ${100 - used}%`);
    await expect(page.locator('body')).not.toContainText('Sign in to Codex');
  });
}
test('cards distinguish previous values from current quota beside the update age', async ({
  page,
}) => {
  const state = sampleState('stale');
  state.settings.preferences.claude.refreshMinutes = 2;
  state.settings.preferences.codex.refreshMinutes = 15;
  await mockNative(page, state);
  await expect(page.locator('.kyl-card-foot').first()).toContainText('Updated');
  await expect(page.locator('.kyl-left span')).toHaveText(['Previous value', 'Previous value']);
  await expect(page.locator('#panel-footer')).toHaveCount(0);
  await expect(page.locator('#counter-help')).toHaveCount(0);
  await expect(
    page.locator('.kyl-card').first().getByText('Out of date', { exact: true }),
  ).toHaveCount(1);
});
test('gear, menu item and Cmd-comma open the matching settings tab', async ({ page }) => {
  await mockNative(page, sampleState());
  await page.getByRole('button', { name: 'Codex settings', exact: true }).click();
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.keyboard.press('Meta+Comma');
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.nativeMock.calls.filter((c) => c.command === 'open_settings').map((c) => c.tab),
      ),
    )
    .toEqual(['codex', 'menubar', 'menubar']);
});
test('panel window follows its content height instead of a fixed 620 pt', async ({ page }) => {
  await mockNative(page);
  const lastHeight = () =>
    page.evaluate(
      () => window.nativeMock.calls.filter((c) => c.command === 'fit_panel').at(-1)?.height ?? 0,
    );
  await expect.poll(lastHeight).toBeGreaterThan(0);
  const empty = await lastHeight();
  expect(empty).toBeLessThan(620);
  const full = sampleState();
  full.revision = 2;
  await page.evaluate((s) => window.nativeMock.emit(s), full);
  await expect.poll(lastHeight).toBeGreaterThan(empty);
});

test('settings window opens on the requested tab and follows the backend tab event', async ({
  page,
}) => {
  await mockNative(page, sampleState(), false, 'settings.html#codex');
  await expect(page.getByRole('tab', { name: 'Codex', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.locator('#settings-title')).toHaveText('Codex');
  await page.evaluate(() => window.nativeMock.emitTab('claude'));
  await expect(page.locator('#pane-claude')).toBeVisible();
  await expect(page.locator('#pane-codex')).toBeHidden();
  await page.getByRole('tab', { name: 'Claude', exact: true }).press('ArrowLeft');
  await expect(page.getByRole('tab', { name: 'Menu bar', exact: true })).toBeFocused();
  await expect(page.locator('#settings-title')).toHaveText('Menu bar');
  expect(await count(page, 'plugin:window|set_size')).toBeGreaterThan(0);
});
test('settings draft survives backend events and focus reads', async ({ page }) => {
  await page.clock.install();
  await mockNative(page, sampleState(), false, 'settings.html#codex');
  await page.locator('#codex-thresholds').fill('71, 91');
  await page.getByRole('tab', { name: 'Menu bar', exact: true }).click();
  await page.locator('#first-provider').selectOption('codex');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.clock.fastForward(15_000);
  const complete = sampleState();
  complete.revision = 2;
  await page.evaluate((s) => window.nativeMock.emit(s), complete);
  await expect(page.locator('#first-provider')).toHaveValue('codex');
  await page.getByRole('tab', { name: 'Codex', exact: true }).click();
  await expect(page.locator('#codex-thresholds')).toHaveValue('71, 91');
  await page.getByRole('tab', { name: 'Menu bar', exact: true }).click();
  await page.locator('#second-provider').selectOption('none');
  await page.locator('#save-settings').click();
  await expect(page.locator('#settings-result')).toHaveText('Settings saved.');
  await expect(page.locator('.layout-sample').first().locator('.tray-provider')).toHaveText([
    '38%',
  ]);
});
test('provider preferences have independent icons colors sizes notifications and timers', async ({
  page,
}) => {
  await mockNative(page, sampleState(), false, 'settings.html#claude');
  await page.getByRole('radio', { name: 'Dot', exact: true }).check();
  await page.getByRole('radio', { name: 'Large', exact: true }).check();
  await page.getByRole('radio', { name: 'Custom', exact: true }).check();
  await page.locator('#claude-color').fill('#d97757');
  await page.locator('#claude-refresh').selectOption('2');
  await expect(page.locator('#claude-refresh-status')).toHaveText('Saved.');
  await page.locator('#claude-notifications').check();
  await page.locator('#claude-sound').check();
  await page.locator('#claude-recovery').check();
  await page.getByRole('tab', { name: 'Codex', exact: true }).click();
  await expect(page.locator('#codex-notifications')).not.toBeChecked();
  await expect(page.getByRole('radio', { name: 'Medium', exact: true })).toBeChecked();
  await page.getByRole('radio', { name: 'Letter', exact: true }).check();
  await page.locator('#codex-refresh').selectOption('15');
  await page.getByRole('tab', { name: 'Menu bar', exact: true }).click();
  await page.getByRole('radio', { name: 'Rings', exact: true }).check();
  await page.locator('#save-settings').click();
  await expect(page.locator('#settings-result')).toHaveText('Settings saved.');
  expect(await count(page, 'request_notification_permission')).toBe(1);
  await expect(page.getByRole('radio', { name: 'Rings', exact: true })).toBeChecked();
  await page.getByRole('tab', { name: 'Claude', exact: true }).click();
  await expect(page.locator('#claude-notifications')).toBeChecked();
  await expect(page.getByRole('radio', { name: 'Dot', exact: true })).toBeChecked();
  await expect(page.locator('#claude-refresh')).toHaveValue('2');
  await expect(page.locator('#claude-color')).toHaveValue('#d97757');
  await page.locator('#claude-test-notification').click();
  await expect(page.locator('#settings-result')).toHaveText('Notification sent.');
  expect(
    await page.evaluate(() =>
      window.nativeMock.calls.some(
        (c) => c.command === 'test_notification' && c.provider === 'claude',
      ),
    ),
  ).toBe(true);
});
test('choosing 15 minutes applies the timer immediately and survives cancel', async ({ page }) => {
  await mockNative(page, sampleState(), false, 'settings.html#claude');
  await page.locator('#claude-refresh').selectOption('15');
  await expect(page.locator('#claude-refresh-status')).toHaveText('Saved.');
  expect(
    await page.evaluate(() =>
      window.nativeMock.calls.some(
        (c) => c.command === 'set_refresh_minutes' && c.provider === 'claude',
      ),
    ),
  ).toBe(true);
  await page.locator('#cancel-settings').click();
  await expect.poll(() => count(page, 'plugin:window|close')).toBe(1);
  await expect(page.locator('#claude-refresh')).toHaveValue('15');
});
test('applying a timer keeps notification and appearance drafts uncommitted', async ({ page }) => {
  await mockNative(page, sampleState(), false, 'settings.html#claude');
  await page.getByRole('radio', { name: 'Large', exact: true }).check();
  await page.locator('#claude-notifications').check();
  await page.locator('#claude-refresh').selectOption('15');
  await expect(page.locator('#claude-refresh-status')).toHaveText('Saved.');
  expect(await count(page, 'request_notification_permission')).toBe(0);
  expect(await count(page, 'save_settings')).toBe(0);
  await page.locator('#cancel-settings').click();
  await expect(page.locator('#claude-refresh')).toHaveValue('15');
  await expect(page.getByRole('radio', { name: 'Medium', exact: true })).toBeChecked();
  await expect(page.locator('#claude-notifications')).not.toBeChecked();
});
test('failed immediate timer save restores the active value and reports failure', async ({
  page,
}) => {
  await mockNative(page, sampleState(), false, 'settings.html#codex');
  await page.evaluate(() => window.nativeMock.fail('set_refresh_minutes'));
  await page.locator('#codex-refresh').selectOption('15');
  await expect(page.locator('#codex-refresh')).toHaveValue('0');
  await expect(page.locator('#codex-refresh-status')).toContainText('Could not save');
  await expect(page.locator('#bridge-notice')).not.toContainText('SYNTHETIC_PRIVATE');
});
test('every layout uses actual quota and previews preserve provider drafts during events', async ({
  page,
}) => {
  await mockNative(page, sampleState(), false, 'settings.html#menubar');
  for (const label of ['One row', 'Two rows', 'Rings']) {
    await page.getByRole('radio', { name: label, exact: true }).check();
    await expect(page.locator('.layout-sample')).toHaveCount(3);
    await expect(page.locator('.layout-sample').first()).toContainText('26%');
  }
  await page.getByRole('tab', { name: 'Claude', exact: true }).click();
  await page.locator('#claude-refresh').selectOption('5');
  await page.getByRole('radio', { name: 'Small', exact: true }).check();
  const event = sampleState();
  event.revision = 2;
  await page.evaluate((s) => window.nativeMock.emit(s), event);
  await expect(page.locator('#claude-refresh')).toHaveValue('5');
  await expect(page.getByRole('radio', { name: 'Small', exact: true })).toBeChecked();
  await page.locator('#cancel-settings').click();
  await expect(page.locator('input[name="layout"][value="stacked"]')).toBeChecked();
});

test('Claude rate limit shows countdown, preserves history, and does not block Codex', async ({
  page,
}) => {
  await page.clock.install();
  const state = sampleState();
  state.providers[0].status = 'error';
  state.retryAt.claude = Date.now() + 30000;
  state.notice = 'Claude ограничил частоту запросов. Обновление временно приостановлено.';
  await mockNative(page, state);
  await expect(page.locator('#connect-claude')).toBeDisabled();
  await expect(page.locator('#connect-claude')).toContainText('Retry in');
  await expect(page.locator('#connect-codex')).toBeEnabled();
  await expect(page.locator('.kyl-card').first()).toContainText('Paused');
  await expect(page.locator('.kyl-card').first().locator('.used').first()).toHaveText('74% used');
  await expect(page.locator('#tray-preview')).toHaveText('C ~26% · X 38%');
  await expect(page.locator('#bridge-notice')).not.toContainText('подписк');
  await page.locator('#connect-claude').dispatchEvent('click');
  expect(await count(page, 'read_quota')).toBe(0);
  await page.clock.fastForward(31000);
  await expect(page.locator('#connect-claude')).toBeEnabled();
  await expect(page.locator('#connect-claude')).toHaveAccessibleName('Update Claude');
});

test('every locale updates open quota cards, accessible labels and notices without overflow', async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await page.emulateMedia({ colorScheme: 'dark' });
  const state = sampleState('stale');
  state.providers[1].status = 'blocked';
  state.providers[1].fetchedAt = Date.now();
  state.notice = 'kyl:cancelled';
  await mockNative(page, state);
  for (const locale of Object.keys(LANGUAGES) as Locale[]) {
    state.settings.language = locale;
    state.revision += 1;
    await page.evaluate((next) => window.nativeMock.emit(next), state);
    await expect(page.locator('html')).toHaveAttribute('lang', locale);
    await expect(page.locator('#open-settings')).toContainText(catalogs[locale].settings);
    await expect(page.locator('.kyl-window b').nth(1)).toHaveText(catalogs[locale].week);
    await expect(page.locator('#bridge-notice')).toHaveText(catalogs[locale].cancelled);
    await expect(page.locator('.kyl-card').first().locator('.kyl-icon-btn')).toHaveAccessibleName(
      catalogs[locale].settingsProvider.replace('{provider}', 'Claude'),
    );
    const overflow = await page.locator('.kyl-panel *').evaluateAll((elements) =>
      elements
        .filter((el) => el.getClientRects().length && !el.closest('[hidden]'))
        .filter((el) => el.getBoundingClientRect().right > 361)
        .map((el) => `${el.tagName}.${el.className}: ${el.textContent}`),
    );
    expect(overflow, locale).toEqual([]);
  }
  state.settings.language = 'ru';
  state.revision += 1;
  await page.evaluate((next) => window.nativeMock.emit(next), state);
  await expect(page.locator('html')).toHaveAttribute('lang', 'ru');
  await page.screenshot({ path: 'artifacts/localization-panel-ru.png' });
});

test('all settings translations fit and changing language preserves unsaved fields', async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 1000 });
  const state = sampleState();
  await mockNative(page, state, false, 'settings.html#claude');
  await page.locator('#claude-thresholds').fill('70, 90');
  for (const locale of Object.keys(LANGUAGES) as Locale[]) {
    state.settings.language = locale;
    state.revision += 1;
    await page.evaluate((next) => window.nativeMock.emit(next), state);
    await expect(page.locator('html')).toHaveAttribute('lang', locale);
    await expect(page.locator('#claude-thresholds')).toHaveValue('70, 90');
    for (const tab of ['menubar', 'claude', 'codex']) {
      await page.locator(`#tab-${tab}`).click();
      const overflow = await page.locator('.kyl-win *').evaluateAll((elements) =>
        elements
          .filter((el) => el.getClientRects().length && !el.closest('[hidden]'))
          .filter((el) => el.getBoundingClientRect().right > 421)
          .map((el) => `${el.tagName}.${el.className}: ${el.textContent}`),
      );
      expect(overflow, `${locale} ${tab}`).toEqual([]);
    }
  }
  await page.locator('#tab-menubar').click();
  await page.locator('#language').selectOption('fi');
  await page.locator('#save-settings').click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'fi');
  await expect(page.locator('#settings-result')).toHaveText(catalogs.fi.settingsSaved);
  await page.screenshot({ path: 'artifacts/localization-settings-fi.png' });
});

test.describe('Windows provider icons', () => {
  test.use({
    userAgent:
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36',
  });

  test('hides macOS layouts and still saves provider selection and appearance', async ({
    page,
  }) => {
    await mockNative(page, sampleState(), false, 'settings.html#menubar');
    await expect(page.locator('#layout-title')).toBeHidden();
    await expect(page.locator('.kyl-layouts')).toBeHidden();
    await page.locator('#second-provider').selectOption('none');
    await page.getByRole('tab', { name: 'Claude', exact: true }).click();
    await page.getByRole('radio', { name: 'Letter', exact: true }).check();
    await page.getByRole('radio', { name: 'Large', exact: true }).check();
    await page.locator('#save-settings').click();
    await expect(page.locator('#settings-result')).toHaveText('Settings saved.');
    expect(await count(page, 'save_settings')).toBe(1);
    await page.getByRole('tab', { name: 'Menu bar', exact: true }).click();
    await expect(page.locator('#second-provider')).toHaveValue('none');
  });
});
