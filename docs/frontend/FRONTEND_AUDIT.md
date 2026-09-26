# CONTRACTER Frontend Audit

Дата аудита: 2026-09-26  
Область: существующее React/Vite-приложение `frontend/`, опубликованный frontend
`https://contracter-1t9.pages.dev` и доступные production API.

## Ограничение аудита

На этом этапе код приложения не изменялся. Проверены исходники, существующие
дизайн-контексты, API-клиент, маршруты, тесты, production API и визуальное
состояние опубликованного маркета/регистрации.

## Executive summary

Frontend уже имеет рабочий каркас: единый shell, маршрутизацию, регистрацию,
Steam-вход, owner-scoped экраны, admin route, CC-формат, loading/empty/error
состояния и тестовый набор. Но это пока не production-grade frontend.

Критический дефект: production API через HTTP отвечает каталогом и valuation,
однако опубликованный браузерный `/market` показывает `API UNAVAILABLE`.
После полного импорта каталог содержит 6 821 SKU, а клиент делает до 35
последовательных запросов по 200 записей и только затем отображает страницу.
Повторный production-замер 2026-09-26: все 35 страниц заняли 14.03 с,
максимум одной страницы — 0.52 с; первая страница valuations заняла 0.24 с.
Это необходимо исправить до любых визуальных улучшений: пользователь видит
пустой маркет, хотя backend работает.

## Подтверждённые факты

- `frontend/DESIGN.md` и корневой `DESIGN.md` уже существуют; второй документ
  является текущим durable design context, поэтому новый параллельный
  `DESIGN.md` создавать не нужно.
- Стек: React 19 + Vite 8 + TypeScript; UI-библиотека shadcn/ui и Skiper не
  установлены. Из внешних UI-зависимостей фактически используется только
  `lucide-react`.
- Маршруты присутствуют для contracts, market, inventory, history, profile,
  login, register, transparency, terms, privacy, support и admin.
- Backend router предоставляет auth, Steam, account, admin/TOTP, inventory,
  catalog, market, quote lifecycle и history endpoints.
- Регистрация и вход production API проверены сквозным HTTP-тестом: register
  `201`, login `200`, `/me` `200`; session cookie содержит `HttpOnly`,
  `SameSite=None`, `Secure`.
- `GET /api/v1/catalog/skus?limit=200` и
  `GET /api/v1/market/valuations?limit=200` с production API отвечают `200`.
- Frontend проверка: 27 test files, 103 tests passed; TypeScript/Vite build
  passed.
- Production browser registration page содержит рабочие поля логина,
  пароля, подтверждения пароля и переходы shell.

## Найденные проблемы и приоритеты

### P0 — блокирует рабочий сайт

1. **Маркет не отображает данные в опубликованном браузере.**
   API доступен напрямую, но UI остаётся в состоянии `API UNAVAILABLE`.
   Точная причина ещё не доказана в DevTools; наиболее вероятная причина —
   полная последовательная загрузка 6 821 SKU через 35 страниц до первого
   рендера. Нужна инструментированная проверка fetch/таймаутов и изменение
   контракта загрузки: серверная пагинация/ограниченный публичный каталог,
   параллельная или инкрементальная загрузка, отображение первой страницы без
   блокировки всего экрана.
2. **Frontend не имеет наблюдаемого client-side telemetry/error boundary
   потока для production fetch failures.** Ошибка превращается в общий текст,
   а request id и стадия (catalog или valuation) не показываются пользователю
   и не доступны оператору.
3. **Catalog/valuation API расходятся по объёму.** Catalog содержит тысячи
   SKU, valuations — только опубликованные оценённые строки. UI должен явно
   показывать “цена пока не опубликована”, не превращая весь экран в ошибку.

### P1 — функциональная полнота и доверие

1. Login/register формы не имеют полноценной field-level валидации,
   password visibility control и общей accessible error model.
2. Admin screen находится внутри основного приложения и визуально является
   прототипом: нет отдельной навигации, фильтров, pagination, audit details,
   ban/credit/promo controls или безопасных confirmation flows.
3. TOTP provision выводит секрет текстом; отсутствует QR rendering, recovery
   policy, re-authentication и явное завершение setup.
4. История контрактов берёт presentation из `sessionStorage` и показывает
   “Artwork результата недоступен”, если серверный history payload не содержит
   детали результата. Нужен отдельный typed history detail API или ясный
   server-backed projection.
5. Market purchase controls уже отключаются при отсутствии подтверждённых
   оценок — это безопасно, но пользователь не получает объяснение на уровне
   конкретного SKU/времени обновления.
