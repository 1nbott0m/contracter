# Contracter

Репозиторий содержит полный текущий MVP Contracter: frontend, PostgreSQL-схему и инварианты, детерминированное экономическое ядро, application-слой, HTTP API на Axum и бинарный сервер. Frontend собирается отдельно из `frontend/` и подключается к API через `VITE_API_URL`. Внешняя загрузка цен и реальные платежи остаются отдельными интеграциями; вход через Steam реализован в backend и frontend.

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
- `frontend` — production frontend на React/Vite: регистрация, вход, Steam, контракты, инвентарь, маркет, история и административный интерфейс.

## Переменные окружения и запуск

Скопируйте [.env.example](.env.example) в игнорируемый локальный файл и подставьте значения только локально. Сервер требует `DATABASE_URL`; `HOST`, `PORT`, `LOG_FILTER` и `RATE_LIMIT_BURST=30` имеют безопасные значения по умолчанию. `INSECURE_COOKIES=true` допустим только для локального HTTP: в обычном режиме cookie имеют `Secure` и `__Host-` защиту.

Production-развёртывание должно завершать TLS на доверенном reverse proxy и передавать приложению реальный peer IP. Backend добавляет HSTS ко всем ответам, использует Secure-cookie и ограничивает запросы по peer IP; не публикуйте порт приложения напрямую в интернет и не доверяйте клиентским `X-Forwarded-For` без настройки proxy.

Внутренняя валюта платформы — `CC` (Contracter Coins). Ledger, баланс,
market и quote используют целые micro-CC; API помечает денежные ответы
`currency_code: "CC"`. Рубли не являются валютой ledger: они могут появиться
только на будущем платёжном входе и конвертируются сервером по зафиксированному
курсу из подтверждённого события провайдера. Клиент не задаёт курс или сумму
зачисления CC.

Для server-side импорта завершённых продаж Market.CSGO ключ хранится только в
Render Environment под именем `MARKET_CSGO_API_KEY`. Он не должен попадать во
frontend, Git или логи. `MARKET_RUB_TO_CC_RATE` задаёт явный курс пересчёта
внешней цены в CC; публикация valuation всё равно требует валидных данных
sale evidence и минимум 20 продаж на SKU.

```bash
DATABASE_URL='postgres://USER@HOST:PORT/contracter?sslmode=require' \
cargo run -p server --bin contracter-server
```

`TEST_DATABASE_URL` предназначен только для отдельной тестовой БД. Никогда не направляйте проверки на shared или production БД: integration fixtures намеренно создают записи.

### Импорт цен Market.CSGO

Импорт выполняется отдельной server-side job, а не HTTP-маршрутом:

```bash
MARKET_CSGO_API_KEY='...' \
MARKET_IMPORT_DATABASE_URL='postgres://.../contracter?sslmode=require' \
MARKET_RUB_TO_CC_RATE='1' \
python3 scripts/import_market_csgo.py
```

Первый запуск только сохраняет новые sale evidence и ничего не публикует.
После проверки количества валидных продаж запускайте с `--publish`:

```bash
MARKET_CSGO_API_KEY='...' MARKET_IMPORT_DATABASE_URL='postgres://...' \
MARKET_RUB_TO_CC_RATE='1' python3 scripts/import_market_csgo.py --publish
```

Для job нужен DB-пользователь с `EXECUTE` на функциях миграции `0043`, но не
доступ к HTTP runtime. Ключ и DB URL не выводятся и не должны попадать в Git.

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

### Первый администратор

Регистрация пользователей открыта через `POST /api/v1/auth/register` и страницу `/register`.
Новая учётная запись по умолчанию не получает административных прав. После регистрации
первого владельца проекта выполните в SQL-консоли Aiven (или в отдельной безопасной
операционной сессии) запрос ниже, заменив только `YOUR_LOGIN`:

```sql
INSERT INTO administrators (user_id)
SELECT id FROM users WHERE lower(login) = lower('YOUR_LOGIN')
ON CONFLICT (user_id) DO UPDATE
SET is_active = true, deactivated_at = NULL;
```

Запрос идемпотентен. Проверка роли выполняется сервером на каждом запросе к
`/api/v1/admin/me`; не добавляйте администратора через клиентский код и не публикуйте
строку подключения к базе.

После назначения роли администратор открывает `/admin`, создаёт секрет 2FA и добавляет
его в Google Authenticator. Секрет хранится в базе только в зашифрованном виде; для
шифрования используется обязательный `QUOTE_SEED_KEY`, а не пароль пользователя.
Затем нужно подтвердить шестизначный код. Не передавайте секрет или QR-данные в чат.

## Ограничения текущего этапа

- Risk policy v1 содержит четыре согласованных лимита, но остаётся неактивной до калибровки minimum notional и dispersion на реальных данных.
- Источник Market.CSGO добавлен выключенным; включение требует проверенного интеграционного процесса.
- Конкретные модели скинов, SKU и float bounds не добавлены без проверенного источника каталога.
- Для production остаются отдельными задачами deployment, мониторинг, резервное копирование, внешняя загрузка цен и платёжная интеграция. Steam-вход и frontend уже входят в текущий MVP; их production-настройки (URL, секреты и redirect-конфигурация) должны совпадать с окружением деплоя.

# Production deployment
See deploy/README.md and deploy/docker-compose.prod.yml. Internal balances are CC; RUB is only an external top-up input.
