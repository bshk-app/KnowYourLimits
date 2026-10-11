import { isLanguage, LANGUAGES, localizeDOM, localizeNotice, setLanguage, t, text } from './i18n';
import './style.css';
import trayIcon from '../assets/icons/tray.svg?raw';
import {
  closeSettingsWindow,
  fitSettingsWindow,
  getState,
  isNative,
  isSettingsTab,
  onSettingsTab,
  requestNotificationPermission,
  SETTINGS_TABS,
  type SettingsTab,
  saveSettings,
  setRefreshMinutes,
  subscribeState,
  testNotification,
} from './bridge';
import { element, message } from './dom';
import {
  type AppState,
  defaultSettings,
  type IconStyle,
  type IndicatorSize,
  isProviderId,
  LAYOUTS,
  PROVIDER_IDS,
  PROVIDER_LABELS,
  PROVIDER_SHORT,
  type ProviderId,
  parseThresholds,
  REFRESH_MINUTES,
  refreshLabel,
  type Settings,
  validateSettings,
} from './model';
import { providerSymbol, renderTray } from './tray';

function tabLabel(tab: SettingsTab) {
  return tab === 'menubar' ? t('menuBar') : PROVIDER_LABELS[tab];
}

// NSSegmentedControl from radios: [value, visible content, accessible name when content is a glyph].
function segment(name: string, labelledBy: string, options: [string, string, string?][]) {
  return `<div class="kyl-seg" role="radiogroup" aria-labelledby="${labelledBy}">${options
    .map(
      ([value, content, label]) =>
        `<label><input type="radio" name="${name}" value="${value}"${label ? ` data-i18n-aria="${label}"` : ''}>${content}</label>`,
    )
    .join('')}</div>`;
}

function providerPane(id: ProviderId) {
  return `
  <section id="pane-${id}" class="kyl-pane provider-pane" role="tabpanel" aria-labelledby="tab-${id}">
    <h2 class="kyl-group-title">${text('inMenuBar')}</h2>
    <div class="kyl-group">
      <div class="kyl-pref"><span id="${id}-icon-label">${text('icon')}</span>${segment(
        `${id}-icon`,
        `${id}-icon-label`,
        [
          ['symbol', `<span data-symbol="${id}"></span>`, 'symbol'],
          ['letter', PROVIDER_SHORT[id], 'letter'],
          ['dot', '<i class="dot"></i>', 'dot'],
          ['none', text('none')],
        ],
      )}</div>
      <div class="kyl-pref"><span id="${id}-size-label">${text('size')}</span>${segment(
        `${id}-size`,
        `${id}-size-label`,
        [
          ['small', text('small')],
          ['medium', text('medium')],
          ['large', text('large')],
        ],
      )}</div>
      <div class="kyl-pref"><span id="${id}-color-label">${text('color')}</span><div class="kyl-color">${segment(
        `${id}-color-mode`,
        `${id}-color-label`,
        [
          ['system', text('systemColor')],
          ['custom', text('customColor')],
        ],
      )}<input id="${id}-color" class="kyl-well" type="color" aria-label="${t('providerColor', { provider: PROVIDER_LABELS[id] })}" value="${id === 'claude' ? '#d97757' : '#10a37f'}"></div></div>
    </div>
    <h2 class="kyl-group-title">${text('updates')}</h2>
    <div class="kyl-group">
      <div class="kyl-pref"><label for="${id}-refresh">${text('autoRefresh')}</label><select id="${id}-refresh" class="kyl-select">${REFRESH_MINUTES.map((minutes) => `<option value="${minutes}">${refreshLabel(minutes)}</option>`).join('')}</select></div>
      <p id="${id}-refresh-status" class="kyl-hint" role="status">${text('immediateSave')}</p>
    </div>
    <h2 class="kyl-group-title">${text('notifications')}</h2>
    <div class="kyl-group">
      <label class="kyl-switch"><span>${text('notifyThreshold')}</span><input id="${id}-notifications" type="checkbox" role="switch"></label>
      <label class="kyl-switch"><span>${text('sound')}</span><input id="${id}-sound" type="checkbox" role="switch"></label>
      <label class="kyl-switch"><span>${text('notifyRecovery')}</span><input id="${id}-recovery" type="checkbox" role="switch"></label>
      <div class="kyl-pref"><label for="${id}-thresholds">${text('thresholds')}</label><input id="${id}-thresholds" class="kyl-input" type="text" autocomplete="off" spellcheck="false" aria-describedby="${id}-threshold-help settings-error"></div>
      <p id="${id}-threshold-help" class="kyl-hint">${text('thresholdHelp')}</p>
      <div class="kyl-pref"><span></span><button id="${id}-test-notification" class="kyl-btn" type="button">${text('testNotification')}</button></div>
    </div>
  </section>`;
}

