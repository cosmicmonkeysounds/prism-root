# plugin-bundles/crm

CRM bundle. A plugin wrapper around existing Flux people types (`Contact`, `Organization`) — it registers **no new entity types or edges**. Instead it contributes CRM-specific views (contact directory, organization directory, deal pipeline, relationship graph), commands, keybindings, and an activity-bar entry that repurpose the shared Flux data model as a CRM.

```ts
import { createCrmBundle } from "@prism/core/plugin-bundles";
```

## What it registers

- **No new entity or edge types** — wraps Flux `Contact` and `Organization`.
- **Deal stages** (`CRM_DEAL_STAGES`): prospect, qualified, proposal, negotiation, closed_won, closed_lost — used by the pipeline view.
- **Activity types** (`CRM_ACTIVITY_TYPES`): call, email, meeting, note, task.
- **Plugin contributions**:
  - Views: `crm:contacts`, `crm:organizations`, `crm:pipeline`, `crm:relationships`.
  - Commands: `crm:new-contact`, `crm:new-organization`, `crm:log-activity`.
  - Keybinding: `ctrl+shift+c` → `crm:new-contact`.
  - Activity bar: `crm:activity`.

## Key exports

- `createCrmBundle()` — self-registering `PluginBundle`.
- `createCrmRegistry()` — returns a `CrmRegistry` with `getPlugin()`.
- Constants: `CRM_DEAL_STAGES`, `CRM_ACTIVITY_TYPES`.
- Types: `CrmRegistry`.

## Usage

```ts
import { createCrmBundle, installPluginBundles } from "@prism/core/plugin-bundles";

installPluginBundles([createCrmBundle()], {
  objectRegistry,
  pluginRegistry,
});
```
