/** @jsxImportSource hono/jsx */

/**
 * Portal View Routes — serves Sovereign Portal HTML pages.
 *
 * Level 1: Static read-only HTML snapshot
 * Level 2: HTML with WebSocket script for live CRDT updates
 * Level 3+: HTML with interactive form support (future)
 *
 * These routes serve rendered pages at /portals/:id, separate from
 * the JSON API at /api/portals.
 */

import { Hono } from "hono";
import type { FC } from "hono/jsx";
import { html } from "hono/html";
import type { RelayInstance, PortalRegistry, CollectionHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES, extractPortalSnapshot } from "@prism/core/relay";
import type { PortalSnapshot, PortalObject } from "@prism/core/relay";

// ── JSX Components ─────────────────────────────────────────────────────────

const PortalStyles: FC = () => html`
<style>
  :root { --bg: #fff; --fg: #1a1a1a; --muted: #666; --accent: #2563eb; --border: #e5e7eb; --card-bg: #fafafa; }
  @media (prefers-color-scheme: dark) {
    :root { --bg: #0a0a0a; --fg: #e5e5e5; --muted: #999; --accent: #60a5fa; --border: #333; --card-bg: #111; }
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: system-ui, -apple-system, sans-serif; background: var(--bg); color: var(--fg); line-height: 1.6; }
  .container { max-width: 72rem; margin: 0 auto; padding: 2rem; }
  header { border-bottom: 1px solid var(--border); padding-bottom: 1rem; margin-bottom: 2rem; }
  h1 { font-size: 1.75rem; font-weight: 600; }
  .meta { color: var(--muted); font-size: 0.875rem; margin-top: 0.25rem; }
  .object-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(20rem, 1fr)); gap: 1rem; }
  .card { padding: 1rem; border: 1px solid var(--border); border-radius: 0.5rem; background: var(--card-bg); }
  .card h2 { font-size: 1.1rem; font-weight: 600; }
  .card h3 { font-size: 1rem; font-weight: 500; }
  .card-desc { color: var(--muted); font-size: 0.9rem; margin-top: 0.25rem; }
  .card-meta { display: flex; gap: 0.5rem; align-items: center; margin-top: 0.5rem; flex-wrap: wrap; }
  .badge { display: inline-block; font-size: 0.75rem; padding: 0.125rem 0.5rem; border-radius: 9999px; }
  .badge-status { background: var(--accent); color: white; }
  .badge-tag { border: 1px solid var(--border); color: var(--muted); border-radius: 0.25rem; }
  .badge-type { background: var(--border); color: var(--fg); border-radius: 0.25rem; }
  time { font-size: 0.8rem; color: var(--muted); }
  .children { margin-top: 0.75rem; display: flex; flex-direction: column; gap: 0.5rem; padding-left: 0.75rem; border-left: 2px solid var(--border); }
  .children .card { background: transparent; border: none; padding: 0.5rem 0; }
  .live-indicator { display: inline-flex; align-items: center; gap: 0.375rem; font-size: 0.75rem; color: var(--muted); }
  .live-dot { width: 0.5rem; height: 0.5rem; border-radius: 50%; background: #22c55e; animation: pulse 2s infinite; }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.5; } }
  footer { margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--border); color: var(--muted); font-size: 0.8rem; }
  #portal-content { min-height: 10rem; }
</style>
`;

const ObjectCard: FC<{ obj: PortalObject; depth: number }> = ({ obj, depth }) => {
  const HeadingTag = depth === 0 ? "h2" : "h3";
  return (
    <div class="card" data-id={obj.id} data-type={obj.type}>
      <HeadingTag>{obj.name}</HeadingTag>
      {obj.description && <p class="card-desc">{obj.description}</p>}
      <div class="card-meta">
        <span class="badge badge-type">{obj.type}</span>
        {obj.status && <span class="badge badge-status">{obj.status}</span>}
        {obj.tags.map((t) => <span class="badge badge-tag">{t}</span>)}
        {obj.date && <time datetime={obj.date}>{obj.date}</time>}
      </div>
      {obj.children.length > 0 && (
        <div class="children">
          {obj.children.map((child) => <ObjectCard obj={child} depth={depth + 1} />)}
        </div>
      )}
    </div>
  );
};

const PortalPage: FC<{ snapshot: PortalSnapshot; wsUrl?: string }> = ({ snapshot, wsUrl }) => {
  const { portal, objects, objectCount, generatedAt } = snapshot;
  const isLive = portal.level >= 2 && wsUrl;

  return (
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>{portal.name}</title>
        <meta name="generator" content="Prism Sovereign Portal" />
        <meta name="description" content={`${portal.name} — ${objectCount} objects`} />
        <PortalStyles />
      </head>
      <body>
        <div class="container">
          <header>
            <h1>{portal.name}</h1>
            <p class="meta">
              {objectCount} objects &middot; Level {portal.level} portal
              {isLive && (
                <span class="live-indicator">
                  &middot; <span class="live-dot" /> Live
                </span>
              )}
            </p>
          </header>
          <main id="portal-content">
            <div class="object-grid">
              {objects.map((obj) => <ObjectCard obj={obj} depth={0} />)}
            </div>
          </main>
          <footer>
            <p>Powered by Prism Sovereign Portal &middot; Generated {generatedAt}</p>
          </footer>
        </div>
        {isLive && <LiveUpdateScript wsUrl={wsUrl} portalId={portal.portalId} collectionId={portal.collectionId} />}
      </body>
    </html>
  );
};

