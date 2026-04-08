import { describe, it, expect } from "vitest";
import { createWorkRegistry, WORK_TYPES, WORK_EDGES } from "./work/index.js";
import { createFinanceRegistry, FINANCE_TYPES, FINANCE_EDGES } from "./finance/index.js";
import { createCrmRegistry } from "./crm/index.js";
import { createLifeRegistry, LIFE_TYPES, LIFE_EDGES } from "./life/index.js";
import { createAssetsRegistry, ASSETS_TYPES, ASSETS_EDGES } from "./assets/index.js";
import { createPlatformRegistry, PLATFORM_TYPES, PLATFORM_EDGES } from "./platform/index.js";

describe("plugin-work", () => {
  const reg = createWorkRegistry();

  it("registers 3 entity types", () => {
    expect(reg.getEntityDefs()).toHaveLength(3);
  });

  it("registers gig entity", () => {
    const gig = reg.getEntityDef(WORK_TYPES.GIG);
    expect(gig).toBeDefined();
    expect(gig?.label).toBe("Gig");
    expect(gig?.fields?.some(f => f.id === "rate")).toBe(true);
  });

  it("registers time-entry entity", () => {
    const te = reg.getEntityDef(WORK_TYPES.TIME_ENTRY);
    expect(te).toBeDefined();
    expect(te?.childOnly).toBe(true);
  });

  it("registers focus-block entity", () => {
    const fb = reg.getEntityDef(WORK_TYPES.FOCUS_BLOCK);
    expect(fb).toBeDefined();
    expect(fb?.fields?.some(f => f.id === "cognitiveLoad")).toBe(true);
  });

  it("registers 3 edge types", () => {
    expect(reg.getEdgeDefs()).toHaveLength(3);
    expect(reg.getEdgeDef(WORK_EDGES.TRACKED_FOR)).toBeDefined();
    expect(reg.getEdgeDef(WORK_EDGES.BILLED_TO)).toBeDefined();
    expect(reg.getEdgeDef(WORK_EDGES.FOCUS_ON)).toBeDefined();
  });

  it("registers automation presets", () => {
    expect(reg.getAutomationPresets().length).toBeGreaterThan(0);
  });

  it("provides a plugin", () => {
    const plugin = reg.getPlugin();
    expect(plugin.id).toBe("prism.plugin.work");
    expect(plugin.contributes?.views?.length).toBeGreaterThan(0);
    expect(plugin.contributes?.commands?.length).toBeGreaterThan(0);
  });
});

describe("plugin-finance", () => {
  const reg = createFinanceRegistry();

  it("registers 3 entity types", () => {
    expect(reg.getEntityDefs()).toHaveLength(3);
    expect(reg.getEntityDef(FINANCE_TYPES.LOAN)).toBeDefined();
    expect(reg.getEntityDef(FINANCE_TYPES.GRANT)).toBeDefined();
    expect(reg.getEntityDef(FINANCE_TYPES.BUDGET)).toBeDefined();
  });

  it("registers 3 edge types", () => {
    expect(reg.getEdgeDefs()).toHaveLength(3);
    expect(reg.getEdgeDef(FINANCE_EDGES.FUNDED_BY)).toBeDefined();
    expect(reg.getEdgeDef(FINANCE_EDGES.BUDGET_FOR)).toBeDefined();
    expect(reg.getEdgeDef(FINANCE_EDGES.PAYMENT_OF)).toBeDefined();
  });

  it("provides a plugin", () => {
    const plugin = reg.getPlugin();
    expect(plugin.id).toBe("prism.plugin.finance");
  });

  it("loan has interest rate field", () => {
    const loan = reg.getEntityDef(FINANCE_TYPES.LOAN);
    expect(loan?.fields?.some(f => f.id === "interestRate")).toBe(true);
  });

  it("budget has computed remaining field", () => {
    const budget = reg.getEntityDef(FINANCE_TYPES.BUDGET);
    const remaining = budget?.fields?.find(f => f.id === "remainingAmount");
    expect(remaining?.expression).toBeDefined();
  });
});

describe("plugin-crm", () => {
  const reg = createCrmRegistry();

  it("provides a plugin with CRM views", () => {
    const plugin = reg.getPlugin();
    expect(plugin.id).toBe("prism.plugin.crm");
    expect(plugin.contributes?.views?.some(v => v.id === "crm:pipeline")).toBe(true);
    expect(plugin.contributes?.views?.some(v => v.id === "crm:relationships")).toBe(true);
  });
});

