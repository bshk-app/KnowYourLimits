# Codex spike P1a

## Цель

Доказать read-only получение подписочной квоты через официальный Codex и подготовить
изолированный reader. Панель и browser preview P0 **остаются демо**. Это ещё не live монитор.
[Контракт](specs/codex-spike.md).

## Что подтверждено

- Локальный `codex-cli 0.162.0` успешно ответил на `account/rateLimits/read` после
  `initialize` / `initialized`. Ни thread, ни turn, ни inference-запрос не отправлялись.
- Реальный ответ содержал только `primary` с длительностью **10080 минут (неделя)**,
  nullable secondary и map с canonical `codex`. Поэтому primary нельзя называть «5 часов».
- Reset действительно Unix **seconds**, нормализация делает checked seconds → milliseconds.
- `ordinaryUsageAllowed` — отдельное backend разрешение, не выводимое из процентов.
- Ответ может содержать `accountId` и другие ненужные account/credits/banner поля:
  они не должны попадать в нормализованный DTO, диагностику или сохранённые fixtures.
- Probe запускал CLI в отдельной process group; после чтения scoped group kill и ожидание
  child exit закрыли stdio. Raw RPC/stderr и account identity не сохранялись.

## Важные ограничения startup

`app-server` **не является side-effect-free**. Без ограничений он может синхронизировать
plugin repositories, auto-upgrade marketplaces, обновлять catalog/cache, обращаться к
сети и выполнять CLI-owned auth refresh. Отсутствие inference не означает отсутствие
фоновой сети и записей в CLI-owned cache.

Per-process overrides в этом spike (не изменение пользовательского config):

- `analytics.enabled=false`;
- `otel.exporter="none"`, `otel.trace_exporter="none"`, `otel.metrics_exporter="none"`;
- `features.plugins=false`, `features.remote_plugin=false`;
- `features.hooks=false`, `features.apps=false`.

В pinned source plugin startup tasks находятся под `plugins_enabled`, derived из
Feature::Plugins; MCP processor constructor только сохраняет поля. Reader не отправляет
MCP/threads/turns requests. Это не обещание отсутствия любых CLI background tasks: остаются
CLI configuration/auth и model/cache startup. Мы **не** читаем config/credentials нашим
кодом и не предполагаем, что `mcp_servers={}` очищает layered config.

`CODEX_HOME` и авторизация наследуются из среды запуска; они принадлежат официальному CLI.
Запуск в BB/terminal и будущий запуск через Finder могут иметь другой PATH/CODEX_HOME.
Нужен доверенный установленный executable, не случайный файл из cwd.

## CLI example

После сборки reader:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --example codex_read -- "$(command -v codex)"
```

Example не запускает GUI и печатает только нормализованный quota DTO либо статическую
error category (safe numeric RPC code допустим). Он не пишет quota snapshots на диск.
Не перенаправляйте его вывод в публичные документы/issue без необходимости.

Reader — один bounded read, не retry/polling service. Deadline на RPC: 15 секунд;
cleanup имеет отдельный bounded allowance до 2 секунд. Ctrl-C / SIGTERM example
отменяют read future; group ownership нужен также для npm launcher + вложенного Rust CLI.
Невозможно обещать graceful cleanup после SIGKILL самого owner или выключения ОС.

## Выбор bucket и честная нормализация

- Map присутствует → только `rateLimitsByLimitId.codex`, без legacy fallback:
  сервер legacy может содержать первый **другой** bucket.
- Map отсутствует/null → legacy допускается только без foreign limitId.
- Null windows → нет окна, не zero usage; отсутствие обеих → unavailable.
- Labels выводятся из настоящей duration. Unknown duration в текущем P0 DTO — sentinel
  `0` с честным label, не измеренное «0-минутное окно».
- Invalid percentages/durations/epochs → ошибка, без clamp и без придуманного остатка.
- `ordinaryUsageAllowed=false` блокирует current counter; null не превращается в true.
- No login/API-key/zero snapshots могут возвращаться как RPC error, а не пустая квота.
  Не извлекать raw error text: одна безопасная category, проверить ChatGPT login в CLI.

## Проверки реализации

- **25 Codex tests + 13 P0 = 38 Rust tests** прошли с `--locked --all-targets`.
  Проверены schema/normalization, pipe/frame limits, timeout, EOF, RPC/privacy,
  process group cleanup и future cancellation. Synthetic subprocess tests требуют
  `/usr/bin/python3`; runtime reader не зависит от Python.
- Исправлен найденный при проверке bug: `ordinaryUsageAllowed` принадлежит **верхнему
  уровню ответа**, не bucket. Authentic fixtures дали 5 RED failures; после исправления
  top-level false, null/missing, conflicting nested и invalid permission tests зелёные.
  Focused independent re-review подтвердил исправление; это не полный proof отсутствия bugs.
- Финальный **Rust example** успешно получил настоящий normalized quota snapshot:
  current, недельное окно, top-level permission сохранено, milliseconds reset.
  Ни raw account fields, ни реальный response fixture в repo не сохранены.
- CLI signal smoke с synthetic child + helper: **SIGINT и SIGTERM**, exit 130,
  stdout пуст, только статическое cancellation сообщение; оба child PID завершены.
- cargo fmt/check, clippy `-D warnings` и locked dependency resolution прошли.
  Обновлённый `Cargo.lock` сохранён (Tokio process/macros/signal dependencies).
- Frontend regression P0: **34 TS + 6 browser tests**, strict TypeScript и Biome прошли.
  Всего **78 automated test cases**, плюс ручной live-read и два CLI signal smoke.
  Никакой live источник в UI этим шагом не включается.

## Official pinned sources

Version tag `rust-v0.162.0`, commit `c1382380de69521303b416720a52f42d51af6248`:

- [Account handler, map selection and auth errors](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/app-server/src/request_processors/account_processor.rs#L1190-L1296)
- [DTO and nullable permission](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/app-server-protocol/src/protocol/v2/account.rs)
- [Core RateLimitWindow, explicit reset seconds](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/protocol/src/protocol.rs)
- [Feature registry](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/features/src/lib.rs)
- [Plugin startup guard](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/core-plugins/src/manager.rs#L2814-L2922)
- [MCP processor construction](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/app-server/src/request_processors/mcp_processor.rs)

App-server experimental: проверенная версия не означает вечный контракт/SLA.
