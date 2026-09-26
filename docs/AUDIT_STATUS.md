# CONTRACTER — статус аудита 190 пунктов

Дата проверки: 2026-09-26

Статусы в этом документе намеренно консервативны:

- **DONE** — есть проверяемое доказательство в CI, тесте, production smoke или репозитории;
- **PARTIAL** — часть требования реализована, но не весь пункт доказан;
- **UNVERIFIED** — нужен отдельный тест, production-доступ или внешний секрет;
- **TODO** — в текущем checkout не найдено реализации или доказательства.

## Сводка

| Статус | Количество | Правило подсчёта |
|---|---:|---|
| DONE | 16 | пункты, отражённые как выполненные в `docs/LAUNCH_CHECKLIST.md` |
| PARTIAL | не подсчитывается автоматически | реализация есть, но пункт шире имеющегося доказательства |
| UNVERIFIED | не подсчитывается автоматически | требуется production/staging или отдельная проверка |
| TODO | не подсчитывается автоматически | отсутствие реализации ещё не доказано для всех пунктов |

Число `16/190` — это **строгое число подтверждённых launch-gates**, а не утверждение, что остальные 174 пункта отсутствуют.

## Уже подтверждено

Подтверждения находятся в [`LAUNCH_CHECKLIST.md`](LAUNCH_CHECKLIST.md):

1. форматирование Rust;
2. Clippy с `-D warnings`;
3. workspace/PostgreSQL integration в GitHub Actions;
4. frontend tests и production build;
5. `cargo audit` и production `npm audit`;
6. safety checks deployment manifests;
7. backup/restore guards;
8. concurrent market purchase/inventory-selection tests;
9. concurrent quote acceptance test;
10. Cloudflare frontend HTTP 200;
11. frontend bundle использует Render API;
12. `/health/live`;
13. `/health/ready`;
14. registration/login/`/me`/logout и post-logout 401;
15. запрет неподходящего `Origin`;
16. security response headers.

## Не считать закрытым без новой проверки

- Steam OAuth с реальным application configuration;
- публикация подтверждённого valuation snapshot importer’ом;
- market purchase с опубликованной production valuation;
- production secrets, ротация ключей и TOTP двух администраторов;
- non-production backup/restore drill;
- staging-only k6 smoke/load test;
- image vulnerability scan, branch protection и release tagging;
- полное визуальное/accessibility ревью всех frontend-пунктов.

## Правило обновления

Каждый пункт переводится в `DONE` только после добавления конкретной ссылки на тест,
команду, CI job, runtime smoke или production evidence. Прохождение общего build не
закрывает автоматически бизнес- или эксплуатационный пункт.
