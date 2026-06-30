//! Workspace-wide name index + per-document state.
//!
//! Tracks every open document and the cross-file index the LSP request
//! handlers query (spec §17): characters, traits, beats, anchors,
//! todos. Reparses on every edit and emits diagnostics.
//!
//! Mirrors the Rust `loom_lsp::Workspace`, with two TypeScript-side
//! shifts: documents are keyed by their raw URI string (no `url::Url`
//! normalization), and dialogue bodies are the unified `BodyItem[]`
//! list (the TS parser dropped the separate `DialogueLine` carrier), so
//! `indexBody` recurses into a dialogue's body like any other block.

import type {
  Beat,
  BodyItem,
  Declaration,
  LoomFile,
} from "../parser/ast.ts";
import { type Diagnostic as ParserDiagnostic, parse } from "../parser/index.ts";
import type {
  CompletionItem,
  DocumentSymbolResponse,
  GotoDefinitionResponse,
  Hover,
  Location,
  Position,
  PublishDiagnosticsParams,
  Range,
} from "./types.ts";
import { stripPrefix, trimEndMatches, trimStartMatches } from "../parser/rust.ts";
import { findTokenSpans, lines, nameRangeInText, spanToRange, toLspDiagnostic } from "./util.ts";
import { completionAt } from "./completion.ts";
import { hoverAt } from "./hover.ts";
import { definitionAt } from "./definition.ts";
import { documentSymbols } from "./symbols.ts";
import { referencesAt } from "./references.ts";

/** A single open document plus its latest parse. */
export interface OpenDoc {
  text: string;
  file: LoomFile;
  diagnostics: ParserDiagnostic[];
}

/** Cross-file index entry: one occurrence of a name. */
export interface Occurrence {
  uri: string;
  range: Range;
}

/** A captured `todo` fence body. */
export interface Todo {
  uri: string;
  range: Range;
  text: string;
}

/** Per-character snapshot we expose to hover. */
export interface CharacterInfo {
  uri: string;
  range: Range;
  /** Declared `is X, Y` mixins. */
  mixins: string[];
  /** Raw indented body lines, one per source line. */
  body: string[];
}

/** Per-beat snapshot used by hover + definition. */
export interface BeatInfo {
  uri: string;
  range: Range;
  nameRange: Range;
  cast: string | null;
  setting: string | null;
}

/** In-memory project index. */
export class Workspace {
  docs = new Map<string, OpenDoc>();
  characters = new Map<string, CharacterInfo>();
  traits = new Map<string, Occurrence>();
  beats = new Map<string, BeatInfo[]>();
  anchors = new Map<string, Occurrence[]>();
  todos: Todo[] = [];

  /** Insert or replace a document, reparse, rebuild the index. */
  open(uri: string, text: string): void {
    this.update(uri, text);
  }

  /** Replace a document's text and reparse. */
  update(uri: string, text: string): void {
    const [file, diagnostics] = parse(text);
    this.docs.set(uri, { text, file, diagnostics });
    this.rebuildIndex();
  }

  /** Drop a document and rebuild. */
  close(uri: string): void {
    this.docs.delete(uri);
    this.rebuildIndex();
  }

  /**
   * Latest diagnostics for a single document, packaged for
   * `textDocument/publishDiagnostics`. Returns `null` if the document
   * is unknown.
   */
  diagnosticsFor(uri: string): PublishDiagnosticsParams | null {
    const doc = this.docs.get(uri);
    if (!doc) return null;
    return { uri, diagnostics: doc.diagnostics.map(toLspDiagnostic) };
  }

  private rebuildIndex(): void {
    this.characters.clear();
    this.traits.clear();
    this.beats.clear();
    this.anchors.clear();
    this.todos = [];
    for (const [uri, doc] of this.docs) {
      this.indexFile(uri, doc.file, doc.text);
    }
  }

  private indexFile(uri: string, file: LoomFile, text: string): void {
    for (const item of file.items) {
      switch (item.kind) {
        case "declaration":
          this.indexDeclaration(uri, item.value);
          break;
        case "beat":
          this.indexBeat(uri, item.value, text);
          break;
        case "letBinding":
          break;
      }
    }
  }

  private indexBeat(uri: string, beat: Beat, text: string): void {
    const range = spanToRange(beat.span);
    const nameRange = nameRangeInText(text, beat.span, beat.name);
    const info: BeatInfo = {
      uri,
      range,
      nameRange,
      cast: beat.contract.get("cast")?.value ?? null,
      setting: beat.contract.get("setting")?.value ?? null,
    };
    const entries = this.beats.get(beat.name);
    if (entries) entries.push(info);
    else this.beats.set(beat.name, [info]);
    for (const body of beat.body) {
      this.indexBody(uri, body);
    }
  }

