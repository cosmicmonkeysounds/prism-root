# plugin-bundles/finance

Finance bundle. Extends Flux with lending and budgeting entities (Loan, Grant, Budget) on top of the existing Flux finance types (`Transaction`, `Account`, `Invoice`), plus edges that link them together.

```ts
import { createFinanceBundle } from "@prism/core/plugin-bundles";
```

## What it registers

- **Categories** (`FINANCE_CATEGORIES`): `finance:lending`, `finance:budgeting`.
- **Entity types** (`FINANCE_TYPES`): `Loan`, `Grant`, `Budget`.
- **Edges** (`FINANCE_EDGES`): `funded-by`, `budget-for`, `payment-of`.
- **Status enums**: `LOAN_STATUSES` (application/approved/active/deferred/paid_off/defaulted), `GRANT_STATUSES` (researching → closed/rejected), `BUDGET_STATUSES` (draft/active/closed).
- **Plugin contributions**: finance views, commands, and activity-bar entries.

## Key exports

- `createFinanceBundle()` — self-registering `PluginBundle`.
- `createFinanceRegistry()` — lower-level `FinanceRegistry` exposing entity/edge defs, automation presets, and the `PrismPlugin`.
- Constants: `FINANCE_CATEGORIES`, `FINANCE_TYPES`, `FINANCE_EDGES`, `LOAN_STATUSES`, `GRANT_STATUSES`, `BUDGET_STATUSES`.
- Types: `FinanceCategory`, `FinanceEntityType`, `FinanceEdgeType`, `FinanceRegistry`.

## Usage

```ts
import {
  createFinanceBundle,
  installPluginBundles,
} from "@prism/core/plugin-bundles";

installPluginBundles([createFinanceBundle()], {
  objectRegistry,
  pluginRegistry,
});
```
