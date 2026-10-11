# P1c: проверка автономного источника Claude

**Обновление 0.2.1:** выяснено, что пустой machine report также скрывает HTTP429
с seeded fallback. Прежняя гипотеза о traffic flag не закрывала эту причину.
Reader теперь классифицирует 429 и сохраняет server retry deadline;
пустые проценты не восстанавливаются из текста.
[Текущее исправление](release-0.2.1.md). Далее — исторический отчёт spike.

Дата: 10 октября 2026 года. **Standalone reader реализован и проверен;
пригодность источника для native LIVE пока не подтверждена.**

В CLI 2.1.296 есть локальный неинтерактивный `/usage` со структурированным
`usage_report`. Поэтому PTY для этого spike не понадобился. Первый probe получил
global session/weekly окна с процентами и RFC3339 reset; итоговый CLI result
сообщил 0 turns, 0 API duration, 0 стоимости и 0 токенов. Это наблюдение CLI и
проверка его локального dispatch, а не независимый аудит всех сетевых запросов.

## Реализовано

- `src-tauri/src/claude.rs`: отдельный Unix reader, не подключённый к GUI/IPC.
  Выдаёт только существующий `ProviderSnapshot` либо статическую категорию ошибки.
- Trusted absolute executable → canonical target → версия → один `/usage`.
  Поддерживается только **2.1.296**, без fallback на другой CLI, PTY или inference.
- Per-process `--safe-mode`, пустые tools/MCP, no Chrome, no session persistence,
  zero-turn limit, hooks/connectors/auto-continue restrictions и отключение
  nonessential traffic/telemetry/error reporting/updates/background tasks/memory.
  `--bare` не подходит: он исключает subscription OAuth. `--disable-slash-commands`
  тоже не подходит: pinned implementation убирает и локальные built-in commands.
- Ограничены stdout/stderr, JSON frame/count и общий 15s deadline двух запусков.
  На всех исходах owned process group убивается **до reaping**, затем reader
  ожидает подтверждённый exit. При временной OS wait error сохраняет владение.
  Cancellation нужно await, не abort; SIGKILL/power loss не дают такой гарантии.
- Parser принимает только local usage result с подтверждёнными нулевыми
  model counters. Account identity, session details, activity, costs, extra usage,
  scoped-model windows и raw diagnostics не входят в DTO.
- Глобальные `session`/`weekly_all` нормализуются отдельно; `is_active=false`
  не исключает валидное окно. Invalid/duplicate окна → error; отсутствующие →
  unavailable без процентов/timestamp. RFC3339 разбирает уже присутствовавший
  в lockfile Chrono 0.4.45, теперь явная Unix dependency.
- Пример `claude_read` требует `--live <trusted-absolute-path>` и завершает
  managed cancellation перед выходом по SIGINT/SIGTERM.

## Найденные ограничения источника

**Нельзя считать успешное завершение `/usage` подтверждением актуальной квоты.**
В последующих реальных запусках CLI показывал проценты в текстовой части, но
`usage_report.rate_limits.limits` не содержал окон. Rust reader правильно вернул:

```json
{"id":"claude","label":"Claude","status":"unavailable","fetchedAt":null,"windows":[]}
```

Причина отсутствия `limits` в конкретном ответе окончательно не установлена.
Диагностический запуск показал одну попытку CLI-owned quota GET и нулевые
model counters; он не доказал источник/свежесть процентов из текстовой части.
Текст не используется как запасной источник. Новая авторизация, собственный
OAuth client или изменение пользовательского CLI/config ради этого не выполнялись.

В pinned embedded implementation подтверждены два важных пути:

1. `R`/`bXn` могут вернуть сохранённый utilization моложе **60 секунд**, не
   обращаясь к endpoint; машинный report не содержит observation timestamp.
   Поэтому `fetchedAt` reader — консервативная нижняя граница: start минус 60s,
   а не обещание точного времени измерения. При имеющихся 5 мин TTL это оставляет
   максимум около 4 мин текущего показателя после чтения.
