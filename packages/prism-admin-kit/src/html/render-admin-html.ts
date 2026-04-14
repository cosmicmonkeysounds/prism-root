/**
 * renderAdminHtml — generates a self-contained HTML admin dashboard.
 *
 * The returned HTML includes inline CSS + JS that polls a JSON endpoint
 * for an `AdminSnapshot` and renders the same widgets the React admin-kit
 * provides, but without any React/Puck dependency. This makes the dashboard
 * embeddable in any server (Relay's Hono, Daemon's axum, etc.) as a single
 * GET route.
 *
 * The page auto-refreshes every `pollMs` milliseconds (default 5 000).
 */

import type { AdminSnapshot, HealthLevel } from "../types.js";
import {
  formatUptime,
  formatMetricValue,
  formatRelativeTime,
  HEALTH_COLORS,
} from "../admin-helpers.js";

export interface RenderAdminHtmlOptions {
  /** Page `<title>`. */
  title?: string;
  /** Runtime label shown in the header. */
  runtimeLabel?: string;
  /** Relative URL the page's JS fetches for the snapshot JSON. */
  snapshotUrl?: string;
  /** Auto-refresh interval in ms. 0 to disable. Default 5000. */
  pollMs?: number;
  /** Optional initial snapshot to embed as SSR seed data. */
  initialSnapshot?: AdminSnapshot;
}

/* ------------------------------------------------------------------ */
/*  CSS                                                                */
/* ------------------------------------------------------------------ */

const CSS = /* css */ `
*,*::before,*::after{box-sizing:border-box}
body{margin:0;font-family:system-ui,-apple-system,sans-serif;background:#1e1e1e;color:#e5e5e5}
.admin-shell{display:flex;flex-direction:column;min-height:100vh}
.admin-header{display:flex;align-items:center;gap:12px;padding:10px 16px;background:#0f172a;border-bottom:1px solid #1e293b;flex:0 0 auto}
.admin-header h1{font-size:14px;font-weight:600;margin:0;color:#e2e8f0}
.admin-header .runtime{font-size:11px;color:#94a3b8;text-transform:uppercase;letter-spacing:.08em}
.admin-header .refresh-info{margin-left:auto;font-size:11px;color:#64748b}
.admin-body{padding:16px;display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:12px;flex:1}
.card{background:#252526;border:1px solid #333;border-radius:8px;padding:14px 16px}
.card-title{font-size:11px;text-transform:uppercase;letter-spacing:.08em;color:#888;margin-bottom:8px;font-weight:600}
.metric-value{font-size:28px;font-weight:600;color:#e5e5e5;line-height:1.1}
.metric-hint{font-size:12px;color:#888;margin-top:4px}
.metric-delta{font-size:11px;margin-left:6px}
.metric-delta.pos{color:#4ade80}
.metric-delta.neg{color:#f87171}
.health-badge{display:inline-flex;align-items:center;gap:6px;font-size:11px;padding:4px 10px;border-radius:999px;border:1px solid;font-weight:500}
.service-row{display:flex;align-items:center;justify-content:space-between;padding:6px 0;border-bottom:1px solid #333;font-size:13px}
.service-row:last-child{border-bottom:none}
.service-dot{width:8px;height:8px;border-radius:50%;display:inline-block;margin-right:8px}
.activity-row{display:flex;align-items:baseline;gap:8px;padding:4px 0;font-size:12px;border-bottom:1px solid #2a2a2a}
.activity-row:last-child{border-bottom:none}
.activity-kind{font-family:ui-monospace,monospace;color:#6366f1;font-size:11px;min-width:120px}
.activity-time{font-family:ui-monospace,monospace;color:#555;font-size:11px;min-width:60px;text-align:right}
.activity-msg{color:#ccc;flex:1}
.uptime-value{font-size:20px;font-weight:600;color:#e5e5e5}
.source-header{grid-column:1/-1;display:flex;align-items:center;gap:12px;padding:4px 0}
.source-header h2{font-size:16px;font-weight:600;margin:0;color:#e5e5e5}
.empty{font-size:12px;color:#555;font-style:italic;text-align:center;padding:12px}
.error-banner{background:#3b1111;border:1px solid #7f1d1d;color:#f87171;padding:10px 16px;font-size:13px;text-align:center}
.services-card,.activity-card{grid-column:span 2}
@media(max-width:640px){.services-card,.activity-card{grid-column:span 1}}
`;

/* ------------------------------------------------------------------ */
/*  HTML widget helpers                                                */
/* ------------------------------------------------------------------ */

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function renderHealthBadge(level: HealthLevel, label: string): string {
  const c = HEALTH_COLORS[level];
  return `<span class="health-badge" style="background:${c.bg};color:${c.fg};border-color:${c.border}">${esc(label)}</span>`;
}

