# Frontend UX contract

## Canonical UI Map

| Capability | Canonical owner | Source of truth | Allowed variants | Verification |
| --- | --- | --- | --- | --- |
| Select/Listbox | Native HTML select | `premium-ui.json` | OS-owned picker for compact filters | `src/pages/MarketPage.test.tsx`, `src/pages/InventoryPage.test.tsx` |
| Form | Application-owned validation with `noValidate` | `src/main.tsx` | Inline text errors and server errors | `src/main.test.tsx` |
| Scrollbar | Global application stylesheet | `src/accessibility.css` | WebKit fallback plus standards properties | `src/__tests__/dialog-accessibility.test.tsx` |

## Interaction decisions

- Market and inventory filters intentionally use native selects: their option sets are short, and OS keyboard/touch pickers are acceptable for this utility workflow.
- Authentication and verification forms disable browser validation bubbles and render application-owned text feedback.
- Scrollbar styling is global and keeps the native scrollbar operable; WebKit rules are only an engine fallback.
