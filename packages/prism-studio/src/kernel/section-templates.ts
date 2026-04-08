/**
 * Pre-defined section templates covering the common landing-page
 * patterns users expect from SquareSpace / Webflow / Framer: hero,
 * feature grid, testimonial, pricing, call-to-action, footer.
 *
 * Each template root is a `section` block so it can be dropped under
 * any `page` object. The templates are registered at Studio startup
 * via `registerSectionTemplates()`.
 */

import type { ObjectTemplate } from "@prism/core/template";

const now = () => new Date().toISOString();

/** Hero with headline + subtitle + CTA button. */
export const HERO_SECTION_TEMPLATE: ObjectTemplate = {
  id: "section-hero",
  name: "Hero Section",
  description: "Headline + subtitle + call-to-action button",
  category: "section",
  createdAt: now(),
  root: {
    placeholderId: "root",
    type: "section",
    name: "Hero",
    data: {
      variant: "hero",
      padding: "xl",
      paddingX: 32,
      paddingY: 64,
      backgroundColor: "#0f172a",
      textAlign: "center",
    },
    children: [
      {
        placeholderId: "headline",
        type: "heading",
        name: "Headline",
        data: {
          text: "{{headline}}",
          level: "h1",
          align: "center",
          color: "#ffffff",
          fontSize: 48,
        },
      },
      {
        placeholderId: "subtitle",
        type: "text-block",
        name: "Subtitle",
        data: {
          content: "{{subtitle}}",
          format: "markdown",
          color: "#cbd5e1",
          fontSize: 18,
          textAlign: "center",
        },
      },
      {
        placeholderId: "cta",
        type: "button",
        name: "Primary CTA",
        data: {
          label: "{{ctaLabel}}",
          variant: "primary",
          url: "#",
        },
      },
    ],
  },
  variables: [
    { name: "headline", label: "Headline", required: true },
    { name: "subtitle", label: "Subtitle", required: false },
    { name: "ctaLabel", label: "Button Label", required: false },
  ],
};

/** Feature grid — three equal columns. */
export const FEATURE_GRID_TEMPLATE: ObjectTemplate = {
  id: "section-feature-grid",
  name: "Feature Grid",
  description: "Three-column grid of feature cards",
  category: "section",
  createdAt: now(),
  root: {
    placeholderId: "root",
    type: "section",
    name: "Features",
    data: { variant: "default", padding: "lg", paddingY: 48 },
    children: [
      {
        placeholderId: "heading",
        type: "heading",
        name: "Features Heading",
        data: { text: "Features", level: "h2", align: "center" },
      },
      {
        placeholderId: "feature1",
        type: "card",
        name: "Feature 1",
        data: {
          title: "Fast",
          body: "Delivered in milliseconds — no compile step.",
        },
      },
      {
        placeholderId: "feature2",
        type: "card",
        name: "Feature 2",
        data: {
          title: "Simple",
          body: "A clean API your whole team can learn in a day.",
        },
      },
      {
        placeholderId: "feature3",
        type: "card",
        name: "Feature 3",
        data: {
          title: "Open",
          body: "Vendor-lockin free. Self-host on anything.",
        },
      },
    ],
  },
};

/** Testimonial section — quote + attribution. */
export const TESTIMONIAL_TEMPLATE: ObjectTemplate = {
  id: "section-testimonial",
  name: "Testimonial",
  description: "Pull quote with attribution",
  category: "section",
  createdAt: now(),
  root: {
    placeholderId: "root",
    type: "section",
    name: "Testimonial",
    data: {
      variant: "default",
      padding: "lg",
      paddingY: 64,
      backgroundColor: "#f8fafc",
      textAlign: "center",
    },
    children: [
      {
        placeholderId: "quote",
        type: "text-block",
        name: "Quote",
        data: {
          content: "> {{quote}}",
          format: "markdown",
          fontSize: 24,
          textAlign: "center",
        },
      },
      {
        placeholderId: "attribution",
        type: "text-block",
        name: "Attribution",
        data: {
          content: "— **{{author}}**, {{role}}",
          format: "markdown",
          textAlign: "center",
          color: "#64748b",
        },
      },
    ],
  },
  variables: [
    { name: "quote", label: "Quote", required: true },
    { name: "author", label: "Author", required: true },
    { name: "role", label: "Role / Company", required: false },
  ],
};

