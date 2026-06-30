//! Document symbol tree: beats, characters, traits, stats profiles,
//! scenes, generators per file.

import type { DeclarationKind } from "../parser/ast.ts";
import { type DocumentSymbol, type DocumentSymbolResponse, type Range, SymbolKind } from "./types.ts";
import { spanToRange } from "./util.ts";
import type { Workspace } from "./workspace.ts";

const DECLARATION_SYMBOL_KIND: Record<DeclarationKind, SymbolKind> = {
  character: SymbolKind.Class,
  role: SymbolKind.Class,
  trait: SymbolKind.Interface,
  item: SymbolKind.Object,
  location: SymbolKind.Namespace,
  faction: SymbolKind.Package,
  stats: SymbolKind.Struct,
  tree: SymbolKind.Enum,
  generator: SymbolKind.Event,
  scene: SymbolKind.Method,
  cohort: SymbolKind.Array,
  person: SymbolKind.Object,
  roster: SymbolKind.Package,
};

export function documentSymbols(ws: Workspace, uri: string): DocumentSymbolResponse | null {
  const doc = ws.docs.get(uri);
  if (!doc) return null;
  const out: DocumentSymbol[] = [];
  for (const item of doc.file.items) {
    switch (item.kind) {
      case "beat": {
        const range = spanToRange(item.value.span);
        out.push(symbol(item.value.name, SymbolKind.Function, range));
        break;
      }
      case "declaration": {
        const range = spanToRange(item.value.span);
        out.push(symbol(item.value.name, DECLARATION_SYMBOL_KIND[item.value.kind], range));
        break;
      }
      case "letBinding": {
        const range = spanToRange(item.value.span);
        out.push(symbol(item.value.name, SymbolKind.Variable, range));
        break;
      }
    }
  }
  return out;
}

function symbol(name: string, kind: SymbolKind, range: Range): DocumentSymbol {
  return { name, kind, range, selectionRange: range };
}
