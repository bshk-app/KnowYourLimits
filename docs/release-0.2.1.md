# 0.2.1 — корректная обработка ограничений Claude

Repro: пользовательская установленная 0.2.0 показывала пустую квоту Claude и
«Проверьте подписку». Тот же production coordinator воспроизвёл пустой snapshot.
Read-only CLI probe показал один quota GET → **HTTP 429**; официальный `/usage`
при этом возвращает result=success с нулевыми model counters и без structured
limits. Старые текстовые проценты в seeded fallback не доказывают свежесть.

Это не проверка наличия подписки. Прежняя гипотеза об umbrella traffic flag
не закрывала обработку HTTP ошибок: успешный CLI result не равен успешному quota GET.
Новые реальные проверки показывают server-provided длительную паузу запросов.

## Исправление

- Pinned CLI quota stage использует `--debug-file /dev/stderr` для узкой диагностики.
  Debug stdout/stderr не записываются приложением и не передаются в IPC. Parser
  извлекает только usage HTTP429 marker и числовой срок паузы, ограниченный часом.
  Строки, URLs, headers, identity, auth материал не сохраняются/не выводятся.
- При пустой квоте и подтверждённом 429 reader возвращает typed rate-limit error.
  Поздняя запись CLI logger ожидается до 250ms в общем 15s deadline; затем owned
  process group убивается до reaping, stderr дренируется и cleanup ожидается.
- Повторы вручную и по таймеру блокируются до server retry deadline.
  При отсутствии срока используется 300s — fallback проверенного CLI 2.1.296.
  Codex продолжает работать отдельно. После успешного нового чтения пауза снимается.
- Последний настоящий снимок и его timestamp не удаляются при пустом ответе.
  Они показываются как история; не становятся текущими и не создают ложный 0%.
- Панель показывает паузу и отсчёт, в меню действие временно отключено.
  Ошибочный совет проверить подписку убран.
  Кнопка разблокируется по времени без необходимости нового события IPC.
- `quota-retry.json` в app-owned config directory хранит только provider deadlines.
  Пауза переживает restart. Запись atomic и mode0600, чтение не исполняет CLI.
  Будущие сроки ограничены часом; старые/невалидные игнорируются.
- Убран дублированный footer «Нет измерения · Нет измерения».

## Проверки

Synthetic subprocess regression сначала RED:
`http_429_is_an_error_even_when_cli_returns_success_with_empty_report`
— 429 молча превращался в успешный пустой snapshot. После исправления GREEN.

Проверяются scope/bounds/privacy диагностики, поздний logger и Retry-After,
reader → production mapper → cooldown, повторные клики/timer без новых CLI,
независимость Codex, сохранение history/timestamp, expiry и restart persistence.
UI mock проверяет countdown, disabled/re-enabled action и доступность Codex.

106 Rust library + 2 examples + 45 TS + 14 Playwright = **167 cases**.
Default tests не делают реальных quota GET. Из production example после
исправления получен typed server rate limit и `retryAt`; server всё ещё возвращал
429. Свежие проценты в этой проверке не получены и не заявляются восстановленными.

Срок последнего подтверждённого ответа: Unix ms `1791663458948`.
При установке этот deadline переносится в app-owned retry metadata, чтобы новая
сборка тоже не запрашивала endpoint раньше разрешённого времени.

CLI контракт/флаги:
[официальный CLI reference](https://code.claude.com/docs/en/cli-reference).
Поведение logger/seeded fallback/retry constants — наблюдение установленного
pinned executable, не публичный SLA других версий.

Подпись, consumer DMG и установленная версия проверяются отдельно после сборки.

## Итог

Подписанный/нотарифицированный DMG: 3 466 261 байт,
SHA-256 `773c5b8780ad0d762fccea98ee2e778a31ddc315721a56e673a289386834045b`.
Consumer DMG/app проверены codesign, stapler и Gatekeeper (accepted).
Версия0.2.1 установлена в /Applications/KnowYourLimits.app; backup
`/Users/akira/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.1-1791661463341.app`.
Срок1791663458948 записан в app-owned quota-retry.json без нового quota GET.
Приложение запущено, PID64062 активен. CUA capture timeout; физическое окно
не подтверждено, UI с mock IPC и настоящий production reader проверены.
Сервер скачивания порт55999, сессия99981; предыдущие ссылки redirect на0.2.1.
167 cases, Clippy-Dwarnings, fmt, TS, Biome, production build пройдены.
