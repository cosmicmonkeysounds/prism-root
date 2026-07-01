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
import { type Diagnostic as ParserDiagnostic, parse, parseMixinRef } from "../parser/index.ts";
import { Bundle, type LoomFileEntry } from "../runtime/bundle.ts";
import { compileModel } from "../runtime/sim/model.ts";
import type {
  CompletionItem,
  Diagnostic as LspDiagnostic,
  DocumentSymbolResponse,
  GotoDefinitionResponse,
  Hover,
  Location,
  Position,
  PublishDiagnosticsParams,
  Range,
} from "./types.ts";
import { DiagnosticSeverity } from "./types.ts";
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
  /** The whole declaration span (used by hover). */
  range: Range;
  /** Tight range over just the declared name (used by go-to-definition). */
  nameRange: Range;
  /** Declared `is X, Y` mixins. */
  mixins: string[];
  /** Raw indented body lines, one per source line. */
  body: string[];
  /** Names of the class-owned `beat name(…)` blocks this character declares. */
  ownedBeats: string[];
}

/**
 * Per-trait index entry. Richer than a bare `Occurrence`: carries the
 * trait's parameter list (`TRAIT Scanner(beat)` → `["beat"]`) and the
 * names of the beats it ships (`beat greet(…)` blocks), so hover and
 * completion can surface a trait's shape without re-parsing.
 */
export interface TraitInfo {
  uri: string;
  range: Range;
  /** Tight range over just the trait name (used by go-to-definition). */
  nameRange: Range;
  params: string[];
  /** Shipped-beat names authored inside the trait body. */
  beats: string[];
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
  traits = new Map<string, TraitInfo>();
  beats = new Map<string, BeatInfo[]>();
  anchors = new Map<string, Occurrence[]>();
  todos: Todo[] = [];
  /** Cross-file project diagnostics keyed by owning document URI. */
  projectDiagnostics = new Map<string, LspDiagnostic[]>();

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

  /** Drop every document (project switch / close) and rebuild empty. */
  reset(): void {
    if (this.docs.size === 0) return;
    this.docs.clear();
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
    const parser = doc.diagnostics.map(toLspDiagnostic);
    const project = this.projectDiagnostics.get(uri) ?? [];
    return { uri, diagnostics: [...parser, ...project] };
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
    this.rebuildProjectDiagnostics();
  }

  /**
   * Batch replace many documents with a **single** index rebuild. Plain
   * `update` reparses + rebuilds the whole cross-file index (and recompiles
   * the throwaway diagnostics bundle) on every call, so pushing N files one
   * at a time is O(N²). `updateMany` parses each doc once, then rebuilds the
   * index a single time — used by the editor's whole-project indexer so
   * opening a project is one rebuild, not one-per-file.
   */
  updateMany(entries: Array<[string, string]>): void {
    if (entries.length === 0) return;
    for (const [uri, text] of entries) {
      const [file, diagnostics] = parse(text);
      this.docs.set(uri, { text, file, diagnostics });
    }
    this.rebuildIndex();
  }

  /**
   * Compile every open document into a throwaway [`Bundle`] and lift its
   * cross-file `projectDiagnostics` into per-URI LSP diagnostics. The
   * compile is wrapped in a try/catch so a malformed in-progress edit
   * never takes the index down — a compile failure just leaves the
   * project-diagnostic layer empty until the next keystroke fixes it.
   */
  private rebuildProjectDiagnostics(): void {
    this.projectDiagnostics.clear();
    if (this.docs.size === 0) return;
    const firstUri = this.docs.keys().next().value as string;
    const bundle = new Bundle();
    for (const [uri, doc] of this.docs) {
      const entry: LoomFileEntry = {
        path: uriToPath(uri),
        stem: stemOf(uri),
        qualifier: "",
        source: doc.text,
        file: doc.file,
        diagnostics: doc.diagnostics,
      };
      bundle.files.push(entry);
    }
    try {
      compileModel(bundle);
    } catch {
      // Defensive: a partial edit can trip a compile invariant. Never let
      // that break parser diagnostics / completion / hover.
      return;
    }
    for (const d of bundle.projectDiagnostics) {
      const [uri, diagnostic] = this.mapProjectDiagnostic(d, firstUri);
      const list = this.projectDiagnostics.get(uri);
      if (list) list.push(diagnostic);
      else this.projectDiagnostics.set(uri, [diagnostic]);
    }
  }