2. `Ghn` убирает generic `limits` из `seeded` fallback, сохраняя legacy поля для
   текстового отображения. `v` формирует machine twin только из `limits` и
   `extra_usage`. Отсутствие generic окон нельзя восстанавливать из legacy text
   без доказательства свежести. Начальный положительный probe также мог попасть
   в разрешённый короткий cache; он сам по себе не доказывает cold autonomous read.

Из-за этого **Claude не включён в native LIVE и polling не добавлен**. Нужен
подтверждённый cold-read путь с observation time/статусом источника либо исправленный
официальный machine report. PTY-текст сам по себе этот freshness gap не решает.
Codex manual integration продолжает работать по прежнему контракту.

## Startup и границы доверия

Официальный `--safe-mode` сохраняет CLI-owned auth и отключает пользовательские
customizations. Но **admin-managed settings/policy hooks продолжают применяться**.
В `system/init` могут перечисляться metadata установленных plugins/skills даже
при safe mode; наличие/отсутствие списка не доказывает исполнение. В probes tools
и MCP были пусты. Код приложения не открывает credentials/config/cookies;
официальный CLI сам может читать их, обновлять auth/cache и анализировать activity.
Этот запуск не является side-effect-free и не является публичным API SLA.

Проверенный локальный native binary: 2.1.296,
SHA-256 `c9b5341637becbd423ddffc5b254afb645682a3868cb708bbc6cc0e7bb419937`.
Hash фиксирует проверенный образец, не удостоверяет происхождение установки.
Embedded dispatch: local `/usage` → `shouldQuery=false`; structured report —
отдельное sibling поле synthetic assistant message. Код binary не копировался
в проект; реальные raw RPC/debug/identity/credential fixtures не сохранялись.

## Проверки

- **20 Claude tests**, включая schema/privacy/freshness, местное служебное
  `ui_invalidate`, compatibility gate, exact isolated launch, executable symlink
  retarget, budgets, EOF, timeout, cancellation до старта/в обоих subprocess stages
  и подтверждённое завершение child/helper. Все fixture данные искусственные.
- Normalization: сначала 4 RED failures, затем GREEN. Первый Rust live run выявил
  служебное `system/ui_invalidate` перед init; добавлен targeted regression и
  обработка в пределах stream budgets. Следующий run честно вернул unavailable.
- Полный locked suite: **77 Rust library + 2 example, 42 TS, 13 Playwright = 134**.
  TypeScript, Biome, production frontend build, rustfmt и all-target Clippy проходят.
- Настоящий CLI не вызывается ни одним default test. Live smoke запускался
  явно; его результат выше. Browser native tests остаются synthetic IPC mocks,
  физическую native QA они не заменяют.
- Собранный `claude_read` проверен с synthetic child/helper и реальными SIGINT,
  SIGTERM: оба запуска завершились через awaited cancellation с exit 130,
  stdout пуст, сообщение статическое, child и helper завершены. Это проверка
  standalone signal path, не native GUI и не сетевого источника.

## Запуск standalone example

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --example claude_read -- --live "$HOME/.local/bin/claude"
```

Используйте реальный доверенный абсолютный путь своей установки. Другие версии
reader отвергает. Официальный login выполняется пользователем в самом CLI;
example не начинает login и не сохраняет snapshots. `--help` не запускает CLI.

## Первичные источники

- [CLI reference: safe mode, bare, settings, tools, MCP, persistence](https://code.claude.com/docs/en/cli-reference)
- [Built-in `/usage`](https://code.claude.com/docs/en/commands#built-in-commands)
- [Hooks: per-process restrictions и managed hooks](https://code.claude.com/docs/en/hooks#disable-or-remove-hooks)
- [Environment variables: nonessential traffic, telemetry и updates](https://code.claude.com/docs/en/env-vars)
- [Chrono RFC3339 parsing](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#method.parse_from_rfc3339)

Детали machine twin/cache/dispatch выше — проверка локального pinned executable,
а не обещание стабильности этих недокументированных полей.
[Контракт реализации](specs/claude-spike.md).
