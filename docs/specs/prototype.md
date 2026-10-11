# KnowYourLimits — прототип P0

## Цель и границы

Запускаемый macOS-first Tauri 2 прототип на Rust + vanilla TypeScript, опирающийся на
[исследование](../../claudedocs/research_limits_tauri2_2026-10-10.md). Это проверка UX,
не рабочий автономный монитор. Все проценты вымышлены и явно помечены «Демо».
Реальные CLI, OAuth, credentials, inference, уведомления и автозапуск НЕ подключаются.
Будущий автономный Claude adapter остаётся отдельным техническим spike.

## Поведение

- Одна template-иконка в строке меню; слева/справа от неё системный title с `C`/`X`.
- Старт с закрытой панелью, без Dock icon. Левый клик открывает панель под иконкой;
  повторный клик оставляет её открытой. Потеря фокуса / Escape / закрытие скрывают её.
  Правое меню: открыть, обновить демо, выйти.
  **REVISED после ревью:** P0 использует идемпотентное открытие вместо первоначального
  toggle, чтобы не зависеть от порядка macOS blur/mouse-up событий. Это намеренное
  сужение prototype UX; toggle с координацией событий отложен.
- Обе карточки одновременно: отдельные короткое/недельное окна, «использовано», reset countdown.
- Счётчик в меню — округлённый минимум остатка доступных свежих основных окон.
  Только current sample не старше 5 минут; истёкшее окно не становится нулём.
  Stale/error/unavailable и отсутствие подходящих окон дают `—`, не `0%`.
  Исторические числа в панели сохраняются, но явно обозначены как неактуальные.
- Настройки: первый провайдер, второй или «не показывать», без повторов; массив 1–2 ID
  сохраняет порядок. Пороги 1–100%, уникальные, до 5 на провайдера, default 80/95/100.
  В P0 пороги только изменяют визуальное предупреждение, не отправляют уведомления.
- Native settings сохраняются в app config JSON, атомарной заменой. Preview хранит
  только настройки в localStorage; демосценарий не сохраняется. Ошибка чтения/записи видна.
- Демо-переключатель: обычный расход, почти исчерпано, устарело, ошибка, нет данных.
  «Обновить демо» создаёт новый вымышленный snapshot, а не запрашивает аккаунт.
- Preview страницы — средство удалённой демонстрации, не дополнительная веб-версия продукта.

## Модули и контракт

- `src/model.ts`: DTO, свежесть, форматирование, counters, валидация настроек.
- `src/fixtures/demo.json`: общие вымышленные проценты и reset offsets для Rust/TS.
- `src/bridge.ts`: узкие native команды; в браузере локальный demo fallback.
- `src/main.ts`, `src/style.css`: компактный UI без React/router/remote assets.
- `src-tauri/src/model.rs`: native DTO, demo snapshot, settings validation, counter.
- `src-tauri/src/lib.rs`: state, atomic settings storage, menu/tray/window, fixed IPC.

DTO camelCase:

```ts
type ProviderId = 'claude' | 'codex';
type Scenario = 'normal' | 'critical' | 'stale' | 'error' | 'unavailable';
type Settings = {
  barProviders: ProviderId[];
  thresholds: Record<ProviderId, number[]>;
};
type QuotaWindow = {
  id: string; label: string; usedPercent: number;
  resetsAt: number | null; // Unix milliseconds
  durationMins: number; primary: boolean;
};
type ProviderSnapshot = {
  id: ProviderId; label: string;
  status: 'current' | 'stale' | 'error' | 'unavailable';
  fetchedAt: number | null; // Unix milliseconds
  windows: QuotaWindow[];
};
type AppState = {
  settings: Settings; providers: ProviderSnapshot[];
  scenario: Scenario; notice: string | null;
};
```

IPC: `get_state()`, `save_settings({settings})`, `set_demo_scenario({scenario})`,
`refresh_demo()`, `hide_panel()`. State-changing commands return `AppState`; get returns
`AppState`; hide returns void. Ни frontend-controlled paths, ни shell/http/fs plugins.

## Критерии проверки

1. TS/Rust tests: minimum remaining (не среднее), порядок, singleton, missing/stale/error,
   истёкший reset, invalid percentages, settings validation и camelCase parity.
2. TS strict check, Biome check, Vite build, cargo fmt/test/clippy и native build.
3. Проверка preview: переключение сценариев, редактирование/сохранение настроек,
   восстановление после reload, light/dark, отсутствие внешних запросов/ошибок JS.
4. Native smoke: title, скрытая панель, открытие из tray, сохранение настроек,
   packaged `.app`. Непроверенное поведение явно отметить в README.