const app = element('#app');
app.innerHTML = `
  <form id="settings-form" class="kyl-win" novalidate aria-labelledby="settings-title">
    <header class="kyl-win-bar" data-tauri-drag-region>
      <h1 id="settings-title" data-tauri-drag-region>${text('menuBar')}</h1>
      <div class="kyl-tabs" role="tablist" data-i18n-aria="settingsSections" data-tauri-drag-region>
        ${SETTINGS_TABS.map((tab) => `<button id="tab-${tab}" type="button" role="tab" aria-controls="pane-${tab}"><span data-tab-label>${tabLabel(tab)}</span></button>`).join('')}
      </div>
    </header>
    <fieldset id="settings-fields" class="kyl-win-body" disabled>
      <legend class="sr-only">${text('settings')}</legend>
      <section id="pane-menubar" class="kyl-pane" role="tabpanel" aria-labelledby="tab-menubar">
        <div class="kyl-group"><div class="kyl-pref"><label for="language">${text('language')}</label><select id="language" class="kyl-select"><option value="system" data-i18n="systemLanguage"></option>${Object.entries(
          LANGUAGES,
        )
          .map(([code, name]) => `<option value="${code}">${name}</option>`)
          .join('')}</select></div></div>
        <h2 id="layout-title" class="kyl-group-title">${text('appearance')}</h2>
        <div class="kyl-layouts" role="radiogroup" aria-labelledby="layout-title">
          ${LAYOUTS.map((layout) => `<label class="kyl-layout"><input type="radio" name="layout" value="${layout}"><span class="layout-sample" data-layout="${layout}" aria-hidden="true"></span>${text(layout)}</label>`).join('')}
        </div>
        <h2 class="kyl-group-title">${text('counters')}</h2>
        <div class="kyl-group">
          <div class="kyl-pref"><label for="first-provider">${text('firstCounter')}</label><select id="first-provider" class="kyl-select" aria-describedby="settings-error"></select></div>
          <div class="kyl-pref"><label for="second-provider">${text('secondCounter')}</label><select id="second-provider" class="kyl-select" aria-describedby="settings-error"></select></div>
        </div>
      </section>
      ${PROVIDER_IDS.map(providerPane).join('')}
    </fieldset>
    <footer class="kyl-win-foot">
      <div>
        <p id="bridge-notice" class="kyl-hint error" role="status" aria-live="polite" hidden></p>
        <p id="settings-error" class="kyl-hint error" role="alert" hidden></p>
        <p id="settings-result" class="kyl-hint" role="status" hidden></p>
      </div>
      <div class="kyl-actions"><button id="cancel-settings" class="kyl-btn" type="button">${text('cancel')}</button><button id="save-settings" class="kyl-btn primary" type="submit">${text('save')}</button></div>
    </footer>
  </form>
`;
localizeDOM();
document.body.classList.add(isNative ? 'native' : 'preview');

function trayGlyph(): SVGElement {
  const template = document.createElement('template');
  template.innerHTML = trayIcon;
  const svg = template.content.querySelector('svg');
  if (!svg) throw new Error(t('unknownError'));
  svg.setAttribute('aria-hidden', 'true');
  return svg;
}
for (const tab of SETTINGS_TABS)
  element(`#tab-${tab}`).prepend(tab === 'menubar' ? trayGlyph() : providerSymbol(tab));
for (const slot of document.querySelectorAll<HTMLElement>('[data-symbol]'))
  if (isProviderId(slot.dataset.symbol)) slot.replaceWith(providerSymbol(slot.dataset.symbol));

const form = element<HTMLFormElement>('#settings-form');
const fields = element<HTMLFieldSetElement>('#settings-fields');
const notice = element('#bridge-notice');
const settingsError = element('#settings-error');
const settingsResult = element('#settings-result');
const saveButton = element<HTMLButtonElement>('#save-settings');
const cancelButton = element<HTMLButtonElement>('#cancel-settings');
const firstProvider = element<HTMLSelectElement>('#first-provider');
const secondProvider = element<HTMLSelectElement>('#second-provider');
const thresholdInputs = {
  claude: element<HTMLInputElement>('#claude-thresholds'),
  codex: element<HTMLInputElement>('#codex-thresholds'),
};