export function renderMetricCard(metric: { id: string; label: string; value: number | string; unit?: string; hint?: string; delta?: number }): string {
  const formatted = formatMetricValue(metric);
  let deltaHtml = "";
  if (typeof metric.delta === "number" && metric.delta !== 0) {
    const cls = metric.delta > 0 ? "pos" : "neg";
    const sign = metric.delta > 0 ? "+" : "";
    deltaHtml = `<span class="metric-delta ${cls}">${sign}${metric.delta}</span>`;
  }
  const hintHtml = metric.hint ? `<div class="metric-hint">${esc(metric.hint)}</div>` : "";
  return `<div class="card"><div class="card-title">${esc(metric.label)}</div><div class="metric-value">${esc(formatted)}${deltaHtml}</div>${hintHtml}</div>`;
}

export function renderUptimeCard(seconds: number): string {
  return `<div class="card"><div class="card-title">Uptime</div><div class="uptime-value">${esc(formatUptime(seconds))}</div></div>`;
}

export function renderServiceList(services: Array<{ id: string; name: string; health: HealthLevel; status?: string }>): string {
  if (services.length === 0) return `<div class="card services-card"><div class="card-title">Services</div><div class="empty">No services reported</div></div>`;
  const rows = services.map((s) => {
    const c = HEALTH_COLORS[s.health];
    const statusText = s.status ? `<span style="color:#888;font-size:11px">${esc(s.status)}</span>` : "";
    return `<div class="service-row"><span><span class="service-dot" style="background:${c.fg}"></span>${esc(s.name)}</span>${statusText}</div>`;
  }).join("");
  return `<div class="card services-card"><div class="card-title">Services</div>${rows}</div>`;
}

export function renderActivityTail(items: Array<{ id: string; timestamp: string; kind: string; message: string }>, now: number): string {
  if (items.length === 0) return `<div class="card activity-card"><div class="card-title">Activity</div><div class="empty">No recent activity</div></div>`;
  const rows = items.slice(0, 20).map((a) => {
    const rel = formatRelativeTime(a.timestamp, now);
    return `<div class="activity-row"><span class="activity-kind">${esc(a.kind)}</span><span class="activity-msg">${esc(a.message)}</span><span class="activity-time">${esc(rel)}</span></div>`;
  }).join("");
  return `<div class="card activity-card"><div class="card-title">Activity</div>${rows}</div>`;
}

/* ------------------------------------------------------------------ */
/*  Full-page render                                                   */
/* ------------------------------------------------------------------ */

export function renderSnapshotBody(snap: AdminSnapshot, now: number): string {
  const parts: string[] = [];

  // Source header + health
  parts.push(`<div class="source-header"><h2>${esc(snap.sourceLabel)}</h2>${renderHealthBadge(snap.health.level, snap.health.label)}</div>`);

  // Uptime
  parts.push(renderUptimeCard(snap.uptimeSeconds));

  // Metrics
  for (const m of snap.metrics) {
    parts.push(renderMetricCard(m));
  }

  // Services
  parts.push(renderServiceList(snap.services));

  // Activity
  parts.push(renderActivityTail(snap.activity, now));

  return parts.join("\n");
}

