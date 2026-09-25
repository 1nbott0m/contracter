# CONTRACTER — передача проекта

## 1. Репозиторий

```bash
git clone https://github.com/1nbott0m/contracter.git
cd contracter
git checkout codex/quote-write-v2
```

Не добавляйте в Git `DATABASE_URL`, ключи подписи, пароли или секреты OAuth.

## 2. Локальная проверка

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
cd frontend
npm ci
npm test
npm run build
```

Для integration-тестов нужна отдельная свежая PostgreSQL и `TEST_DATABASE_URL`.

## 3. Production

- PostgreSQL: Aiven, миграции применяются сервером при запуске.
- Backend: Render, ветка `codex/quote-write-v2`, Dockerfile из репозитория.
- Frontend: Cloudflare Pages, корень `frontend`, команда `npm run build`, output `dist`.
- Frontend variable: `VITE_API_URL=https://contracter.onrender.com`.

Секреты backend добавляются только в Render Environment: `DATABASE_URL`,
`QUOTE_SIGNING_KEY`, `QUOTE_SEED_KEY` и остальные значения из `.env.example`.

## 4. Два администратора

Сначала каждый администратор регистрируется через `/register`. Затем для каждого
логина выполняется в Aiven PG Studio безопасный идемпотентный bootstrap:

```sql
INSERT INTO administrators (user_id)
SELECT id FROM users WHERE lower(login) = lower('ADMIN_LOGIN')
ON CONFLICT (user_id) DO UPDATE
SET is_active = true, deactivated_at = NULL;
```

Пароли задаются самими администраторами и не записываются в исходники.

## 5. Проверка перед выпуском

Проверить `/health/live`, `/health/ready`, регистрацию, повторный вход, owner
isolation, market idempotency, контракт, inventory/history и `/admin`. При любом
расхождении сначала сохранить HTTP-код и тело ошибки, затем исправлять backend,
а не обходить защиту во frontend.