secondProvider.add(new Option(t('hideCounter'), 'none'));
for (const id of PROVIDER_IDS) {
  firstProvider.add(new Option(PROVIDER_LABELS[id], id));
  secondProvider.add(new Option(PROVIDER_LABELS[id], id));
}

let state: AppState | null = null;
let busy = false;
let lockFields = false;
let focusPending = false;
let commandError: string | null = null;
let subscriptionNotice: string | null = null;
interface Draft {
  first: string;
  second: string;
  claude: string;
  codex: string;
  options: Settings;
}
let draft: Draft | null = null;

function radioValue(name: string): string | undefined {
  return form.querySelector<HTMLInputElement>(`input[name="${name}"]:checked`)?.value;
}
function setRadio(name: string, value: string) {
  for (const radio of form.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`))
    radio.checked = radio.value === value;
}

// The window follows the content height (tab, messages), like macOS settings panes.
// A failed resize only leaves the previous size; the content still scrolls.
new ResizeObserver(() => {
  fitSettingsWindow(form.offsetHeight).catch(() => {});
}).observe(form);

function closeWindow() {
  closeSettingsWindow().catch((error) => {
    commandError = `${t('errorClose')} ${message(error)}`;
    renderNotice();
  });
}

function selectTab(tab: SettingsTab) {
  for (const candidate of SETTINGS_TABS) {
    element(`#pane-${candidate}`).hidden = candidate !== tab;
    const button = element<HTMLButtonElement>(`#tab-${candidate}`);
    button.setAttribute('aria-selected', String(candidate === tab));
    button.tabIndex = candidate === tab ? 0 : -1;
  }
  element('#settings-title').textContent = tabLabel(tab);
  document.title = t('settingsTitle', { tab: tabLabel(tab) });
  // Keep the hash on the shown tab, so a later request for another tab is a real hashchange.
  history.replaceState(null, '', `#${tab}`);
}

function renderNotice() {
  notice.textContent = [
    subscriptionNotice ? localizeNotice(subscriptionNotice) : null,
    commandError,
  ]
    .filter(Boolean)
    .join(' ');
  notice.hidden = !notice.textContent;
}

function renderControls() {
  app.setAttribute('aria-busy', String(busy || !state));
  fields.disabled = !state || (busy && lockFields);
  saveButton.disabled = busy || !state;
  cancelButton.disabled = busy && lockFields;
  for (const id of PROVIDER_IDS) {
    element<HTMLButtonElement>(`#${id}-test-notification`).disabled = !isNative || busy;
    element<HTMLSelectElement>(`#${id}-refresh`).disabled = busy;
  }
}

function renderTrayPreviews() {
  if (!state) return;
  const settings = structuredClone(draft?.options ?? state.settings);
  if (
    draft &&
    isProviderId(draft.first) &&
    (draft.second === 'none' || isProviderId(draft.second)) &&
    draft.first !== draft.second
  )
    settings.barProviders = draft.second === 'none' ? [draft.first] : [draft.first, draft.second];
  const preview = { ...state, settings };
  for (const sample of document.querySelectorAll<HTMLElement>('.layout-sample')) {
    const layout = LAYOUTS.find((candidate) => candidate === sample.dataset.layout);
    if (layout) renderTray(sample, preview, layout);
  }
}

function renderPreferenceControls() {
  for (const id of PROVIDER_IDS) {
    const enabled = element<HTMLInputElement>(`#${id}-notifications`).checked;
    element<HTMLInputElement>(`#${id}-color`).disabled =
      radioValue(`${id}-color-mode`) !== 'custom';
    element<HTMLInputElement>(`#${id}-notifications`).disabled = !isNative;
    for (const field of ['sound', 'recovery'])
      element<HTMLInputElement>(`#${id}-${field}`).disabled = !isNative || !enabled;
  }
}

function clearValidation() {
  settingsError.hidden = true;
  settingsError.textContent = '';
  settingsResult.hidden = true;
  for (const input of [firstProvider, secondProvider, ...Object.values(thresholdInputs)])
    input.removeAttribute('aria-invalid');
}