describe("plugin-life", () => {
  const reg = createLifeRegistry();

  it("registers 7 entity types", () => {
    expect(reg.getEntityDefs()).toHaveLength(7);
  });

  it("registers habit with streak field", () => {
    const habit = reg.getEntityDef(LIFE_TYPES.HABIT);
    expect(habit).toBeDefined();
    expect(habit?.fields?.some(f => f.id === "streak")).toBe(true);
  });

  it("registers habit-log as child-only", () => {
    const log = reg.getEntityDef(LIFE_TYPES.HABIT_LOG);
    expect(log?.childOnly).toBe(true);
  });

  it("registers fitness-log with workout type", () => {
    const fl = reg.getEntityDef(LIFE_TYPES.FITNESS_LOG);
    expect(fl?.fields?.some(f => f.id === "workoutType")).toBe(true);
  });

  it("registers sleep-record with quality", () => {
    const sr = reg.getEntityDef(LIFE_TYPES.SLEEP_RECORD);
    expect(sr?.fields?.some(f => f.id === "quality")).toBe(true);
  });

  it("registers journal-entry with mood", () => {
    const je = reg.getEntityDef(LIFE_TYPES.JOURNAL_ENTRY);
    expect(je?.fields?.some(f => f.id === "mood")).toBe(true);
  });

  it("registers cycle-entry with phase and flow", () => {
    const ce = reg.getEntityDef(LIFE_TYPES.CYCLE_ENTRY);
    expect(ce?.fields?.some(f => f.id === "phase")).toBe(true);
    expect(ce?.fields?.some(f => f.id === "flow")).toBe(true);
  });

  it("registers 3 edge types", () => {
    expect(reg.getEdgeDefs()).toHaveLength(3);
    expect(reg.getEdgeDef(LIFE_EDGES.LOG_OF)).toBeDefined();
    expect(reg.getEdgeDef(LIFE_EDGES.RELATED_SYMPTOM)).toBeDefined();
  });

  it("provides a plugin", () => {
    const plugin = reg.getPlugin();
    expect(plugin.id).toBe("prism.plugin.life");
    expect(plugin.contributes?.views?.some(v => v.id === "life:habits")).toBe(true);
    expect(plugin.contributes?.views?.some(v => v.id === "life:journal")).toBe(true);
  });
});

describe("plugin-assets", () => {
  const reg = createAssetsRegistry();

  it("registers 4 entity types", () => {
    expect(reg.getEntityDefs()).toHaveLength(4);
    expect(reg.getEntityDef(ASSETS_TYPES.MEDIA_ASSET)).toBeDefined();
    expect(reg.getEntityDef(ASSETS_TYPES.CONTENT_ITEM)).toBeDefined();
    expect(reg.getEntityDef(ASSETS_TYPES.SCANNED_DOC)).toBeDefined();
    expect(reg.getEntityDef(ASSETS_TYPES.COLLECTION)).toBeDefined();
  });

  it("collection can hold other asset types", () => {
    const col = reg.getEntityDef(ASSETS_TYPES.COLLECTION);
    expect(col?.extraChildTypes).toContain(ASSETS_TYPES.MEDIA_ASSET);
    expect(col?.extraChildTypes).toContain(ASSETS_TYPES.CONTENT_ITEM);
  });

  it("registers 3 edge types", () => {
    expect(reg.getEdgeDefs()).toHaveLength(3);
    expect(reg.getEdgeDef(ASSETS_EDGES.IN_COLLECTION)).toBeDefined();
    expect(reg.getEdgeDef(ASSETS_EDGES.DERIVED_FROM)).toBeDefined();
    expect(reg.getEdgeDef(ASSETS_EDGES.ATTACHED_TO)).toBeDefined();
  });

  it("scanned doc has OCR fields", () => {
    const doc = reg.getEntityDef(ASSETS_TYPES.SCANNED_DOC);
    expect(doc?.fields?.some(f => f.id === "ocrText")).toBe(true);
    expect(doc?.fields?.some(f => f.id === "ocrConfidence")).toBe(true);
  });

  it("provides a plugin", () => {
    const plugin = reg.getPlugin();
    expect(plugin.id).toBe("prism.plugin.assets");
  });
});