const LiveUpdateScript: FC<{ wsUrl: string; portalId: string; collectionId: string }> = ({ wsUrl, portalId, collectionId }) => html`
<script>
(function() {
  var wsUrl = '${wsUrl}';
  var portalId = '${portalId}';
  var collectionId = '${collectionId}';
  var ws = null;
  var reconnectDelay = 1000;

  function connect() {
    ws = new WebSocket(wsUrl);
    ws.onopen = function() {
      reconnectDelay = 1000;
      ws.send(JSON.stringify({ type: 'auth', did: 'did:key:portal-viewer-' + portalId }));
    };
    ws.onmessage = function(evt) {
      var msg;
      try { msg = JSON.parse(evt.data); } catch(e) { return; }
      if (msg.type === 'auth-ok') {
        ws.send(JSON.stringify({ type: 'sync-request', collectionId: collectionId }));
      } else if (msg.type === 'sync-snapshot' || msg.type === 'sync-update') {
        // Reload the page to get fresh SSR content
        // Future: incremental DOM patching
        window.location.reload();
      }
    };
    ws.onclose = function() {
      setTimeout(connect, reconnectDelay);
      reconnectDelay = Math.min(reconnectDelay * 2, 30000);
    };
  }

  connect();
})();
</script>
`;

// ── Route Factory ──────────────────────────────────────────────────────────

export function createPortalViewRoutes(relay: RelayInstance, wsBaseUrl?: string): Hono {
  const app = new Hono();

  function getRegistry(): PortalRegistry | undefined {
    return relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
  }

  function getCollections(): CollectionHost | undefined {
    return relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
  }

  // List all public portals
  app.get("/", (c) => {
    const registry = getRegistry();
    if (!registry) return c.text("Portals not available", 404);

    const portals = registry.list().filter((p) => p.isPublic);
    return c.html(
      <html lang="en">
        <head>
          <meta charset="utf-8" />
          <meta name="viewport" content="width=device-width, initial-scale=1" />
          <title>Portals</title>
          <PortalStyles />
        </head>
        <body>
          <div class="container">
            <header>
              <h1>Sovereign Portals</h1>
              <p class="meta">{portals.length} public portals</p>
            </header>
            <main>
              <div class="object-grid">
                {portals.map((p) => (
                  <a href={`/portals/${p.portalId}`} style="text-decoration: none; color: inherit;">
                    <div class="card">
                      <h2>{p.name}</h2>
                      <div class="card-meta">
                        <span class="badge badge-type">Level {p.level}</span>
                        <time datetime={p.createdAt}>{p.createdAt}</time>
                      </div>
                    </div>
                  </a>
                ))}
                {portals.length === 0 && <p class="meta">No public portals registered.</p>}
              </div>
            </main>
            <footer>
              <p>Powered by Prism Sovereign Portal</p>
            </footer>
          </div>
        </body>
      </html>,
    );
  });

  // View a specific portal
  app.get("/:id", (c) => {
    const registry = getRegistry();
    const collections = getCollections();
    if (!registry) return c.text("Portals not available", 404);

    const portal = registry.get(c.req.param("id"));
    if (!portal) return c.text("Portal not found", 404);

    // For non-public portals, we'd check capability tokens here (Level 3+)
    if (!portal.isPublic) {
      return c.text("Portal is not public", 403);
    }

    if (!collections) {
      return c.text("Collection hosting not available", 503);
    }

    const store = collections.get(portal.collectionId);
    if (!store) {
      return c.text("Collection not found", 404);
    }

    const snapshot = extractPortalSnapshot(portal, store);

    // Build WS URL for live portals
    if (portal.level >= 2 && wsBaseUrl) {
      const wsUrl = `${wsBaseUrl}/ws/relay`;
      return c.html(<PortalPage snapshot={snapshot} wsUrl={wsUrl} />);
    }

    return c.html(<PortalPage snapshot={snapshot} />);
  });

  // Raw JSON snapshot (for API consumers)
  app.get("/:id/snapshot.json", (c) => {
    const registry = getRegistry();
    const collections = getCollections();
    if (!registry) return c.json({ error: "portals not available" }, 404);

    const portal = registry.get(c.req.param("id"));
    if (!portal) return c.json({ error: "portal not found" }, 404);
    if (!portal.isPublic) return c.json({ error: "portal is not public" }, 403);
    if (!collections) return c.json({ error: "collections not available" }, 503);

    const store = collections.get(portal.collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const snapshot = extractPortalSnapshot(portal, store);
    return c.json(snapshot);
  });

  return app;
}