6. Полный список catalog грузится целиком в память и фильтруется локально;
   это не масштабируется для production и мобильных устройств.

### P2 — качество интерфейса и поддерживаемость

1. CSS разнесён по множеству файлов (`styles`, `extra`, `ux`, `commerce`,
   `layout-fixes`, `visual-polish`, `contract-builder` и др.) без единой
   documented cascade order; это повышает риск конфликтов и объясняет часть
   “квадратного/поехавшего” визуального поведения.
2. В `AppShell` есть общий shell, но reusable owners из DESIGN.md (`Panel`,
   `Button`, `Dialog`, `ToastRegion`, `DataList`) не представлены как единая
   component system; многие состояния реализованы screen-local markup.
3. Логотип — CSS-две наклонённые плашки с буквой `C`, без отдельного SVG asset
   и без явной responsive optical-size системы.
4. Motion частично присутствует, но не оформлен как единый motion token/flow
   contract для selection → validation → reveal → result; требуется visual
   browser review, а не только unit tests.
5. Нет подтверждённого Refero/design-reference connector в текущем окружении.
   Поэтому исследование Refero не было выполнено и не должно быть выдано за
   выполненное.

## Component-system audit

Сильные стороны: `AppShell`, `Logo`, `ProfileMenu`, `Footer`, `SkinCard`,
`SkinImage`, `ContractBuilder`, `ContractReveal`, `Panel`, filters и error
boundary уже дают основу для консолидации.

Слабые стороны: нет canonical Button/Dialog/Toast/DataList primitives;
асинхронные состояния повторяются в pages; API loading и domain empty state
смешаны; нет общего typed result model для admin/history/market tables.

Рекомендация: не добавлять shadcn или Skiper как массовую зависимость. Сначала
собрать маленький Contracter-owned primitive слой на существующем React и
`lucide-react`; это сохранит текущую визуальную идентичность и снизит bundle /
migration risk. shadcn можно точечно рассмотреть только для сложного Dialog,
Select или DataTable после определения accessibility contract.

## Backend/API compatibility map

| Frontend capability | API | Состояние |
|---|---|---|
| Registration/login/logout | `/api/v1/auth/*` | подключено, HTTP smoke passed |
| Steam | `/api/v1/auth/steam/*` | route exists, external provider setup required |
| Session/profile/balance | `/me`, `/me/balance` | подключено |
| Inventory | `/me/inventory` | подключено, client fetches all pages |
| Catalog | `/catalog/skus`, `/catalog/collections` | подключено, full fetch too large |
| Market | `/market/valuations`, purchase/buyback | API works; published browser P0 failure |
| Contract lifecycle | allocation → quote → accept | connected, needs browser E2E proof |
| History | `/me/history/contracts`, ledger/inventory endpoints | partial typed reader; presentation details limited |
| Admin | `/admin/me`, dashboard, users, audit, TOTP | connected prototype; needs production admin UX |

## Recommended implementation plan (after approval)

### P0 — restore visible product functionality

1. Instrument market fetch by stage and reproduce browser failure.
2. Replace “load all catalog before render” with bounded first-page/incremental
   loading, or add a server endpoint returning only publishable market rows.
3. Render market cards progressively and keep unpublished prices as explicit
   unavailable state.
4. Add browser smoke coverage for production-shaped API responses and a 35-page
   pagination fixture.

### P1 — make core flows dependable

1. Harden auth forms: field errors, show/hide password, busy state, focus
   recovery, and Steam failure return state.
2. Complete contract browser E2E: inventory selection, allocation, quote,
   cinematic reveal, accept, history, owner isolation and retry.
3. Split typed history readers into contract/ledger/inventory projections and
   use server data for detail views.
4. Build admin workspace as a protected route group with TOTP setup/verify,
   dashboard, audit table, safe mutation dialogs, pagination and no client
   authority over role/permissions.

### P2 — visual system and polish

1. Consolidate CSS variables and cascade ownership.
2. Promote shared primitives and document their states in `DESIGN.md` and a
   new `UX-CONTRACT.md` if the workflow remains multi-screen.
3. Replace the CSS logo with an owned SVG mark and test optical sizing.
4. Tune motion with reduced-motion and mobile verification; remove decorative
   overlays that obscure skin artwork.
5. Re-evaluate Refero references only when the reference connector is available;
   until then use the existing original Contracter direction as the source of
   truth.

## Approval gate

This document is an audit and plan only. No implementation changes are proposed
in this turn. The next implementation batch should start with P0 market loading
and browser instrumentation after explicit approval.
