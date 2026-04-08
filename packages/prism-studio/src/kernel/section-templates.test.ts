/**
 * Tests for built-in section template presets.
 */

import { describe, it, expect, vi } from "vitest";
import {
  SECTION_TEMPLATES,
  HERO_SECTION_TEMPLATE,
  registerSectionTemplates,
} from "./section-templates.js";

describe("SECTION_TEMPLATES", () => {
  it("includes the six headline presets", () => {
    const ids = SECTION_TEMPLATES.map((t) => t.id);
    expect(ids).toEqual([
      "section-hero",
      "section-feature-grid",
      "section-testimonial",
      "section-pricing",
      "section-cta",
      "section-footer",
    ]);
  });

  it("each template has a section-typed root", () => {
    for (const t of SECTION_TEMPLATES) {
      expect(t.root.type).toBe("section");
      expect(t.category).toBe("section");
    }
  });

  it("hero template exposes headline/subtitle/ctaLabel variables", () => {
    const names = HERO_SECTION_TEMPLATE.variables?.map((v) => v.name) ?? [];
    expect(names).toContain("headline");
    expect(names).toContain("subtitle");
    expect(names).toContain("ctaLabel");
  });

  it("each template declares at least one child block", () => {
    for (const t of SECTION_TEMPLATES) {
      expect(t.root.children && t.root.children.length).toBeGreaterThan(0);
    }
  });
});

describe("registerSectionTemplates", () => {
  it("registers every preset on a kernel-like object", () => {
    const registerTemplate = vi.fn();
    registerSectionTemplates({ registerTemplate });
    expect(registerTemplate).toHaveBeenCalledTimes(SECTION_TEMPLATES.length);
    const registered = registerTemplate.mock.calls.map((c) => c[0].id);
    expect(registered).toContain("section-hero");
    expect(registered).toContain("section-footer");
  });
});
