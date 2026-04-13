# plugin-bundles/platform

Platform Services bundle. Cross-cutting platform capabilities rather than a single domain: calendar events, messaging, reminders, and feeds. Registered as first-class Flux entity types so they can be queried, linked, and automated alongside everything else.

```ts
import { createPlatformBundle } from "@prism/core/plugin-bundles";
```

## What it registers

- **Categories** (`PLATFORM_CATEGORIES`): `platform:calendar`, `platform:messaging`, `platform:reminders`, `platform:feeds`.
- **Entity types** (`PLATFORM_TYPES`): `CalendarEvent`, `Message`, `Reminder`, `Feed`, `FeedItem`.
- **Edges** (`PLATFORM_EDGES`): `reminds-about`, `event-for`, `reply-to`, `feed-source`.
- **Status / enum sets**: `EVENT_STATUSES`, `EVENT_RECURRENCES` (none/daily/weekly/biweekly/monthly/yearly), `MESSAGE_STATUSES`, `MESSAGE_CHANNELS` (internal/email/sms/slack/discord), `REMINDER_STATUSES`, `REMINDER_PRIORITIES`.
- **Plugin contributions**: platform-service views, commands, and activity-bar entries.

## Key exports

- `createPlatformBundle()` — self-registering `PluginBundle`.
- `createPlatformRegistry()` — lower-level `PlatformRegistry` exposing entity/edge defs, automation presets, and the `PrismPlugin`.
- Constants: `PLATFORM_CATEGORIES`, `PLATFORM_TYPES`, `PLATFORM_EDGES`, `EVENT_STATUSES`, `EVENT_RECURRENCES`, `MESSAGE_STATUSES`, `MESSAGE_CHANNELS`, `REMINDER_STATUSES`, `REMINDER_PRIORITIES`.
- Types: `PlatformCategory`, `PlatformEntityType`, `PlatformEdgeType`, `PlatformRegistry`.

## Usage

```ts
import {
  createPlatformBundle,
  installPluginBundles,
} from "@prism/core/plugin-bundles";

installPluginBundles([createPlatformBundle()], {
  objectRegistry,
  pluginRegistry,
});
```
