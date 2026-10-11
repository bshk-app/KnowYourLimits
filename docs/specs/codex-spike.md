# Codex — read-only spike P1a

## Зачем

Следующий шаг после P0: доказать получение серверной подписочной квоты через официальный
Codex CLI. Это отдельный on-demand reader, не включение live данных в существующий demo
UI. P0 и browser preview продолжают показывать только явно помеченные демоданные.

## Границы

- Поддерживаемая измеряемая версия в этом шаге: установленный `codex-cli 0.162.0`.
- Только stdio `app-server`, без daemon/WebSocket, shell-интерполяции и прямого HTTP API.
- Handshake `initialize` → `initialized` → `account/rateLimits/read`.
- Не создавать threads/turns, не вызывать inference, account/read/login/logout,
  не читать/переписывать credentials, пользовательские config и history нашим кодом.
  Официальный Codex самостоятельно использует свою существующую авторизацию.
- Отдельный нейтральный cwd; telemetry в дочернем CLI выключается per-process overrides.
  Перед реальным запросом проверить startup побочные эффекты по official source.
- Не сохранять raw RPC/stderr, email, account IDs, tokens или реальные quota fixtures.
- Не добавлять polling, notifications, автозапуск, новый authentication UI и Claude adapter.

## Узкий контракт

Reader получает путь к доверенному установленному Codex executable (только Rust/example
caller; не новый frontend-controlled path). Возвращает нормализованный `ProviderSnapshot`
или типизированную безопасную ошибку. CLI example печатает только quota snapshot или
статическую error category; реальный ответ не пишется в репозиторий.

- Проценты: native `usedPercent`, без локального расчёта usage/cap и без clamp.
- Reset: Unix seconds → checked Unix milliseconds.
- Labels определяются реальной `windowDurationMins`, а не положением primary/secondary.
- Legacy `rateLimits` и актуальный `rateLimitsByLimitId`; в bar-квоту попадает только
  подтверждённый общий Codex bucket, не code review / неизвестные model limits.
- Nullable / missing windows и не-ChatGPT quota дают unavailable, не «0% осталось».
- Invalid response и RPC error не выводят raw message/data. Error не означает исчерпание.
- Reader не подменяет expired resets восстановившейся квотой; freshness/counter логика P0
  остаётся источником вычисления минимального остатка.
- `ordinaryUsageAllowed=false` запрещает актуальный bar-counter даже при оставшихся
  процентах. Nullable permission сохраняется отдельно как «неизвестно», не `true`.
- Ненулевая длительность nullable: для P0 DTO неизвестная duration представляется
  sentinel `0` и честным label; это не измеренное нулевое окно. Percentages остаются native.
- Если map присутствует, он авторитетен: при отсутствии `codex` не использовать legacy
  fallback (сервер может положить туда первый чужой bucket).

## Ресурсные ограничения

- Один запрос, без автоматических retries. RPC deadline 15 секунд (включая handshake и writes),
  затем отдельный bounded cleanup allowance до 2 секунд.
- Ограниченные stdout frame/total bytes; stderr читается и отбрасывается, чтобы не заполнить pipe.
- EOF, oversize, protocol error, timeout: scoped kill process group, await child exit.
  При отмене future Drop синхронно kills group и schedules reaping на ещё живом Tokio
  runtime; await в Drop невозможен. Caller обязан сохранять runtime живым для reaping.
  Graceful cleanup после SIGKILL owner/выключения ОС не обещается.
  Не оставлять app-server daemon при нормальных completion/error/cancellation.
  Test harness не запускает реальный CLI по умолчанию.

## Проверка

1. Получить live read без inference; в отчёт вынести только успешность, version и поля квот.
2. Test-first normalization: flat/map, nulls, unknown buckets, duration, Unix units,
   invalid percentages/timestamps, API/no quota, отсутствие raw error leakage.
3. Fake stdio subprocess tests: handshake, unrelated notifications, EOF/malformed,
   blocking/timeout, large stdout/stderr и cleanup child при success/error/cancellation.
4. cargo fmt/test/clippy и regression checks P0; independent correctness review.
5. Зафиксировать фактические ограничения. Live integration в tray/panel — следующий
   отдельный шаг после успешного spike и явного разделения live/demo источников в UI.
