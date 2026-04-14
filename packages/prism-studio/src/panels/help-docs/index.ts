import appShell from "./app-shell.md?raw";
import columns from "./columns.md?raw";
import facetView from "./facet-view.md?raw";
import heading from "./heading.md?raw";
import image from "./image.md?raw";
import luauBlock from "./luau-block.md?raw";
import pageShell from "./page-shell.md?raw";
import recordList from "./record-list.md?raw";
import section from "./section.md?raw";
import siteFooter from "./site-footer.md?raw";
import siteHeader from "./site-header.md?raw";
import spatialCanvas from "./spatial-canvas.md?raw";

/**
 * Bundled map from docPath → markdown source.
 *
 * Studio ships its flagship help docs as Vite `?raw` imports so the full
 * text is baked into the client bundle. That means DocSheet works offline
 * in Tauri/Capacitor with no HTTP dependency, and the help surface stays
 * entirely in userland — no relay round-trips, no service worker.
 *
 * Doc paths are kebab-case without extensions; the HelpEntry records in
 * `puck-help-entries.ts` reference these exact keys via `docPath`.
 */
export const BUNDLED_HELP_DOCS: Readonly<Record<string, string>> = {
  "page-shell": pageShell,
  "app-shell": appShell,
  "site-header": siteHeader,
  "site-footer": siteFooter,
  section: section,
  columns: columns,
  heading: heading,
  image: image,
  "facet-view": facetView,
  "luau-block": luauBlock,
  "record-list": recordList,
  "spatial-canvas": spatialCanvas,
};

/**
 * Resolve a docPath to bundled markdown. Returns a rejected promise if the
 * path is unknown so DocSheet's existing error-state UI shows a readable
 * message instead of a blank panel.
 */
export function fetchHelpDoc(path: string): Promise<string> {
  const content = BUNDLED_HELP_DOCS[path];
  if (content) return Promise.resolve(content);
  return Promise.reject(
    new Error(`No bundled help doc for "${path}". Check puck-help-entries.ts.`),
  );
}
