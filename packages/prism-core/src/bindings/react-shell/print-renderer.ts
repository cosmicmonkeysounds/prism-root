/**
 * Print renderer — wraps report HTML in a full document with @page CSS
 * derived from a PrintConfig, and optionally triggers the browser's
 * native print dialog via a hidden iframe.
 *
 * Pure TS (no React). Used by ReportSurface and anywhere else that
 * needs to emit printable output.
 */

import type { PrintConfig, PageSize } from "@prism/core/facet";

// Page dimensions in points (1/72 inch). Width × height in portrait.
const PAGE_SIZES: Record<Exclude<PageSize, "custom">, { w: number; h: number }> = {
  letter: { w: 612, h: 792 },
  legal: { w: 612, h: 1008 },
  a4: { w: 595, h: 842 },
  a3: { w: 842, h: 1191 },
};

/** Compute effective page dimensions in points. */
export function resolvePageSize(config: PrintConfig): { width: number; height: number } {
  if (config.pageSize === "custom") {
    return {
      width: config.customWidth ?? 612,
      height: config.customHeight ?? 792,
    };
  }
  const { w, h } = PAGE_SIZES[config.pageSize];
  return { width: w, height: h };
}

/** Build the @page CSS rule for this config. */
export function buildPageCss(config: PrintConfig): string {
  const { width, height } = resolvePageSize(config);
  const orientation = config.orientation ?? "portrait";
  const m = config.margins ?? { top: 36, right: 36, bottom: 36, left: 36 };

  const [w, h] =
    orientation === "landscape" ? [height, width] : [width, height];

  const headerRule = config.pageHeader
    ? `@top-center { content: "${escapeCssString(config.pageHeader)}"; }`
    : "";
  const footerRule = config.pageFooter
    ? `@bottom-center { content: "${escapeCssString(config.pageFooter)}"; }`
    : "";

  const pageNumberPos = config.pageNumberPosition ?? "bottom-center";
  const pageNumberRule = config.showPageNumbers
    ? `@${pageNumberBox(pageNumberPos)} { content: counter(page); }`
    : "";

  return `
    @page {
      size: ${w}pt ${h}pt;
      margin: ${m.top}pt ${m.right}pt ${m.bottom}pt ${m.left}pt;
      ${headerRule}
      ${footerRule}
      ${pageNumberRule}
    }
  `.trim();
}

function pageNumberBox(pos: string): string {
  return `${pos.replace("-", "-")}` // keep position tokens unchanged for margin box
    ;
}

function escapeCssString(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

/**
 * Wrap body HTML in a full <html>/<head>/<body> document with the page
 * CSS for this print config applied.
 */
export function renderForPrint(body: string, config: PrintConfig): string {
  const pageCss = buildPageCss(config);
  const groupBreakCss: string[] = [];
  if (config.pageBreakBeforeGroup) {
    groupBreakCss.push(".report-group { page-break-before: always; }");
  }
  if (config.pageBreakAfterGroup) {
    groupBreakCss.push(".report-group { page-break-after: always; }");
  }

  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  ${pageCss}
  body { font-family: system-ui, -apple-system, sans-serif; font-size: 11pt; color: #111; }
  h1, h2, h3 { margin: 0 0 8pt 0; }
  .report-title { font-size: 18pt; font-weight: 600; margin-bottom: 12pt; }
  .report-group { margin-bottom: 16pt; }
  .report-group-header { font-size: 13pt; font-weight: 600; background: #f3f3f3; padding: 4pt 6pt; }
  .report-row { display: flex; gap: 8pt; padding: 3pt 6pt; border-bottom: 0.5pt solid #ddd; }
  .report-summary { padding: 4pt 6pt; font-style: italic; background: #fafafa; border-top: 0.5pt solid #aaa; }
  .report-grand-total { margin-top: 12pt; padding: 6pt; font-weight: 600; border-top: 1pt solid #000; }
  ${groupBreakCss.join("\n")}
</style>
</head>
<body>
${body}
</body>
</html>`;
}

/**
 * Fire the browser's native print dialog. Creates a hidden iframe,
 * writes the full document, calls print(), and removes the iframe
 * after a short delay. No-op if not in a browser environment.
 */
export function triggerBrowserPrint(body: string, config: PrintConfig): void {
  if (typeof document === "undefined") return;

  const html = renderForPrint(body, config);
  const iframe = document.createElement("iframe");
  iframe.style.position = "fixed";
  iframe.style.right = "0";
  iframe.style.bottom = "0";
  iframe.style.width = "0";
  iframe.style.height = "0";
  iframe.style.border = "0";
  document.body.appendChild(iframe);

  const idoc = iframe.contentWindow?.document;
  if (!idoc) {
    document.body.removeChild(iframe);
    return;
  }

  idoc.open();
  idoc.write(html);
  idoc.close();

  // Defer print until the iframe has settled, then clean up.
  const win = iframe.contentWindow;
  if (!win) return;

  const fire = () => {
    try {
      win.focus();
      win.print();
    } finally {
      setTimeout(() => {
        if (iframe.parentNode) iframe.parentNode.removeChild(iframe);
      }, 500);
    }
  };

  if (idoc.readyState === "complete") {
    fire();
  } else {
    win.addEventListener("load", fire, { once: true });
  }
}