  /**
   * Map one cross-file [`ProjectDiagnostic`] to a `[uri, Diagnostic]`
   * pair. Character-owned diagnostics land on the declaring character's
   * document + range; project-wide ones (no character) attach to the
   * first document at the file head.
   */
  private mapProjectDiagnostic(
    d: Bundle["projectDiagnostics"][number],
    firstUri: string,
  ): [string, LspDiagnostic] {
    const head: Range = {
      start: { line: 0, character: 0 },
      end: { line: 0, character: 0 },
    };
    // Everything but the whole-project trio carries a `character`; resolve
    // it to the character's declaration site.
    const owned = "character" in d ? this.characters.get(d.character) : undefined;
    const uri = owned?.uri ?? firstUri;
    const range = owned?.range ?? head;
    let severity: DiagnosticSeverity = DiagnosticSeverity.Error;
    let message: string;
    switch (d.kind) {
      case "missingMainFile":
        message = "project has no `main.loom` entry file";
        break;
      case "entryBeatUnresolved":
        message = `entry beat \`${d.name}\` is not defined`;
        break;
      case "noEntryBeat":
        message = "project defines no entry beat";
        break;
      case "ambiguousSlot":
        message = `\`${d.character}\` inherits slot \`${d.prop}\` ambiguously from two traits`;
        break;
      case "requiredSlotUnfilled":
        message = `\`${d.character}\` leaves required slot \`${d.slot}\` unfilled`;
        break;
      case "requiredParamUnfilled":
        message = `trait \`${d.trait}\` requires an argument for \`${d.param}\``;
        break;
      case "unresolvedTraitArg":
        message = `\`${d.arg}\` (for \`${d.trait}.${d.param}\`) names no beat`;
        break;
      case "derivedBeatConflict":
        message = `two traits both ship a beat named \`${d.beat}\``;
        break;
      case "unfilledDerivedSlot":
        message = `derived beat \`${d.beat}\` has an unfilled slot \`${d.slot}\``;
        severity = DiagnosticSeverity.Warning;
        break;
    }
    return [uri, { range, severity, source: "loom", message }];
  }

  /**
   * The beat names an owner (CHARACTER / ROLE / TRAIT) makes reachable by
   * a qualified divert `-> owner.<beat>`: its own `beat` blocks plus the
   * beats shipped by every trait in its `is` chain (one level, via the
   * trait index). Used by completion inside `is Trait(…)` and after
   * `-> self.` / `-> Owner.`.
   */
  ownerBeatNames(name: string): string[] {
    const out = new Set<string>();
    const char = this.characters.get(name);
    if (char) {
      for (const b of char.ownedBeats) out.add(b);
      for (const mixin of char.mixins) {
        const trait = this.traits.get(parseMixinRef(mixin).name);
        if (trait) for (const b of trait.beats) out.add(b);
      }
    }
    const trait = this.traits.get(name);
    if (trait) for (const b of trait.beats) out.add(b);
    return [...out];
  }

  private indexFile(uri: string, file: LoomFile, text: string): void {
    for (const item of file.items) {
      switch (item.kind) {
        case "declaration":
          this.indexDeclaration(uri, item.value, text);
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

  private indexDeclaration(uri: string, decl: Declaration, text: string): void {
    const range = spanToRange(decl.span);
    const nameRange = nameRangeInText(text, decl.span, decl.name);
    if (decl.kind === "character" || decl.kind === "role") {
      this.characters.set(decl.name, {
        uri,
        range,
        nameRange,
        mixins: [...decl.mixin],
        body: decl.body.map((l) => l.text),
        ownedBeats: decl.character?.beats.map((b) => b.name) ?? [],
      });
    } else if (decl.kind === "trait") {
      this.traits.set(decl.name, {
        uri,
        range,
        nameRange,
        params: decl.character ? [...decl.character.params] : [],
        beats: decl.character?.beats.map((b) => b.name) ?? [],
      });
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

/** Strip a `scheme://` prefix off a document URI to get a path-ish string. */
function uriToPath(uri: string): string {
  const schemeEnd = uri.indexOf("://");
  return schemeEnd >= 0 ? uri.slice(schemeEnd + 3) : uri;
}

/** File stem for a URI — basename without the `.loom` suffix. */
function stemOf(uri: string): string {
  const path = uriToPath(uri);
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.loom$/, "");
}
