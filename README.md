# Contracter backend

Репозиторий содержит backend Contracter: PostgreSQL-схему и инварианты, детерминированное экономическое ядро, application-слой, HTTP API на Axum и бинарный сервер. Frontend, внешняя загрузка цен, платежи и Steam-интеграция не входят в этот репозиторий.

Пошаговая инструкция для второго разработчика: [`docs/FRIEND_ONBOARDING_RU.md`](docs/FRIEND_ONBOARDING_RU.md). Готовая версия для отправки другу: [`docs/Инструкция_для_разработчика_Contracter.docx`](docs/Инструкция_для_разработчика_Contracter.docx). Правила для Claude Code находятся в [`CLAUDE.md`](CLAUDE.md).

## Состав проекта

- `crates/economy-core` — точные расчёты контрактов, цен и risk policy без сети и базы данных.
- `crates/db` — SQLx 0.9, PostgreSQL pool, встроенные миграции, health-check, транзакции, типизированные ID и защищённый доступ к ledger.
- `crates/application` — use cases, валидация и правила доступа между HTTP и БД.
- `crates/api` — Axum routes, HTTP-валидация, middleware и безопасные ответы.
- `crates/server` — конфигурация, подключение зависимостей и запуск `contracter-server`.
- `db/migrations` — упорядоченные миграции PostgreSQL.
- `db/seeds` — идемпотентные справочные данные без каталога конкретных скинов.
- `db/tests` — SQL-проверки ограничений, журналов и административных правил.
- `scripts/verify.sh` — единая локальная проверка.

## Переменные окружения и запуск

Скопируйте [.env.example](.env.example) в игнорируемый локальный файл и подставьте значения только локально. Сервер требует `DATABASE_URL`; `HOST`, `PORT` и `LOG_FILTER` имеют безопасные значения по умолчанию. `INSECURE_COOKIES=true` допустим только для локального HTTP: в обычном режиме cookie имеют `Secure` и `__Host-` защиту.

```bash
DATABASE_URL='postgres://USER@HOST:PORT/contracter?sslmode=require' \
cargo run -p server --bin contracter-server
```

`TEST_DATABASE_URL` предназначен только для отдельной тестовой БД. Никогда не направляйте проверки на shared или production БД: integration fixtures намеренно создают записи.

## Работа в Zed

Откройте корень репозитория в Zed. Rust Analyzer использует workspace из `Cargo.toml`; дополнительных настроек редактора не требуется.

Полная офлайн-проверка:

```bash
./scripts/verify.sh
```

Она запускает форматирование, Clippy, все Rust-тесты и проверку shell-скриптов. Без `TEST_DATABASE_URL` PostgreSQL-интеграция явно пропускается. В GitHub Actions этот полный путь выполняется на отдельном PostgreSQL service.

## Проверка на изолированной PostgreSQL

Используйте локальную базу или выделенную тестовую PostgreSQL. Пароль аккаунта провайдера не подходит: нужен пароль PostgreSQL-пользователя с правами на тестовую БД.

На macOS с Homebrew `psql` установлен в keg-only пакете `libpq`. Пароль можно ввести без отображения и без сохранения в истории:

```bash
read -s "PGPASSWORD?Пароль PostgreSQL: "
export PGPASSWORD
echo
export TEST_DATABASE_URL='postgres://USER@HOST:PORT/contracter_test?sslmode=require'
./scripts/verify.sh
unset PGPASSWORD TEST_DATABASE_URL
```

Не добавляйте настоящий Service URI, пароль или содержимое переменных окружения в Git, README, issue или сообщения чата.

`db/verify.sh` применяет SQL-миграции и seed-файлы к чистой тестовой БД, затем выполняет SQL-инварианты и проверку справочных данных. После этого интеграционный Rust-тест проверяет миграции через SQLx, health-check и rollback. Финальный `ROLLBACK` ожидаем: транзакционные fixture-записи не остаются в базе.

## Ограничения текущего этапа

- Risk policy v1 содержит четыре согласованных лимита, но остаётся неактивной до калибровки minimum notional и dispersion на реальных данных.
- Источник Market.CSGO добавлен выключенным; включение требует проверенного интеграционного процесса.
- Конкретные модели скинов, SKU и float bounds не добавлены без проверенного источника каталога.
- Для production остаются отдельными задачами deployment, мониторинг, резервное копирование, внешняя загрузка цен, Steam-интеграция и frontend.
