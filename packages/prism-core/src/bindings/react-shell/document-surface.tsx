/**
 * DocumentSurface — unified multi-mode editor component for all file formats.
 *
 * Every text file in a Prism workspace opens through this component. It
 * auto-resolves the document type from the file extension, selects the
 * editing mode, and renders the appropriate surface:
 *
 *   Mode        | Engine     | What the user sees
 *   ────────────┼────────────┼──────────────────────────────────────────
 *   code        | CodeMirror | Raw syntax, full syntax highlighting
 *   preview     | CodeMirror | Live-preview: rendered on inactive lines
 *   form        | React      | Schema-driven field inputs (placeholder)
 *   spreadsheet | React      | Grid editing for tabular data (placeholder)
 *   report      | React      | Full HTML layout (placeholder)
 *
 * The underlying text string is ALWAYS the source of truth.
 *
 * Inline tokens from the document contribution are automatically applied
 * across all surfaces — [[links]], {expressions}, [operands] work
 * identically in CodeMirror code marks and preview chips.
 *
 * Usage:
 *   <DocumentSurface value={content} onChange={setContent} filePath="greeting.md" />
 *   <DocumentSurface value={content} onChange={setContent} filePath="scene.loom" mode="code" />
 *   <DocumentSurface value={md} onChange={setMd} documentType="prism:markdown" />
 */

import React, { useState, useMemo, useCallback } from "react";
import type { Extension } from "@codemirror/state";
import type {
  SurfaceMode,
  InlineTokenDef,
  DocumentContributionDef,
  DocumentSurfaceRegistry,
  DocumentSurfaceEntry,
} from "@prism/core/syntax";
import type { SpellChecker } from "@prism/core/syntax";
import type { FormSchema } from "@prism/core/forms";
import { useCodemirror } from "@prism/core/codemirror";
import { FormSurface } from "./form-surface.js";
import { CsvSurface } from "./csv-surface.js";
import { ReportSurface } from "./report-surface.js";
import { markdownLivePreview } from "@prism/core/codemirror";
import { spellCheckExtension } from "@prism/core/codemirror";
import { yamlLanguageSupport } from "@prism/core/codemirror";
import {
  createTokenMarkExtension,
  createTokenPreviewExtension,
  inlineTokenTheme,
} from "@prism/core/codemirror";
import type { LoroDoc, LoroText } from "loro-crdt";

// ── Mode labels ──────────────────────────────────────────────────────────────

const MODE_LABELS: Record<SurfaceMode, string> = {
  code: "Source",
  preview: "Preview",
  form: "Form",
  report: "Report",
  spreadsheet: "Spreadsheet",
};

// ── Custom Surface Props ─────────────────────────────────────────────────────

/**
 * Props passed to custom surface renderers.
 * Plugins implement this to provide completely custom document UIs.
 */
export interface CustomSurfaceProps {
  value: string;
  onChange: ((value: string) => void) | undefined;
  mode: SurfaceMode;
  filePath: string | undefined;
  readOnly: boolean | undefined;
  className: string | undefined;
}

// ── Toolbar ──────────────────────────────────────────────────────────────────

interface SurfaceToolbarProps {
  mode: SurfaceMode;
  availableModes: SurfaceMode[];
  displayName: string;
  onModeChange: (mode: SurfaceMode) => void;
}

function SurfaceToolbar({
  mode,
  availableModes,
  displayName,
  onModeChange,
}: SurfaceToolbarProps) {
  if (availableModes.length <= 1) return null;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        padding: "6px 12px",
        borderBottom: "1px solid rgba(255,255,255,0.08)",
        background: "rgba(9,9,11,0.6)",
        fontSize: 12,
        userSelect: "none",
      }}
    >
      <span style={{ color: "#71717a", marginRight: 8 }}>{displayName}</span>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          borderRadius: 6,
          background: "rgba(255,255,255,0.05)",
          padding: 2,
        }}
      >
        {availableModes.map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => onModeChange(m)}
            style={{
              padding: "4px 10px",
              borderRadius: 5,
              border: "none",
              fontSize: 11,
              fontWeight: 500,
              cursor: "pointer",
              transition: "all 0.15s",
              ...(mode === m
                ? {
                    background: "rgba(139,92,246,0.3)",
                    color: "rgb(196,181,253)",
                    boxShadow: "0 1px 2px rgba(0,0,0,0.2)",
                  }
                : {
                    background: "transparent",
                    color: "#71717a",
                  }),
            }}
          >
            {MODE_LABELS[m]}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── CodeMirror Surface ───────────────────────────────────────────────────────

