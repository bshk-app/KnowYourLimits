# P1b: проверки live Codex

## Подтверждено автоматическими проверками

- 42 TS tests; strict TypeScript и Biome прошли.
- 13 Playwright tests: 7 browser-preview + 6 synthetic native IPC/event tests.
- 57 Rust library + 1 example opt-in test; `--locked --all-targets`, fmt/check и
  clippy `-D warnings` прошли. **Всего 113 автоматических test cases**.
- P0 settings/order/freshness/expired-reset semantics сохранены; browser остаётся
  только демо. Native mock проверяет connect/weekly label/Claude disconnected,
  draft isolation, refresh/back-to-demo и late revision rejection.
- Fake stdio/coordinator tests проверяют единственный job, history timestamps,
  settings merge, cancellation, mode/shutdown races и child/helper termination.

## Реальное чтение

Opt-in example `live_read --live` использует **тот же production coordinator и
resolver**, что IPC/tray. Запуск с `PATH=/usr/bin:/bin:/usr/sbin:/sbin` прошёл:
`mode=live`, `refreshing=false`, Codex current с недельным окном, Claude empty и
без timestamp, notice отсутствует. Проверенный CLI — Homebrew `0.162.0`.
Не сохранялись реальный raw RPC/stderr, identity или credential fixture.

Первый smoke выбрал неработающий дубль из user bin и честно вернул Error/`—`.
Discovery исправлен: известные Homebrew bins имеют приоритет над user bins;
метаданные вместо shell/версий-проб, без fallback/retry чтения другой CLI.
Порядок прошёл RED/GREEN regression. Глобальный PATH/CODEX_HOME не меняются.

## Найдено и исправлено

- Установку GUI signal listener перенесли после `manage(RuntimeState)`, чтобы
  ранний сигнал не вызывал callback до появления managed state.
- Spec-auditor нашёл условный cleanup gap: managed reader мог после 2 секунд
  передать reaping в Drop и завершить job раньше ожидания process exit.
  Managed cleanup теперь **сохраняет владение до подтверждённого wait**, включая
  OS wait errors (backoff, не новые RPC/CLI calls). Standalone reader сохраняет
  прежний bounded cleanup. Regression воспроизвёл RED на 2.01s, затем GREEN.
  При отказе ОС managed cleanup может ждать дольше; force-kill не гарантирует cleanup.

## Native build / UI

- Debug `.app` собрана: `src-tauri/target/debug/bundle/macos/KnowYourLimits.app`,
  **27.27 MiB**. Это не notarized/оптимизированный release.
- Два запуска packaged executable с минимальным PATH: процесс работал 3 секунды,
  Codex child при старте не обнаружен; SIGTERM и SIGINT обработаны через graceful
  exit (коды 143 и 130, не OS signal termination), без panic. Smoke-процессы завершены.
- Это signal smoke **без активного GUI-чтения**; активная отмена/child/helper проверены
  synthetic managed tests. Физические tray clicks, packaged WebView IPC,
  focus/DPI/multi-monitor UX всё ещё требуют ручной QA.
- Focused correctness review не выявил подтверждённых bugs; spec audit после
  managed-cleanup исправления — **GREEN**. Это не proof отсутствия всех ошибок.
- Разрешения macOS для Screen Recording/Accessibility не менялись.
[Ручной чек-лист](live-codex-qa.md).

## Не входит в этап

Polling, Claude reader, уведомления, login UI, автозапуск и произвольный выбор
executable. Live mode/session consent не сохраняются; settings сохраняются.
