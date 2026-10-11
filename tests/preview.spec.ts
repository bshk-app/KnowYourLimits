import { expect, test } from '@playwright/test';

test('browser has no quotas or demo controls and directs connections to macOS app', async ({
  page,
}) => {
  await page.goto('index.html');
  await expect(page.locator('#tray-preview')).toHaveText('C — · X —');
  await expect(page.locator('.kyl-bar')).toHaveCount(0);
  await expect(page.locator('body')).not.toContainText(/демо/i);
  await expect(page.locator('#connect-claude')).toBeDisabled();
  await expect(page.locator('#connect-codex')).toBeDisabled();
  await expect(page.locator('#browser-live-hint')).toContainText('macOS');
});
test('settings open in their own window on the requested tab', async ({ page }) => {
  await page.goto('index.html');
  const popup = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Codex settings', exact: true }).click();
  const settings = await popup;
  await expect(settings).toHaveURL(/settings\.html#codex$/);
  await expect(settings.getByRole('tab', { name: 'Codex', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});
test('a gear reselects its tab in an open settings popup after switching tabs there', async ({
  page,
}) => {
  await page.goto('index.html');
  const popup = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Claude settings', exact: true }).click();
  const settings = await popup;
  await settings.getByRole('tab', { name: 'Codex', exact: true }).click();
  await page.getByRole('button', { name: 'Claude settings', exact: true }).click();
  await expect(settings.getByRole('tab', { name: 'Claude', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});
test('settings save persist and cancel restores without changing quota state', async ({ page }) => {
  await page.goto('settings.html#menubar');
  await page.locator('#first-provider').selectOption('codex');
  await page.locator('#second-provider').selectOption('none');
  await page.getByRole('tab', { name: 'Codex', exact: true }).click();
  await page.locator('#codex-thresholds').fill('75, 90');
  await page.locator('#save-settings').click();
  await expect(page.locator('#settings-result')).toHaveText('Settings saved.');
  await page.goto('index.html');
  await expect(page.locator('#tray-preview')).toHaveText('X —');
  await page.goto('settings.html#codex');
  await expect(page.locator('#codex-thresholds')).toHaveValue('75, 90');
  await page.locator('#codex-thresholds').fill('80, 99');
  await page.locator('#cancel-settings').click();
  await expect(page.locator('#codex-thresholds')).toHaveValue('75, 90');
});
test('invalid settings cannot save in the dark settings window', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('settings.html#codex');
  await page.locator('#codex-thresholds').fill('80, 80');
  await page.locator('#save-settings').click();
  await expect(page.locator('#settings-error')).toBeVisible();
  await expect(page.locator('#codex-thresholds')).toHaveAttribute('aria-invalid', 'true');
  await expect(page.locator('#save-settings')).toBeVisible();
});
test('panel fits a narrow dark layout', async ({ page }) => {
  await page.setViewportSize({ width: 300, height: 900 });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('index.html');
  await expect(page.locator('#open-settings')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(300);
});

test('ordinary text cannot be selected while settings inputs remain editable', async ({ page }) => {
  await page.goto('index.html');
  await page.locator('#app-title').dblclick();
  expect(await page.evaluate(() => getSelection()?.toString())).toBe('');
  await page.goto('settings.html#claude');
  await page.locator('#settings-title').dblclick();
  expect(await page.evaluate(() => getSelection()?.toString())).toBe('');
  await page.locator('#claude-thresholds').fill('80, 95');
  await page.locator('#claude-thresholds').press('ControlOrMeta+A');
  await page.locator('#claude-thresholds').pressSequentially('75, 90');
  await expect(page.locator('#claude-thresholds')).toHaveValue('75, 90');
});

test('language selection persists and system mode follows browser language', async ({
  browser,
}) => {
  const context = await browser.newContext({ locale: 'fi-FI' });
  const page = await context.newPage();
  await page.goto('http://127.0.0.1:1420/settings.html');
  await expect(page.locator('html')).toHaveAttribute('lang', 'fi');
  await expect(page.locator('#language option')).toHaveCount(26);
  await page.locator('#language').selectOption('ru');
  await page.locator('#save-settings').click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ru');
  await expect(page.locator('#settings-result')).toHaveText('Настройки сохранены.');
  await page.reload();
  await expect(page.locator('#language')).toHaveValue('ru');
  await page.goto('http://127.0.0.1:1420/index.html');
  await expect(page.locator('#open-settings')).toContainText('Настройки');
  await page.goto('http://127.0.0.1:1420/settings.html');
  await page.locator('#language').selectOption('system');
  await page.locator('#save-settings').click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'fi');
  await context.close();
});