interface CodeSurfaceProps {
  doc: LoroDoc;
  text: LoroText;
  extensions: Extension[];
  readOnly: boolean;
}

function CodeSurface({ doc, text, extensions, readOnly }: CodeSurfaceProps) {
  const { containerRef } = useCodemirror({ doc, text, extensions, readOnly });
  return (
    <div
      ref={containerRef}
      style={{ height: "100%", width: "100%", overflow: "auto" }}
    />
  );
}

function SvgPreview({ value }: { value: string }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        padding: 16,
        overflow: "auto",
        background: "#09090b",
      }}
    >
      <img
        src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(value)}`}
        alt="SVG preview"
        style={{ maxWidth: "100%", maxHeight: "100%" }}
      />
    </div>
  );
}

function MarkdownPreview({ value }: { value: string }) {
  return (
    <div
      style={{
        padding: 24,
        height: "100%",
        overflow: "auto",
        color: "#e4e4e7",
        background: "#09090b",
        lineHeight: 1.6,
      }}
      dangerouslySetInnerHTML={{ __html: simpleMarkdownToHtml(value) }}
    />
  );
}

/** Minimal markdown to HTML — just enough for preview. */
function simpleMarkdownToHtml(md: string): string {
  return md
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/^### (.+)$/gm, "<h3>$1</h3>")
    .replace(/^## (.+)$/gm, "<h2>$1</h2>")
    .replace(/^# (.+)$/gm, "<h1>$1</h1>")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    .replace(/`(.+?)`/g, "<code>$1</code>")
    .replace(/\n/g, "<br/>");
}

// ── Main component ───────────────────────────────────────────────────────────

export interface DocumentSurfaceProps {
  /** LoroDoc containing the text — CRDT source of truth. */
  doc: LoroDoc;
  /** The LoroText node to edit. */
  text: LoroText;
  /** Current plain-text value (for preview/form surfaces that need it). */
  value: string;
  /** Called on every content change with the new source text. */
  onChange?: (value: string) => void;
  /** File path — used to auto-resolve document type. */
  filePath?: string;
  /** Override: contribution ID to use directly. */
  documentType?: string;
  /** Override: force a specific editing mode. */
  mode?: SurfaceMode;
  /** Extra CodeMirror extensions (merged with registered ones). */
  extensions?: Extension[];
  /** Extra inline tokens (merged with contribution tokens). */
  inlineTokens?: InlineTokenDef[];
  readOnly?: boolean;
  className?: string;
  /** Hide the mode-switching toolbar. */
  hideToolbar?: boolean;
  /** Schema for form mode. */
  formSchema?: FormSchema;
  /** SpellChecker instance for spell checking in code/preview modes. */
  spellChecker?: SpellChecker;
  /** Document surface registry to resolve contributions from. */
  registry?: DocumentSurfaceRegistry;
  /** Custom surface renderers keyed by mode. */
  modeRenderers?: Partial<Record<SurfaceMode, React.ComponentType<CustomSurfaceProps>>>;
  /** Full custom renderer — replaces the default surface entirely. */
  customRenderer?: React.ComponentType<CustomSurfaceProps>;
}

export function DocumentSurface({
  doc,
  text,
  value,
  onChange,
  filePath,
  documentType,
  mode: forcedMode,
  extensions: extraExtensions = [],
  inlineTokens: extraTokens = [],
  readOnly = false,
  className,
  hideToolbar = false,
  formSchema,
  spellChecker,
  registry,
  modeRenderers,
  customRenderer: CustomRenderer,
}: DocumentSurfaceProps) {
  // Resolve document type
  const entry: DocumentSurfaceEntry | undefined = useMemo(() => {
    if (!registry) return undefined;
    return registry.resolve(filePath, documentType);
  }, [registry, filePath, documentType]);

  const contribution: DocumentContributionDef | undefined = entry?.contribution;
  const availableModes = contribution?.availableModes ?? (["code"] as SurfaceMode[]);
  const defaultMode = contribution?.defaultMode ?? "code";

  // Mode state
  const [internalMode, setInternalMode] = useState<SurfaceMode>(forcedMode ?? defaultMode);
  const mode = forcedMode ?? internalMode;

  // Collect all inline tokens (contribution + extra)
  const allTokens = useMemo((): InlineTokenDef[] => {
    return [...(contribution?.inlineTokens ?? []), ...extraTokens];
  }, [contribution?.inlineTokens, extraTokens]);

  // Build CodeMirror extensions for code/preview modes
  const cmExtensions = useMemo((): Extension[] => {
    const exts: Extension[] = [];

    // YAML language support for yaml contributions
    if (contribution?.mimeType === "application/x-yaml") {
      exts.push(yamlLanguageSupport);
    }

    // Inline token marks (code mode)
    if (mode === "code" && allTokens.length > 0) {
      exts.push(...createTokenMarkExtension(allTokens));
      exts.push(inlineTokenTheme);
    }

    // Live preview mode: markdown preview + token widget replacements
    if (mode === "preview") {
      if (contribution?.mimeType === "text/markdown") {
        exts.push(...markdownLivePreview());
      }
      if (allTokens.length > 0) {
        exts.push(...createTokenPreviewExtension(allTokens));
        exts.push(inlineTokenTheme);
      }
    }

    // Spell checking (code/preview modes — linter-based)
    if (spellChecker && (mode === "code" || mode === "preview")) {
      exts.push(spellCheckExtension({ checker: spellChecker }));
    }

    // Consumer-provided extras
    exts.push(...extraExtensions);
    return exts;
  }, [contribution, mode, allTokens, extraExtensions, spellChecker]);

  // Stable mode change handler
  const handleModeChange = useCallback((m: SurfaceMode) => {
    setInternalMode(m);
  }, []);

  // ── Render ──────────────────────────────────────────────────────────────────

  const containerStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    height: "100%",
    width: "100%",
  };

  return (
    <div className={className ?? ""} style={containerStyle}>
      {!hideToolbar && (
        <SurfaceToolbar
          mode={mode}
          availableModes={availableModes}
          displayName={contribution?.displayName ?? "Text"}
          onModeChange={handleModeChange}
        />
      )}

      <div style={{ flex: 1, minHeight: 0 }}>
        {/* Full custom renderer — override for this document type */}
        {CustomRenderer ? (
          <CustomRenderer
            value={value}
            onChange={onChange}
            mode={mode}
            filePath={filePath}
            readOnly={readOnly}
            className="h-full"
          />
        ) : modeRenderers?.[mode] ? (
          /* Per-mode custom renderer */
          React.createElement(modeRenderers[mode] as unknown as React.ComponentType<Record<string, unknown>>, {
            value,
            onChange,
            mode,
            filePath,
            readOnly,
            className: "h-full",
          })
        ) : (
          <>
            {/* SVG preview — render as data URI to sandbox content */}
            {mode === "preview" && contribution?.mimeType === "image/svg+xml" && (
              <SvgPreview value={value} />
            )}

            {/* Markdown preview — simple HTML rendering */}
            {mode === "preview" && contribution?.mimeType === "text/markdown" && (
              <MarkdownPreview value={value} />
            )}

            {/* Code editor — CodeMirror via Loro CRDT */}
            {(mode === "code" || (mode === "preview" && contribution?.mimeType !== "text/markdown" && contribution?.mimeType !== "image/svg+xml")) && (
              <CodeSurface
                doc={doc}
                text={text}
                extensions={cmExtensions}
                readOnly={readOnly}
              />
            )}

            {/* Spreadsheet surface — CSV/TSV grid editor */}
            {mode === "spreadsheet" && (
              <CsvSurface value={value} onChange={onChange} readOnly={readOnly} />
            )}

            {/* Form surface — schema-driven field editor */}
            {mode === "form" && (
              <FormSurface
                value={value}
                onChange={onChange}
                schema={formSchema}
                filePath={filePath}
                readOnly={readOnly}
              />
            )}

            {/* Report/layout mode — grouped/summarised read-only view */}
            {mode === "report" && (
              <ReportSurface value={value} filePath={filePath} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