describe("plugin-platform", () => {
  const reg = createPlatformRegistry();

  it("registers 5 entity types", () => {
    expect(reg.getEntityDefs()).toHaveLength(5);
    expect(reg.getEntityDef(PLATFORM_TYPES.CALENDAR_EVENT)).toBeDefined();
    expect(reg.getEntityDef(PLATFORM_TYPES.MESSAGE)).toBeDefined();
    expect(reg.getEntityDef(PLATFORM_TYPES.REMINDER)).toBeDefined();
    expect(reg.getEntityDef(PLATFORM_TYPES.FEED)).toBeDefined();
    expect(reg.getEntityDef(PLATFORM_TYPES.FEED_ITEM)).toBeDefined();
  });

  it("feed-item is child-only", () => {
    const fi = reg.getEntityDef(PLATFORM_TYPES.FEED_ITEM);
    expect(fi?.childOnly).toBe(true);
  });

  it("calendar event has recurrence", () => {
    const ev = reg.getEntityDef(PLATFORM_TYPES.CALENDAR_EVENT);
    expect(ev?.fields?.some(f => f.id === "recurrence")).toBe(true);
  });

  it("message has channel field", () => {
    const msg = reg.getEntityDef(PLATFORM_TYPES.MESSAGE);
    expect(msg?.fields?.some(f => f.id === "channel")).toBe(true);
  });

  it("registers 4 edge types", () => {
    expect(reg.getEdgeDefs()).toHaveLength(4);
    expect(reg.getEdgeDef(PLATFORM_EDGES.REMINDS_ABOUT)).toBeDefined();
    expect(reg.getEdgeDef(PLATFORM_EDGES.EVENT_FOR)).toBeDefined();
    expect(reg.getEdgeDef(PLATFORM_EDGES.REPLY_TO)).toBeDefined();
    expect(reg.getEdgeDef(PLATFORM_EDGES.FEED_SOURCE)).toBeDefined();
  });

  it("provides a plugin", () => {
    const plugin = reg.getPlugin();
    expect(plugin.id).toBe("prism.plugin.platform");
    expect(plugin.contributes?.views?.some(v => v.id === "platform:calendar")).toBe(true);
    expect(plugin.contributes?.views?.some(v => v.id === "platform:inbox")).toBe(true);
  });
});

describe("cross-plugin", () => {
  it("all plugins have unique IDs", () => {
    const ids = [
      createWorkRegistry().getPlugin().id,
      createFinanceRegistry().getPlugin().id,
      createCrmRegistry().getPlugin().id,
      createLifeRegistry().getPlugin().id,
      createAssetsRegistry().getPlugin().id,
      createPlatformRegistry().getPlugin().id,
    ];
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("all entity types are unique across plugins", () => {
    const allTypes = [
      ...createWorkRegistry().getEntityDefs(),
      ...createFinanceRegistry().getEntityDefs(),
      ...createLifeRegistry().getEntityDefs(),
      ...createAssetsRegistry().getEntityDefs(),
      ...createPlatformRegistry().getEntityDefs(),
    ].map(d => d.type);
    expect(new Set(allTypes).size).toBe(allTypes.length);
  });

  it("all edge relations are unique across plugins", () => {
    const allRelations = [
      ...createWorkRegistry().getEdgeDefs(),
      ...createFinanceRegistry().getEdgeDefs(),
      ...createLifeRegistry().getEdgeDefs(),
      ...createAssetsRegistry().getEdgeDefs(),
      ...createPlatformRegistry().getEdgeDefs(),
    ].map(d => d.relation);
    expect(new Set(allRelations).size).toBe(allRelations.length);
  });

  it("all NSIDs are unique across plugins", () => {
    const allNsids = [
      ...createWorkRegistry().getEntityDefs(),
      ...createFinanceRegistry().getEntityDefs(),
      ...createLifeRegistry().getEntityDefs(),
      ...createAssetsRegistry().getEntityDefs(),
      ...createPlatformRegistry().getEntityDefs(),
    ].map(d => d.nsid).filter(Boolean);
    expect(new Set(allNsids).size).toBe(allNsids.length);
  });
});