function resetDraft() {
  if (!state) return;
  draft = {
    first: state.settings.barProviders[0] ?? 'claude',
    second: state.settings.barProviders[1] ?? 'none',
    claude: state.settings.thresholds.claude.join(', '),
    codex: state.settings.thresholds.codex.join(', '),
    options: structuredClone(state.settings),
  };
  element<HTMLSelectElement>('#language').value = draft.options.language;
  firstProvider.value = draft.first;
  secondProvider.value = draft.second;
  for (const id of PROVIDER_IDS) thresholdInputs[id].value = draft[id];
  setRadio('layout', draft.options.layout);
  for (const id of PROVIDER_IDS) {
    const p = draft.options.preferences[id];
    setRadio(`${id}-icon`, p.icon);
    setRadio(`${id}-size`, p.size);
    setRadio(`${id}-color-mode`, p.color === 'system' ? 'system' : 'custom');
    if (p.color !== 'system') element<HTMLInputElement>(`#${id}-color`).value = p.color;
    element<HTMLSelectElement>(`#${id}-refresh`).value = String(p.refreshMinutes);
    element<HTMLInputElement>(`#${id}-notifications`).checked = p.notifications.enabled;
    element<HTMLInputElement>(`#${id}-sound`).checked = p.notifications.sound;
    element<HTMLInputElement>(`#${id}-recovery`).checked = p.notifications.recovery;
  }
  renderPreferenceControls();
  renderTrayPreviews();
  clearValidation();
}

function applyLanguage() {
  if (!state) return;
  setLanguage(state.settings.language === 'system' ? state.systemLocale : state.settings.language);
  localizeDOM();
  const label = element('#tab-menubar').querySelector('span[data-tab-label]');
  if (label) label.textContent = t('menuBar');
  const hiddenOption = secondProvider.options[0];
  if (hiddenOption) hiddenOption.textContent = t('hideCounter');
  for (const id of PROVIDER_IDS) {
    element(`#${id}-color`).setAttribute(
      'aria-label',
      t('providerColor', { provider: PROVIDER_LABELS[id] }),
    );
    for (const option of element<HTMLSelectElement>(`#${id}-refresh`).options)
      option.textContent = refreshLabel(Number(option.value));
  }
  const tab = location.hash.slice(1);
  selectTab(isSettingsTab(tab) ? tab : 'menubar');
}
function applyState(next: AppState) {
  if (state && next.revision <= state.revision) return;
  state = next;
  applyLanguage();
  // An open window keeps the user's draft across backend updates.
  if (!draft) resetDraft();
  renderTrayPreviews();
  renderNotice();
  renderControls();
}

async function command(
  label: string,
  operation: () => Promise<AppState> | Promise<void>,
  success?: () => void,
  lock = false,
): Promise<boolean> {
  if (busy) return false;
  // A passive focus read must not erase a failed mutation's notice.
  if (operation !== getState || !state) commandError = null;
  busy = true;
  lockFields = lock;
  renderNotice();
  renderControls();
  try {
    const next = await operation();
    if (next) applyState(next);
    success?.();
    return true;
  } catch (error) {
    commandError = `${label}: ${message(error)}`;
    return false;
  } finally {
    busy = false;
    lockFields = false;
    renderNotice();
    renderControls();
    if (focusPending) {
      focusPending = false;
      void command(t('errorLoad'), getState);
    }
  }
}

function readDraft(): Settings {
  if (!draft) throw new Error(t('settingsNotLoaded'));
  if (!isProviderId(draft.first) || (draft.second !== 'none' && !isProviderId(draft.second)))
    throw new Error(t('invalidProviders'));
  if (draft.first === draft.second) {
    firstProvider.setAttribute('aria-invalid', 'true');
    secondProvider.setAttribute('aria-invalid', 'true');
    throw new Error(t('duplicateCounters'));
  }
  const thresholds: Settings['thresholds'] = { claude: [], codex: [] };
  for (const id of PROVIDER_IDS) {
    try {
      thresholds[id] = parseThresholds(draft[id]);
    } catch (error) {
      thresholdInputs[id].setAttribute('aria-invalid', 'true');
      throw new Error(`${PROVIDER_LABELS[id]}: ${message(error)}`);
    }
  }
  return validateSettings({
    ...draft.options,
    barProviders: draft.second === 'none' ? [draft.first] : [draft.first, draft.second],
    thresholds,
  });
}

for (const control of [firstProvider, secondProvider, ...Object.values(thresholdInputs)]) {
  control.addEventListener('input', () => {
    draft = {
      first: firstProvider.value,
      second: secondProvider.value,
      claude: thresholdInputs.claude.value,
      codex: thresholdInputs.codex.value,
      options: draft?.options ?? structuredClone(state?.settings ?? defaultSettings()),
    };
    renderTrayPreviews();
    clearValidation();
  });
}