/** Pricing section — three tiers. */
export const PRICING_TEMPLATE: ObjectTemplate = {
  id: "section-pricing",
  name: "Pricing",
  description: "Three-tier pricing table",
  category: "section",
  createdAt: now(),
  root: {
    placeholderId: "root",
    type: "section",
    name: "Pricing",
    data: { variant: "default", padding: "lg", paddingY: 64 },
    children: [
      {
        placeholderId: "heading",
        type: "heading",
        name: "Pricing Heading",
        data: { text: "Simple pricing", level: "h2", align: "center" },
      },
      {
        placeholderId: "tier-free",
        type: "card",
        name: "Free",
        data: { title: "Free", body: "$0 / month — Hobby projects." },
      },
      {
        placeholderId: "tier-pro",
        type: "card",
        name: "Pro",
        data: { title: "Pro", body: "$12 / month — For individuals." },
      },
      {
        placeholderId: "tier-team",
        type: "card",
        name: "Team",
        data: { title: "Team", body: "$48 / month — Up to 10 seats." },
      },
    ],
  },
};

/** Call-to-action section — short headline + single button. */
export const CTA_TEMPLATE: ObjectTemplate = {
  id: "section-cta",
  name: "Call to Action",
  description: "Short headline + button",
  category: "section",
  createdAt: now(),
  root: {
    placeholderId: "root",
    type: "section",
    name: "Call to Action",
    data: {
      variant: "hero",
      padding: "lg",
      paddingY: 48,
      backgroundColor: "#1e40af",
      textAlign: "center",
    },
    children: [
      {
        placeholderId: "headline",
        type: "heading",
        name: "CTA Headline",
        data: {
          text: "{{headline}}",
          level: "h2",
          align: "center",
          color: "#ffffff",
        },
      },
      {
        placeholderId: "btn",
        type: "button",
        name: "CTA Button",
        data: { label: "{{ctaLabel}}", variant: "primary", url: "#" },
      },
    ],
  },
  variables: [
    { name: "headline", label: "Headline", required: true },
    { name: "ctaLabel", label: "Button Label", required: false },
  ],
};

/** Footer section — copyright + links. */
export const FOOTER_TEMPLATE: ObjectTemplate = {
  id: "section-footer",
  name: "Footer",
  description: "Copyright + footer links",
  category: "section",
  createdAt: now(),
  root: {
    placeholderId: "root",
    type: "section",
    name: "Footer",
    data: {
      variant: "default",
      padding: "md",
      paddingY: 32,
      backgroundColor: "#0f172a",
      textAlign: "center",
    },
    children: [
      {
        placeholderId: "copy",
        type: "text-block",
        name: "Copyright",
        data: {
          content: "© {{year}} {{company}}. All rights reserved.",
          format: "markdown",
          textAlign: "center",
          color: "#94a3b8",
        },
      },
    ],
  },
  variables: [
    { name: "year", label: "Year", required: false },
    { name: "company", label: "Company Name", required: true },
  ],
};

export const SECTION_TEMPLATES: ObjectTemplate[] = [
  HERO_SECTION_TEMPLATE,
  FEATURE_GRID_TEMPLATE,
  TESTIMONIAL_TEMPLATE,
  PRICING_TEMPLATE,
  CTA_TEMPLATE,
  FOOTER_TEMPLATE,
];

/**
 * Register all built-in section templates on a studio kernel.
 * Safe to call multiple times — templates are keyed by id.
 */
export function registerSectionTemplates(
  kernel: { registerTemplate: (t: ObjectTemplate) => void },
): void {
  for (const template of SECTION_TEMPLATES) {
    kernel.registerTemplate(template);
  }
}
