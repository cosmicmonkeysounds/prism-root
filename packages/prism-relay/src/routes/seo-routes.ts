/**
 * SEO Routes — sitemap.xml and robots.txt generation from public portals.
 *
 * Auto-generates SEO assets from the PortalRegistry so that public
 * Sovereign Portals are discoverable by search engines.
 */

import { Hono } from "hono";
import type { RelayInstance, PortalRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

export function createSeoRoutes(relay: RelayInstance, publicUrl?: string): Hono {
  const app = new Hono();

  function getRegistry(): PortalRegistry | undefined {
    return relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
  }

  // GET /sitemap.xml — XML sitemap of all public portals
  app.get("/sitemap.xml", (c) => {
    const registry = getRegistry();
    const portals = registry ? registry.list().filter((p) => p.isPublic) : [];
    const base = publicUrl ?? "";

    const urls = portals.map((p) => {
      const loc = `${base}/portals/${p.portalId}`;
      const lastmod = p.createdAt.split("T")[0]; // YYYY-MM-DD
      return `  <url>\n    <loc>${escapeXml(loc)}</loc>\n    <lastmod>${lastmod}</lastmod>\n    <changefreq>${p.level >= 2 ? "hourly" : "daily"}</changefreq>\n    <priority>${p.level >= 3 ? "0.9" : "0.7"}</priority>\n  </url>`;
    });

    const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>${escapeXml(base)}/portals</loc>
    <changefreq>daily</changefreq>
    <priority>1.0</priority>
  </url>
${urls.join("\n")}
</urlset>`;

    c.header("Content-Type", "application/xml; charset=utf-8");
    return c.body(xml);
  });

  // GET /robots.txt — allow crawlers to index public portals
  app.get("/robots.txt", (c) => {
    const base = publicUrl ?? "";
    const txt = `User-agent: *
Allow: /portals/
Allow: /sitemap.xml
Disallow: /api/
Disallow: /ws/

Sitemap: ${base}/sitemap.xml
`;
    c.header("Content-Type", "text/plain; charset=utf-8");
    return c.body(txt);
  });

  return app;
}

function escapeXml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