function updatePreferencesDraft() {
  if (!draft) return;
  const options = structuredClone(draft.options);
  const layout = LAYOUTS.find((candidate) => candidate === radioValue('layout'));
  if (layout) options.layout = layout;
  for (const id of PROVIDER_IDS) {
    options.preferences[id] = {
      icon: radioValue(`${id}-icon`) as IconStyle,
      size: radioValue(`${id}-size`) as IndicatorSize,
      color:
        radioValue(`${id}-color-mode`) === 'custom'
          ? element<HTMLInputElement>(`#${id}-color`).value
          : 'system',
      refreshMinutes: Number(element<HTMLSelectElement>(`#${id}-refresh`).value),
      notifications: {
        enabled: element<HTMLInputElement>(`#${id}-notifications`).checked,
        sound: element<HTMLInputElement>(`#${id}-sound`).checked,
        recovery: element<HTMLInputElement>(`#${id}-recovery`).checked,
      },
    };
  }
  draft.options = validateSettings(options);
  renderPreferenceControls();
  renderTrayPreviews();
  clearValidation();
}
for (const control of form.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
  'input[name="layout"], .provider-pane input:not([type="text"]), .provider-pane select',
))
  control.addEventListener('input', updatePreferencesDraft);

element('#language').addEventListener('change', () => {
  const value = element<HTMLSelectElement>('#language').value;
  if (draft && isLanguage(value)) draft.options.language = value;
  clearValidation();
});

for (const id of PROVIDER_IDS) {
  // The interval is applied at once, outside the Save/Cancel draft.
  const refreshControl = element<HTMLSelectElement>(`#${id}-refresh`);
  refreshControl.addEventListener('change', async () => {
    if (!state) return;
    const feedback = element(`#${id}-refresh-status`);
    feedback.dataset.i18n = 'saving';
    feedback.textContent = t('saving');
    const saved = await command(t('errorInterval'), () =>
      setRefreshMinutes(id, Number(refreshControl.value)),
    );
    if (!state) return;
    const actual = state.settings.preferences[id].refreshMinutes;
    refreshControl.value = String(actual);
    if (draft) draft.options.preferences[id].refreshMinutes = actual;
    feedback.dataset.i18n = saved ? 'saved' : 'intervalFailed';
    feedback.textContent = t(saved ? 'saved' : 'intervalFailed');
  });
  element(`#${id}-test-notification`).addEventListener('click', () => {
    void command(
      t('errorNotification'),
      () => testNotification(id, element<HTMLInputElement>(`#${id}-sound`).checked),
      () => {
        settingsResult.textContent = t('notificationSent');
        settingsResult.hidden = false;
      },
    );
  });
}

for (const tab of SETTINGS_TABS) {
  const button = element(`#tab-${tab}`);
  button.addEventListener('click', () => selectTab(tab));
  button.addEventListener('keydown', (event) => {
    const index = SETTINGS_TABS.indexOf(tab);
    const last = SETTINGS_TABS.length - 1;
    const target =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? last
          : event.key === 'ArrowLeft'
            ? (index + last) % SETTINGS_TABS.length
            : event.key === 'ArrowRight'
              ? (index + 1) % SETTINGS_TABS.length
              : null;
    const next = target === null ? undefined : SETTINGS_TABS[target];
    if (!next) return;
    event.preventDefault();
    selectTab(next);
    element(`#tab-${next}`).focus();
  });
}

cancelButton.addEventListener('click', () => {
  resetDraft();
  closeWindow();
});
form.addEventListener('submit', (event) => {
  event.preventDefault();
  if (busy) return;
  clearValidation();
  let settings: Settings;
  try {
    settings = readDraft();
  } catch (error) {
    settingsError.textContent = message(error);
    settingsError.hidden = false;
    return;
  }
  void command(
    t('errorSave'),
    async () => {
      if (
        PROVIDER_IDS.some(
          (id) =>
            settings.preferences[id].notifications.enabled &&
            !state?.settings.preferences[id].notifications.enabled,
        )
      ) {
        if (!(await requestNotificationPermission())) throw new Error(t('allowNotifications'));
      }
      return saveSettings(settings);
    },
    () => {
      resetDraft();
      settingsResult.textContent = t('settingsSaved');
      settingsResult.hidden = false;
    },
    true,
  );
});
window.addEventListener('keydown', (event) => {
  if (event.metaKey && event.key.toLowerCase() === 'w') {
    event.preventDefault();
    closeWindow();
  }
});
window.addEventListener('focus', () => {
  if (busy) focusPending = true;
  else void command(t('errorLoad'), getState);
});

const hash = location.hash.slice(1);
selectTab(isSettingsTab(hash) ? hash : 'menubar');
onSettingsTab(selectTab);
subscribeState(applyState, (text) => {
  subscriptionNotice = text;
  renderNotice();
});
renderControls();
void command(t('errorSettings'), getState);
