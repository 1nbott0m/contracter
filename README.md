# Contracter database and economy core

Репозиторий содержит только фундамент базы данных и детерминированную экономическую логику Contracter. HTTP backend и frontend намеренно не реализованы.

## Состав проекта

- `crates/economy-core` — точные расчёты контрактов, цен и risk policy без сети и базы данных.
- `db/migrations` — упорядоченные миграции PostgreSQL.
- `db/seeds` — идемпотентные справочные данные без каталога конкретных скинов.
- `db/tests` — SQL-проверки ограничений, журналов и административных правил.
- `scripts/verify.sh` — единая локальная проверка.

## Работа в Zed

Откройте корень репозитория в Zed. Rust Analyzer использует workspace из `Cargo.toml`; дополнительных настроек редактора не требуется.

Полная офлайн-проверка:

```bash
./scripts/verify.sh
```

Она запускает форматирование, Clippy, все Rust-тесты и проверку shell-скриптов. Без `TEST_DATABASE_URL` интеграция PostgreSQL явно пропускается.

## Проверка на Aiven PostgreSQL

Скопируйте Host, Port и пароль пользователя базы из Aiven Quick connect. Пароль Aiven-аккаунта не подходит: нужен пароль PostgreSQL-пользователя, например `avnadmin`.

На macOS с Homebrew `psql` установлен в keg-only пакете `libpq`. Пароль можно ввести без отображения и без сохранения в истории:

```bash
read -s "PGPASSWORD?Пароль PostgreSQL: "
export PGPASSWORD
echo
export TEST_DATABASE_URL='postgres://avnadmin@HOST:PORT/defaultdb?sslmode=require'
./scripts/verify.sh
unset PGPASSWORD TEST_DATABASE_URL
```

Не добавляйте настоящий Service URI, пароль или содержимое переменных окружения в Git, README, issue или сообщения чата.

`db/verify.sh` применяет миграции и seed-файлы идемпотентно, затем выполняет SQL-инварианты и проверку справочных данных в транзакциях. Финальный `ROLLBACK` ожидаем: тестовые записи не остаются в базе.

## Ограничения текущего этапа

- Risk policy v1 содержит четыре согласованных лимита, но остаётся неактивной до калибровки minimum notional и dispersion на реальных данных.
- Источник Market.CSGO добавлен выключенным; включение требует проверенного интеграционного процесса.
- Конкретные модели скинов, SKU и float bounds не добавлены без проверенного источника каталога.
- Backend, HTTP API, Steam-аутентификация и frontend относятся к следующим этапам.