  private indexDeclaration(uri: string, decl: Declaration): void {
    const range = spanToRange(decl.span);
    if (decl.kind === "character" || decl.kind === "role") {
      this.characters.set(decl.name, {
        uri,
        range,
        mixins: [...decl.mixin],
        body: decl.body.map((l) => l.text),
      });
    } else if (decl.kind === "trait") {
      this.traits.set(decl.name, { uri, range });
    }
  }

  private indexBody(uri: string, body: BodyItem): void {
    switch (body.kind) {
      case "directive":
        this.maybeAnchor(uri, body.value.raw, spanToRange(body.value.span));
        break;
      case "directiveBlock":
        this.maybeAnchor(uri, body.value.directive.raw, spanToRange(body.value.directive.span));
        for (const child of body.value.body) this.indexBody(uri, child);
        break;
      case "conditional":
        for (const arm of body.value.arms) {
          for (const child of arm.body) this.indexBody(uri, child);
        }
        break;
      case "choice":
        for (const child of body.value.body) this.indexBody(uri, child);
        break;
      case "dialogue":
        // TS parser unified the dialogue body into `BodyItem[]` (no
        // separate `DialogueLine`), so anchors in dialogue directives —
        // and anything else — surface by recursing like any other block.
        for (const child of body.value.body) this.indexBody(uri, child);
        break;
      case "match":
        for (const arm of body.value.arms) {
          for (const child of arm.body) this.indexBody(uri, child);
        }
        break;
      case "eachVisit":
        for (const child of body.value.first) this.indexBody(uri, child);
        for (const child of body.value.then) this.indexBody(uri, child);
        for (const child of body.value.finally) this.indexBody(uri, child);
        break;
      case "afterMorph":
        for (const child of body.value.after) this.indexBody(uri, child);
        for (const child of body.value.otherwise) this.indexBody(uri, child);
        break;
      case "metadata": {
        // A metadata fence stores its *tail* (the text after the opening
        // ```), then any body lines — see the parser's `collectFence`.
        // A todo fence is therefore one whose tail word is `todo`. The
        // Rust LSP tested for a literal "```todo" prefix, which never
        // matched the backtick-free value the shared parser actually
        // produces (its `todos` index was dead); keying off the tail
        // populates it.
        const trimmed = body.value.value.replace(/^\s+/, "");
        const rest = stripPrefix(trimmed, "todo");
        if (rest !== null && (rest.length === 0 || " \t\r\n".includes(rest[0]!))) {
          const text = trimChars(rest, "\n\r ");
          this.todos.push({ uri, range: spanToRange(body.value.span), text });
        }
        break;
      }
      default:
        break;
    }
  }

  private maybeAnchor(uri: string, raw: string, range: Range): void {
    // `<anchor: name>` — store the name.
    const inner = trimEndMatches(trimStartMatches(raw, "<"), ">").trim();
    const args = stripPrefix(inner, "anchor:");
    if (args !== null) {
      const name = args.trim();
      if (name.length > 0) {
        const entry = this.anchors.get(name);
        if (entry) entry.push({ uri, range });
        else this.anchors.set(name, [{ uri, range }]);
      }
    }
  }

  // ---------- request entry points ----------

  completionAt(uri: string, pos: Position): CompletionItem[] {
    return completionAt(this, uri, pos);
  }

  hoverAt(uri: string, pos: Position): Hover | null {
    return hoverAt(this, uri, pos);
  }

  definitionAt(uri: string, pos: Position): GotoDefinitionResponse | null {
    return definitionAt(this, uri, pos);
  }

  documentSymbols(uri: string): DocumentSymbolResponse | null {
    return documentSymbols(this, uri);
  }

  referencesAt(uri: string, pos: Position): Location[] {
    return referencesAt(this, uri, pos);
  }

  /**
   * References to a *named* identifier without a position. Scans every
   * document for whole-word occurrences of `name`. Used by the editor's
   * References panel when a character / beat / world key is pinned in
   * the focus bus (no cursor — only the name).
   */
  referencesByName(name: string): Location[] {
    const out: Location[] = [];
    for (const [uri, doc] of this.docs) {
      lines(doc.text).forEach((lineText, lineNo) => {
        for (const [start, end] of findTokenSpans(lineText, name)) {
          out.push({
            uri,
            range: {
              start: { line: lineNo, character: start },
              end: { line: lineNo, character: end },
            },
          });
        }
      });
    }
    return out;
  }
}

/**
 * `s.trim_matches(|c| chars.contains(c))` — strip every leading and
 * trailing character that appears in the `chars` set.
 */
function trimChars(s: string, chars: string): string {
  let start = 0;
  let end = s.length;
  while (start < end && chars.includes(s[start]!)) start += 1;
  while (end > start && chars.includes(s[end - 1]!)) end -= 1;
  return s.slice(start, end);
}