export function renderAdminHtml(options: RenderAdminHtmlOptions = {}): string {
  const {
    title = "Prism Admin",
    runtimeLabel = "Prism Runtime",
    snapshotUrl = "/admin/api/snapshot",
    pollMs = 5000,
    initialSnapshot,
  } = options;

  const seedJson = initialSnapshot
    ? JSON.stringify(initialSnapshot).replace(/<\//g, "<\\/")
    : "null";

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(title)}</title>
<style>${CSS}</style>
</head>
<body>
<div class="admin-shell">
  <div class="admin-header">
    <h1>${esc(title)}</h1>
    <span class="runtime">${esc(runtimeLabel)}</span>
    <span class="refresh-info" id="refresh-info"></span>
  </div>
  <div id="error-slot"></div>
  <div class="admin-body" id="dashboard"></div>
</div>
<script>
(function(){
  var POLL_MS = ${pollMs};
  var SNAPSHOT_URL = ${JSON.stringify(snapshotUrl)};
  var seed = ${seedJson};

  var healthColors = ${JSON.stringify(HEALTH_COLORS)};

  function esc(s){return String(s).replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;")}

  function fmtUptime(s){
    if(!isFinite(s)||s<0)return "\\u2014";
    s=Math.floor(s);
    if(s<60)return s+"s";
    var m=Math.floor(s/60);
    if(m<60)return m+"m "+(s%60)+"s";
    var h=Math.floor(m/60);
    if(h<24)return h+"h "+(m%60)+"m";
    var d=Math.floor(h/24);
    return d+"d "+(h%24)+"h";
  }
  function fmtMetric(v,unit){
    if(typeof v==="string")return v;
    var a=Math.abs(v),r;
    if(a>=1e6)r=(v/1e6).toFixed(1)+"M";
    else if(a>=1e3)r=(v/1e3).toFixed(1)+"k";
    else if(Number.isInteger(v))r=""+v;
    else r=v.toFixed(2);
    return unit?r+unit:r;
  }
  function fmtRel(iso,now){
    var t=Date.parse(iso);if(isNaN(t))return "\\u2014";
    var d=Math.max(0,Math.floor((now-t)/1000));
    if(d<5)return "just now";
    if(d<60)return d+"s ago";
    var m=Math.floor(d/60);if(m<60)return m+"m ago";
    var h=Math.floor(m/60);if(h<24)return h+"h ago";
    return Math.floor(h/24)+"d ago";
  }

  function renderDashboard(snap){
    var now=Date.now();
    var hc=healthColors[snap.health.level]||healthColors.unknown;
    var html='<div class="source-header"><h2>'+esc(snap.sourceLabel)+'</h2>'
      +'<span class="health-badge" style="background:'+hc.bg+';color:'+hc.fg+';border-color:'+hc.border+'">'+esc(snap.health.label)+'</span></div>';
    html+='<div class="card"><div class="card-title">Uptime</div><div class="uptime-value">'+esc(fmtUptime(snap.uptimeSeconds))+'</div></div>';
    for(var i=0;i<snap.metrics.length;i++){
      var m=snap.metrics[i];
      var dh="";
      if(typeof m.delta==="number"&&m.delta!==0){
        dh='<span class="metric-delta '+(m.delta>0?"pos":"neg")+'">'+(m.delta>0?"+":"")+m.delta+'</span>';
      }
      var hint=m.hint?'<div class="metric-hint">'+esc(m.hint)+'</div>':"";
      html+='<div class="card"><div class="card-title">'+esc(m.label)+'</div><div class="metric-value">'+esc(fmtMetric(m.value,m.unit))+dh+'</div>'+hint+'</div>';
    }
    if(snap.services.length===0){
      html+='<div class="card services-card"><div class="card-title">Services</div><div class="empty">No services reported</div></div>';
    }else{
      var rows="";
      for(var j=0;j<snap.services.length;j++){
        var s=snap.services[j];
        var sc=healthColors[s.health]||healthColors.unknown;
        var st=s.status?'<span style="color:#888;font-size:11px">'+esc(s.status)+'</span>':"";
        rows+='<div class="service-row"><span><span class="service-dot" style="background:'+sc.fg+'"></span>'+esc(s.name)+'</span>'+st+'</div>';
      }
      html+='<div class="card services-card"><div class="card-title">Services</div>'+rows+'</div>';
    }
    if(snap.activity.length===0){
      html+='<div class="card activity-card"><div class="card-title">Activity</div><div class="empty">No recent activity</div></div>';
    }else{
      var arows="";
      for(var k=0;k<Math.min(snap.activity.length,20);k++){
        var a=snap.activity[k];
        arows+='<div class="activity-row"><span class="activity-kind">'+esc(a.kind)+'</span><span class="activity-msg">'+esc(a.message)+'</span><span class="activity-time">'+esc(fmtRel(a.timestamp,now))+'</span></div>';
      }
      html+='<div class="card activity-card"><div class="card-title">Activity</div>'+arows+'</div>';
    }
    document.getElementById("dashboard").innerHTML=html;
  }

  function showError(msg){
    document.getElementById("error-slot").innerHTML='<div class="error-banner">'+esc(msg)+'</div>';
  }
  function clearError(){document.getElementById("error-slot").innerHTML="";}

  function fetchAndRender(){
    fetch(SNAPSHOT_URL).then(function(r){
      if(!r.ok)throw new Error("HTTP "+r.status);
      return r.json();
    }).then(function(snap){
      clearError();
      renderDashboard(snap);
      var info=document.getElementById("refresh-info");
      if(info)info.textContent="Last refresh: "+new Date().toLocaleTimeString();
    }).catch(function(e){
      showError("Failed to fetch admin data: "+e.message);
    });
  }

  if(seed){renderDashboard(seed);}
  fetchAndRender();
  if(POLL_MS>0){setInterval(fetchAndRender,POLL_MS);}
})();
</script>
</body>
</html>`;
}
