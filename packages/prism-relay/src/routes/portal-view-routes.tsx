/** @jsxImportSource hono/jsx */

/**
 * Portal View Routes — serves Sovereign Portal HTML pages.
 *
 * Level 1: Static read-only HTML snapshot
 * Level 2: HTML with incremental DOM patching via WebSocket
 * Level 3: Interactive forms with ephemeral DID auth + capability tokens
 * Level 4: Complex webapps with full client-side hydration + bidirectional CRDT sync
 *
 * These routes serve rendered pages at /portals/:id, separate from
 * the JSON API at /api/portals.
 */

import { Hono } from "hono";
import type { FC } from "hono/jsx";
import { html } from "hono/html";
import type {
  RelayInstance,
  PortalRegistry,
  CollectionHost,
  CapabilityTokenManager,
  PortalManifest,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES, extractPortalSnapshot } from "@prism/core/relay";
import type { PortalSnapshot, PortalObject } from "@prism/core/relay";
import { objectId as toObjectId } from "@prism/core/object-model";
import { deserializeToken } from "./token-routes.js";

// ── JSX Components ─────────────────────────────────────────────────────────

const PortalStyles: FC = () => html`
<style>
  :root { --bg: #fff; --fg: #1a1a1a; --muted: #666; --accent: #2563eb; --border: #e5e7eb; --card-bg: #fafafa; --success: #16a34a; --error: #dc2626; }
  @media (prefers-color-scheme: dark) {
    :root { --bg: #0a0a0a; --fg: #e5e5e5; --muted: #999; --accent: #60a5fa; --border: #333; --card-bg: #111; --success: #22c55e; --error: #ef4444; }
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
  .portal-form { margin-top: 2rem; padding: 1.5rem; border: 1px solid var(--border); border-radius: 0.5rem; background: var(--card-bg); }
  .portal-form h2 { font-size: 1.25rem; margin-bottom: 1rem; }
  .form-field { margin-bottom: 1rem; }
  .form-field label { display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem; }
  .form-field input, .form-field textarea, .form-field select { width: 100%; padding: 0.5rem; border: 1px solid var(--border); border-radius: 0.375rem; background: var(--bg); color: var(--fg); font-size: 0.875rem; font-family: inherit; }
  .form-field textarea { min-height: 4rem; resize: vertical; }
  .form-submit { padding: 0.5rem 1.5rem; background: var(--accent); color: white; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; font-weight: 500; }
  .form-submit:hover { opacity: 0.9; }
  .form-submit:disabled { opacity: 0.5; cursor: not-allowed; }
  .form-status { margin-top: 0.75rem; font-size: 0.875rem; padding: 0.5rem; border-radius: 0.375rem; }
  .form-status.success { background: color-mix(in srgb, var(--success) 15%, transparent); color: var(--success); }
  .form-status.error { background: color-mix(in srgb, var(--error) 15%, transparent); color: var(--error); }
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

const PortalPage: FC<{ snapshot: PortalSnapshot; wsUrl?: string | undefined; showForm?: boolean | undefined }> = ({ snapshot, wsUrl, showForm }) => {
  const { portal, objects, objectCount, generatedAt } = snapshot;
  const isLive = portal.level >= 2 && wsUrl;
  const isInteractive = portal.level >= 3;
  const isApp = portal.level >= 4;

  return (
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>{portal.name}</title>
        <meta name="generator" content="Prism Sovereign Portal" />
        <meta name="description" content={`${portal.name} — ${objectCount} objects`} />
        {/* OpenGraph */}
        <meta property="og:title" content={portal.name} />
        <meta property="og:description" content={`${portal.name} — ${objectCount} objects. Level ${portal.level} Sovereign Portal.`} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="Prism Sovereign Portal" />
        {/* Twitter Card */}
        <meta name="twitter:card" content="summary" />
        <meta name="twitter:title" content={portal.name} />
        <meta name="twitter:description" content={`${portal.name} — ${objectCount} objects`} />
        {/* Structured Data */}
        <script type="application/ld+json">{JSON.stringify({
          "@context": "https://schema.org",
          "@type": "WebPage",
          name: portal.name,
          description: `${portal.name} — ${objectCount} objects`,
          dateCreated: portal.createdAt,
          dateModified: generatedAt,
        })}</script>
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
              {isInteractive && " \u00b7 Interactive"}
              {isApp && " \u00b7 App"}
            </p>
          </header>
          <main id="portal-content">
            <div class="object-grid">
              {objects.map((obj) => <ObjectCard obj={obj} depth={0} />)}
            </div>
            {showForm && isInteractive && <PortalFormSection portalId={portal.portalId} />}
          </main>
          <footer>
            <p>Powered by Prism Sovereign Portal &middot; Generated {generatedAt}</p>
          </footer>
        </div>
        {isLive && <LiveUpdateScript wsUrl={wsUrl} portalId={portal.portalId} collectionId={portal.collectionId} />}
        {isInteractive && <FormScript portalId={portal.portalId} />}
        {isApp && wsUrl && <HydrationScript wsUrl={wsUrl} portalId={portal.portalId} collectionId={portal.collectionId} />}
      </body>
    </html>
  );
};

// ── Level 3: Interactive Form Section ─────────────────────────────────────

const PortalFormSection: FC<{ portalId: string }> = ({ portalId }) => (
  <div class="portal-form" id="portal-form">
    <h2>Submit Data</h2>
    <form id="portal-submit-form" data-portal-id={portalId}>
      <div class="form-field">
        <label for="field-name">Name</label>
        <input type="text" id="field-name" name="name" required />
      </div>
      <div class="form-field">
        <label for="field-type">Type</label>
        <input type="text" id="field-type" name="type" value="submission" />
      </div>
      <div class="form-field">
        <label for="field-description">Description</label>
        <textarea id="field-description" name="description" />
      </div>
      <button type="submit" class="form-submit">Submit</button>
      <div id="form-status" class="form-status" style="display:none;" />
    </form>
  </div>
);

// ── Level 3: Form Submission Script ───────────────────────────────────────

const FormScript: FC<{ portalId: string }> = ({ portalId }) => html`
<script>
(function() {
  var form = document.getElementById('portal-submit-form');
  if (!form) return;

  form.addEventListener('submit', function(e) {
    e.preventDefault();
    var status = document.getElementById('form-status');
    var btn = form.querySelector('.form-submit');
    btn.disabled = true;
    status.style.display = 'none';

    var data = {};
    var inputs = form.querySelectorAll('input, textarea, select');
    for (var i = 0; i < inputs.length; i++) {
      var input = inputs[i];
      if (input.name) data[input.name] = input.value;
    }

    fetch('/portals/${portalId}/submit', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    })
    .then(function(res) { return res.json().then(function(body) { return { ok: res.ok, body: body }; }); })
    .then(function(result) {
      status.style.display = 'block';
      if (result.ok) {
        status.className = 'form-status success';
        status.textContent = 'Submitted successfully.';
        form.reset();
      } else {
        status.className = 'form-status error';
        status.textContent = result.body.error || 'Submission failed.';
      }
      btn.disabled = false;
    })
    .catch(function() {
      status.style.display = 'block';
      status.className = 'form-status error';
      status.textContent = 'Network error. Please try again.';
      btn.disabled = false;
    });
  });
})();
</script>
`;

// ── Level 2+: Incremental DOM Patching Script ─────────────────────────────

const LiveUpdateScript: FC<{ wsUrl: string; portalId: string; collectionId: string }> = ({ wsUrl, portalId, collectionId }) => html`
<script>
(function() {
  var wsUrl = '${wsUrl}';
  var portalId = '${portalId}';
  var collectionId = '${collectionId}';
  var ws = null;
  var reconnectDelay = 1000;
  var updating = false;

  function patchPortalContent() {
    if (updating) return;
    updating = true;
    fetch('/portals/' + portalId + '/snapshot.json')
      .then(function(res) { return res.json(); })
      .then(function(snapshot) {
        var container = document.getElementById('portal-content');
        if (!container) { updating = false; return; }

        var grid = container.querySelector('.object-grid');
        if (!grid) { updating = false; return; }

        var newHtml = '';
        for (var i = 0; i < snapshot.objects.length; i++) {
          newHtml += renderObject(snapshot.objects[i], 0);
        }
        grid.innerHTML = newHtml;

        var meta = document.querySelector('.meta');
        if (meta) {
          meta.innerHTML = meta.innerHTML.replace(/\\d+ objects/, snapshot.objectCount + ' objects');
        }

        updating = false;
      })
      .catch(function() { updating = false; });
  }

  function esc(s) { var d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

  function renderObject(obj, depth) {
    var tag = depth === 0 ? 'h2' : 'h3';
    var h = '<div class="card" data-id="' + esc(obj.id) + '" data-type="' + esc(obj.type) + '">';
    h += '<' + tag + '>' + esc(obj.name) + '</' + tag + '>';
    if (obj.description) h += '<p class="card-desc">' + esc(obj.description) + '</p>';
    h += '<div class="card-meta"><span class="badge badge-type">' + esc(obj.type) + '</span>';
    if (obj.status) h += '<span class="badge badge-status">' + esc(obj.status) + '</span>';
    for (var i = 0; i < obj.tags.length; i++) h += '<span class="badge badge-tag">' + esc(obj.tags[i]) + '</span>';
    if (obj.date) h += '<time datetime="' + esc(obj.date) + '">' + esc(obj.date) + '</time>';
    h += '</div>';
    if (obj.children && obj.children.length > 0) {
      h += '<div class="children">';
      for (var j = 0; j < obj.children.length; j++) h += renderObject(obj.children[j], depth + 1);
      h += '</div>';
    }
    h += '</div>';
    return h;
  }

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
        patchPortalContent();
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

// ── Level 4: Full Client-Side Hydration Script ────────────────────────────

const HydrationScript: FC<{ wsUrl: string; portalId: string; collectionId: string }> = ({ wsUrl, portalId, collectionId }) => html`
<script>
(function() {
  // Portal configuration exposed to client-side apps
  window.__PRISM_PORTAL__ = {
    portalId: '${portalId}',
    collectionId: '${collectionId}',
    wsUrl: '${wsUrl}',
    state: 'loading',
    objects: [],
    edges: [],
    listeners: [],

    subscribe: function(fn) {
      this.listeners.push(fn);
      return function() {
        var idx = window.__PRISM_PORTAL__.listeners.indexOf(fn);
        if (idx >= 0) window.__PRISM_PORTAL__.listeners.splice(idx, 1);
      };
    },

    notify: function() {
      for (var i = 0; i < this.listeners.length; i++) {
        try { this.listeners[i](this); } catch(_) { /* listener error */ }
      }
    },

    sendUpdate: function(collectionId, updateBase64) {
      if (this._ws && this._ws.readyState === 1) {
        this._ws.send(JSON.stringify({ type: 'sync-update', collectionId: collectionId, update: updateBase64 }));
      }
    },

    submitObject: function(data) {
      return fetch('/portals/${portalId}/submit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }).then(function(r) { return r.json(); });
    }
  };

  var portal = window.__PRISM_PORTAL__;
  var ws = null;
  var reconnectDelay = 1000;

  function connect() {
    ws = new WebSocket('${wsUrl}');
    portal._ws = ws;

    ws.onopen = function() {
      reconnectDelay = 1000;
      ws.send(JSON.stringify({ type: 'auth', did: 'did:key:portal-app-' + '${portalId}' }));
    };

    ws.onmessage = function(evt) {
      var msg;
      try { msg = JSON.parse(evt.data); } catch(e) { return; }

      if (msg.type === 'auth-ok') {
        portal.state = 'connected';
        ws.send(JSON.stringify({ type: 'sync-request', collectionId: '${collectionId}' }));
        portal.notify();
      } else if (msg.type === 'sync-snapshot' || msg.type === 'sync-update') {
        // Fetch fresh snapshot and update portal state
        fetch('/portals/${portalId}/snapshot.json')
          .then(function(r) { return r.json(); })
          .then(function(snapshot) {
            portal.objects = snapshot.objects;
            portal.edges = snapshot.edges;
            portal.state = 'synced';
            portal.notify();
          });
      } else if (msg.type === 'error') {
        portal.state = 'error';
        portal.lastError = msg.message;
        portal.notify();
      }
    };

    ws.onclose = function() {
      portal.state = 'disconnected';
      portal._ws = null;
      portal.notify();
      setTimeout(connect, reconnectDelay);
      reconnectDelay = Math.min(reconnectDelay * 2, 30000);
    };
  }

  // Initial snapshot load
  fetch('/portals/${portalId}/snapshot.json')
    .then(function(r) { return r.json(); })
    .then(function(snapshot) {
      portal.objects = snapshot.objects;
      portal.edges = snapshot.edges;
      portal.state = 'loaded';
      portal.notify();
    });

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

  function getTokenManager(): CapabilityTokenManager | undefined {
    return relay.getCapability<CapabilityTokenManager>(RELAY_CAPABILITIES.TOKENS);
  }

  /** Verify capability token from Authorization header for Level 3+ portals. */
  async function verifyPortalAccess(
    portal: PortalManifest,
    authHeader: string | undefined,
  ): Promise<{ allowed: boolean; reason?: string }> {
    // Public portals are always accessible
    if (portal.isPublic) return { allowed: true };

    // Non-public Level 3+ portals require a valid capability token
    const tokenManager = getTokenManager();
    if (!tokenManager) {
      return { allowed: false, reason: "capability tokens module not installed" };
    }

    if (!authHeader || !authHeader.startsWith("Bearer ")) {
      return { allowed: false, reason: "missing authorization token" };
    }

    try {
      const tokenJson = JSON.parse(
        Buffer.from(authHeader.slice(7), "base64").toString("utf-8"),
      );
      const token = deserializeToken(tokenJson);
      const result = await tokenManager.verify(token);
      if (!result.valid) {
        return { allowed: false, reason: result.reason ?? "verification failed" };
      }

      // Check scope matches portal's accessScope
      if (portal.accessScope && token.scope !== portal.accessScope && token.scope !== "*") {
        return { allowed: false, reason: "token scope mismatch" };
      }

      return { allowed: true };
    } catch {
      return { allowed: false, reason: "invalid authorization token" };
    }
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
  app.get("/:id", async (c) => {
    const registry = getRegistry();
    const collections = getCollections();
    if (!registry) return c.text("Portals not available", 404);

    const portal = registry.get(c.req.param("id"));
    if (!portal) return c.text("Portal not found", 404);

    // Check access for non-public portals (Level 3+)
    const access = await verifyPortalAccess(portal, c.req.header("authorization"));
    if (!access.allowed) {
      return c.text(access.reason ?? "Portal is not public", 403);
    }

    if (!collections) {
      return c.text("Collection hosting not available", 503);
    }

    const store = collections.get(portal.collectionId);
    if (!store) {
      return c.text("Collection not found", 404);
    }

    const snapshot = extractPortalSnapshot(portal, store);

    // Build WS URL for live portals (Level 2+)
    const wsUrl = portal.level >= 2 && wsBaseUrl ? `${wsBaseUrl}/ws/relay` : undefined;

    return c.html(
      <PortalPage snapshot={snapshot} wsUrl={wsUrl} showForm={portal.level >= 3} />,
    );
  });

  // Raw JSON snapshot (for API consumers and incremental DOM patching)
  app.get("/:id/snapshot.json", async (c) => {
    const registry = getRegistry();
    const collections = getCollections();
    if (!registry) return c.json({ error: "portals not available" }, 404);

    const portal = registry.get(c.req.param("id"));
    if (!portal) return c.json({ error: "portal not found" }, 404);

    const access = await verifyPortalAccess(portal, c.req.header("authorization"));
    if (!access.allowed) {
      return c.json({ error: access.reason ?? "portal is not public" }, 403);
    }

    if (!collections) return c.json({ error: "collections not available" }, 503);

    const store = collections.get(portal.collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const snapshot = extractPortalSnapshot(portal, store);
    return c.json(snapshot);
  });

  // Level 3: Form submission — creates a new object in the portal's collection
  app.post("/:id/submit", async (c) => {
    const registry = getRegistry();
    const collections = getCollections();
    if (!registry) return c.json({ error: "portals not available" }, 404);

    const portal = registry.get(c.req.param("id"));
    if (!portal) return c.json({ error: "portal not found" }, 404);

    if (portal.level < 3) {
      return c.json({ error: "form submissions require Level 3+ portal" }, 400);
    }

    // Check access for non-public portals
    const access = await verifyPortalAccess(portal, c.req.header("authorization"));
    if (!access.allowed) {
      return c.json({ error: access.reason ?? "unauthorized" }, 403);
    }

    if (!collections) return c.json({ error: "collections not available" }, 503);

    const store = collections.get(portal.collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const body = await c.req.json<{
      name: string;
      type?: string;
      description?: string;
      data?: Record<string, unknown>;
    }>();

    if (!body.name || typeof body.name !== "string") {
      return c.json({ error: "name is required" }, 400);
    }

    // Generate ephemeral DID for the submitter
    const ephemeralDid = `did:key:ephemeral-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

    const now = new Date().toISOString();
    const submissionId = toObjectId(`sub-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`);

    store.putObject({
      id: submissionId,
      type: body.type ?? "submission",
      name: body.name,
      parentId: null,
      position: store.objectCount(),
      status: "submitted",
      tags: ["portal-submission"],
      date: now,
      endDate: null,
      description: body.description ?? "",
      color: null,
      image: null,
      pinned: false,
      data: {
        ...(body.data ?? {}),
        submittedBy: ephemeralDid,
        portalId: portal.portalId,
      },
      createdAt: now,
      updatedAt: now,
    });

    return c.json({
      ok: true,
      objectId: submissionId,
      ephemeralDid,
    }, 201);
  });

  return app;
}
