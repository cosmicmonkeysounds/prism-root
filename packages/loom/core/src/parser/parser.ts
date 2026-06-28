//! Stitch scanned lines into a `LoomFile` AST.

import type {
  AdvanceSignal,
  AfterMorph,
  Beat,
  BodyItem,
  Choice,
  Conditional,
  ConditionalArm,
  DialogueBlock,
  DialogueLine,
  Directive,
  DirectiveBlock,
  Divert,
  DivertTarget,
  EachVisit,
  Header,
  ImprovDirective,
  ImprovDuration,
  Item,
  LetBinding,
  LoomFile,
  MatchArm,
  MatchBlock,
  PropertyValue,
  QuorumOp,
} from "./ast.ts";
import { emptyDeclaration, declarationKindFromKeyword } from "./ast.ts";
import { lower } from "./decl-body.ts";
import { Code, errorDiagnostic, type Diagnostic } from "./diagnostics.ts";
import { scan, scannedLineSpan, type LineKind, type ScannedLine } from "./lexer.ts";
import { pos, span, ZERO_POSITION, type Position, type Span } from "./source.ts";
import { splitOnce, stripPrefix, trimStartMatches } from "./rust.ts";

/** Parse a single `.loom` source string into a `LoomFile` + diagnostics. */
export function parse(source: string): [LoomFile, Diagnostic[]] {
  const [lines, diagnostics] = scan(source);
  const parser = new Parser(lines, diagnostics);
  const file = parser.parseFile();
  return [file, diagnostics];
}

class Parser {
  lines: ScannedLine[];
  cursor: number;
  diagnostics: Diagnostic[];

  constructor(lines: ScannedLine[], diagnostics: Diagnostic[]) {
    this.lines = lines;
    this.cursor = 0;
    this.diagnostics = diagnostics;
  }

  parseFile(): LoomFile {
    const header = this.parseHeader();
    const items: Item[] = [];
    let line = this.peek();
    while (line !== undefined) {
      switch (line.kind.kind) {
        case "declarationOpener": {
          const decl = this.parseDeclaration();
          lower(decl, this.diagnostics);
          items.push({ kind: "declaration", value: decl });
          break;
        }
        case "letBinding":
          items.push({ kind: "letBinding", value: this.parseLetBinding() });
          break;
        case "knotMarker":
          items.push({ kind: "beat", value: this.parseBeat() });
          break;
        default:
          this.cursor += 1;
          break;
      }
      line = this.peek();
    }
    return { header, items };
  }

  parseHeader(): Header {
    const first = this.peek();
    const start = first !== undefined ? scannedLineSpan(first).start : ZERO_POSITION;
    let title: string | null = null;
    const properties = new Map<string, PropertyValue>();
    let end = start;

    let line = this.peek();
    while (line !== undefined) {
      const k = line.kind;
      if (k.kind === "heading") {
        if (title === null) title = k.title;
        end = scannedLineSpan(line).end;
        this.cursor += 1;
      } else if (k.kind === "property") {
        const sp = scannedLineSpan(line);
        properties.set(k.key, { value: k.value, span: sp });
        end = sp.end;
        this.cursor += 1;
      } else {
        break;
      }
      line = this.peek();
    }

    return { title, properties, span: span(start, end) };
  }

  parseDeclaration() {
    const opener = this.lines[this.cursor]!;
    const k = opener.kind;
    if (k.kind !== "declarationOpener") {
      throw new Error("parseDeclaration entered without an opener");
    }
    const kindWord = k.kindWord;
    const name = k.name;
    const mixin = k.mixin;
    const kind = declarationKindFromKeyword(kindWord)!;
    this.cursor += 1;

    const bodyIndentFloor = opener.indent + 1;
    const body = [];
    let end = scannedLineSpan(opener).end;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent < bodyIndentFloor) break;
      const sp = scannedLineSpan(line);
      body.push({ indent: line.indent, text: line.text, span: sp });
      end = sp.end;
      this.cursor += 1;
      line = this.peek();
    }

    return emptyDeclaration(kind, name, mixin, body, span(scannedLineSpan(opener).start, end));
  }

  parseLetBinding(): LetBinding {
    const line = this.lines[this.cursor]!;
    this.cursor += 1;
    const sp = scannedLineSpan(line);
    if (line.kind.kind !== "letBinding") throw new Error("unreachable");
    return { name: line.kind.name, expression: line.kind.expression, span: sp };
  }

  parseBeat(): Beat {
    const opener = this.lines[this.cursor]!;
    if (opener.kind.kind !== "knotMarker") throw new Error("unreachable");
    const name = opener.kind.name;
    const params = opener.kind.params;
    this.cursor += 1;

    const contract = this.parseContract(opener.indent);
    const bodyIndentFloor = opener.indent;
    const body: BodyItem[] = [];
    let end = scannedLineSpan(opener).end;
    let line = this.peek();
    while (line !== undefined) {
      const k = line.kind;
      if (k.kind === "knotMarker") break;
      if (k.kind === "declarationOpener" && line.indent <= bodyIndentFloor) break;
      const item = this.parseBodyItem(bodyIndentFloor);
      if (item !== null) {
        end = bodyItemEnd(item) ?? end;
        body.push(item);
      } else {
        this.cursor += 1;
      }
      line = this.peek();
    }

    return { name, params, contract, body, span: span(scannedLineSpan(opener).start, end) };
  }

  parseContract(openerIndent: number): Map<string, PropertyValue> {
    const out = new Map<string, PropertyValue>();
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent <= openerIndent) break;
      const k = line.kind;
      if (k.kind === "property") {
        out.set(k.key, { value: k.value, span: scannedLineSpan(line) });
        this.cursor += 1;
      } else {
        break;
      }
      line = this.peek();
    }
    return out;
  }

  parseBodyItem(beatIndent: number): BodyItem | null {
    const line = this.peek();
    if (line === undefined) return null;
    const k = line.kind;
    switch (k.kind) {
      case "sceneHeading": {
        this.cursor += 1;
        return { kind: "sceneHeading", value: { value: k.text, span: scannedLineSpan(line) } };
      }
      case "speaker":
        return { kind: "dialogue", value: this.parseDialogueBlock(k.text, line) };
      case "choice":
        return { kind: "choice", value: this.parseChoice(k.sticky, k.text, line, beatIndent) };
      case "divertLine": {
        this.cursor += 1;
        const divert = parseDivertText(k.text, scannedLineSpan(line));
        if (divert.kind === "to") {
          this.collectDivertSlots(line.indent, divert.slots);
        }
        return { kind: "divert", value: divert };
      }
      case "tunnelReturn": {
        this.cursor += 1;
        return { kind: "divert", value: { kind: "return", span: scannedLineSpan(line) } };
      }
      case "fence":
        return {
          kind: "metadata",
          value: this.collectFence(k.tail, k.inlineClose, line),
        };
      case "directive": {
        const raw = k.text;
        const trimmed = raw.replace(/^\s+/u, "");
        if (trimmed.startsWith("if:")) {
          return { kind: "conditional", value: this.parseConditional(line) };
        } else if (trimmed.startsWith("match:")) {
          return { kind: "match", value: this.parseMatch(line) };
        } else if (trimmed === "each visit" || trimmed.startsWith("each visit")) {
          return { kind: "eachVisit", value: this.parseEachVisit(line) };
        } else if (trimmed.startsWith("after:")) {
          return { kind: "afterMorph", value: this.parseAfterMorph(line) };
        } else if (trimmed.startsWith("let:")) {
          this.cursor += 1;
          const rest = trimStartMatches(trimmed, "let:").trim();
          const split = splitOnce(rest, "=");
          const [name, expression] =
            split !== null ? [split[0].trim(), split[1].trim()] : [rest, ""];
          return { kind: "inlineLet", value: { name, expression, span: scannedLineSpan(line) } };
        } else if (this.directiveHasBody(line)) {
          return { kind: "directiveBlock", value: this.parseDirectiveBlock(raw, line) };
        } else {
          this.cursor += 1;
          return { kind: "directive", value: { raw, span: scannedLineSpan(line) } };
        }
      }
      case "prose": {
        const [value, sp] = this.collectAction(line, k.text);
        return { kind: "action", value: { value, span: sp } };
      }
      case "parenthetical": {
        this.cursor += 1;
        return {
          kind: "action",
          value: { value: `(${k.text})`, span: scannedLineSpan(line) },
        };
      }
      case "property": {
        if (k.key === "slot" && k.value.length > 0) {
          this.cursor += 1;
          return {
            kind: "slotPlaceholder",
            value: { name: k.value, span: scannedLineSpan(line) },
          };
        }
        this.cursor += 1;
        return null;
      }
      case "letBinding":
      case "heading":
      case "declarationOpener":
      case "knotMarker":
        return null;
    }
  }

  parseDialogueBlock(speaker: string, opener: ScannedLine): DialogueBlock {
    this.cursor += 1;
    const bodyIndentFloor = opener.indent + 1;
    let parenthetical: string | null = null;
    let improv: ImprovDirective | null = null;
    const lines: DialogueLine[] = [];
    let end = scannedLineSpan(opener).end;

    let line = this.peek();
    while (line !== undefined) {
      if (line.indent < bodyIndentFloor) break;
      const k = line.kind;
      if (k.kind === "parenthetical") {
        const sp = scannedLineSpan(line);
        const trimmedOwned = k.text.replace(/^\s+/u, "");
        if (improv === null && trimmedOwned.startsWith("improv")) {
          improv = parseImprovParenthetical(trimmedOwned, sp, this.diagnostics);
        } else if (parenthetical === null && lines.length === 0) {
          parenthetical = k.text;
        } else {
          lines.push({ kind: "parenthetical", value: { value: k.text, span: sp } });
        }
        end = sp.end;
        this.cursor += 1;
      } else if (k.kind === "divertLine") {
        const sp = scannedLineSpan(line);
        lines.push({ kind: "divert", value: parseDivertText(k.text, sp) });
        end = sp.end;
        this.cursor += 1;
      } else if (k.kind === "tunnelReturn") {
        const sp = scannedLineSpan(line);
        lines.push({ kind: "divert", value: { kind: "return", span: sp } });
        end = sp.end;
        this.cursor += 1;
      } else if (k.kind === "prose") {
        const [value, sp] = this.collectAction(line, k.text);
        end = sp.end;
        lines.push({ kind: "text", value: { value, span: sp } });
      } else if (k.kind === "sceneHeading") {
        const sp = scannedLineSpan(line);
        lines.push({ kind: "text", value: { value: k.text, span: sp } });
        end = sp.end;
        this.cursor += 1;
      } else if (k.kind === "directive") {
        const sp = scannedLineSpan(line);
        lines.push({ kind: "directive", value: { raw: k.text, span: sp } });
        end = sp.end;
        this.cursor += 1;
      } else {
        break;
      }
      line = this.peek();
    }

    const speakers = speaker
      .split("|")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    return {
      speaker,
      speakers,
      parenthetical,
      improv,
      lines,
      span: span(scannedLineSpan(opener).start, end),
    };
  }

  parseChoice(
    sticky: boolean,
    rawText: string,
    opener: ScannedLine,
    _beatIndent: number,
  ): Choice {
    this.cursor += 1;
    const [text, suppressed] = splitChoiceSuppression(rawText);
    const bodyIndentFloor = opener.indent + 1;
    const body: BodyItem[] = [];
    let end = scannedLineSpan(opener).end;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent < bodyIndentFloor) break;
      const item = this.parseBodyItem(bodyIndentFloor);
      if (item !== null) {
        end = bodyItemEnd(item) ?? end;
        body.push(item);
      } else {
        this.cursor += 1;
      }
      line = this.peek();
    }
    return { sticky, text, suppressed, body, span: span(scannedLineSpan(opener).start, end) };
  }

  collectAction(opener: ScannedLine, firstText: string): [string, Span] {
    const start = scannedLineSpan(opener).start;
    let end = scannedLineSpan(opener).end;
    let text = firstText;
    let lastLineNo = opener.line;
    this.cursor += 1;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent !== opener.indent) break;
      if (line.line > lastLineNo + 1) break;
      if (line.kind.kind === "prose") {
        if (text.length > 0) text += " ";
        text += line.kind.text;
        end = scannedLineSpan(line).end;
        lastLineNo = line.line;
        this.cursor += 1;
      } else {
        break;
      }
      line = this.peek();
    }
    return [text, span(start, end)];
  }

  collectFence(tail: string, inlineClose: boolean, opener: ScannedLine) {
    this.cursor += 1;
    if (inlineClose) {
      return { value: tail, span: scannedLineSpan(opener) };
    }
    let buf = tail;
    let end = scannedLineSpan(opener).end;
    let line = this.peek();
    while (line !== undefined) {
      const k = line.kind;
      if (k.kind === "fence") {
        end = scannedLineSpan(line).end;
        this.cursor += 1;
        if (k.tail.length === 0 && !k.inlineClose) break;
      } else {
        if (buf.length > 0) buf += "\n";
        buf += line.text;
        end = scannedLineSpan(line).end;
        this.cursor += 1;
      }
      line = this.peek();
    }
    return { value: buf, span: span(scannedLineSpan(opener).start, end) };
  }

  peek(): ScannedLine | undefined {
    return this.lines[this.cursor];
  }

  peekAhead(offset: number): ScannedLine | undefined {
    return this.lines[this.cursor + offset];
  }

  directiveHasBody(opener: ScannedLine): boolean {
    const next = this.peekAhead(1);
    return next !== undefined ? next.indent > opener.indent : false;
  }

  parseConditional(opener: ScannedLine): Conditional {
    const openerIndent = opener.indent;
    const start = scannedLineSpan(opener).start;
    let end = scannedLineSpan(opener).end;
    const arms: ConditionalArm[] = [];
    let sawOpener = false;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent !== openerIndent) break;
      if (line.kind.kind !== "directive") break;
      const raw = line.kind.text;
      const trimmed = raw.replace(/^\s+/u, "");
      let cond: string | null;
      if (!sawOpener && trimmed.startsWith("if:")) {
        cond = trimStartMatches(trimmed, "if:").trim();
      } else if (sawOpener && trimmed.startsWith("else if:")) {
        cond = trimStartMatches(trimmed, "else if:").trim();
      } else if (sawOpener && trimmed.trim() === "else") {
        cond = null;
      } else {
        break;
      }
      sawOpener = true;
      this.cursor += 1;
      const bodyIndentFloor = openerIndent + 1;
      const body: BodyItem[] = [];
      let armEnd = scannedLineSpan(line).end;
      let child = this.peek();
      while (child !== undefined) {
        if (child.indent < bodyIndentFloor) break;
        const item = this.parseBodyItem(bodyIndentFloor);
        if (item !== null) {
          armEnd = bodyItemEnd(item) ?? armEnd;
          body.push(item);
        } else {
          this.cursor += 1;
        }
        child = this.peek();
      }
      end = armEnd;
      arms.push({ condition: cond, body, span: span(scannedLineSpan(line).start, armEnd) });
      line = this.peek();
    }
    return { arms, span: span(start, end) };
  }

  parseMatch(opener: ScannedLine): MatchBlock {
    const openerIndent = opener.indent;
    if (opener.kind.kind !== "directive") throw new Error("unreachable");
    const scrutinee = trimStartMatches(opener.kind.text.replace(/^\s+/u, ""), "match:").trim();
    this.cursor += 1;

    const peeked = this.peek();
    const armIndent =
      peeked !== undefined && peeked.indent > openerIndent ? peeked.indent : openerIndent + 1;
    const arms: MatchArm[] = [];
    let end = scannedLineSpan(opener).end;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent <= openerIndent) break;
      if (line.indent !== armIndent) {
        this.cursor += 1;
        line = this.peek();
        continue;
      }
      const k = line.kind;
      let pattern: string;
      if (k.kind === "prose" || k.kind === "sceneHeading") {
        pattern = k.text;
      } else if (k.kind === "speaker") {
        pattern = k.text;
      } else if (k.kind === "property") {
        pattern = k.value.length === 0 ? k.key : `${k.key}: ${k.value}`;
      } else {
        break;
      }
      const armStart = scannedLineSpan(line).start;
      this.cursor += 1;
      const body: BodyItem[] = [];
      let armEnd = scannedLineSpan(line).end;
      let child = this.peek();
      while (child !== undefined) {
        if (child.indent <= armIndent) break;
        const item = this.parseBodyItem(armIndent);
        if (item !== null) {
          armEnd = bodyItemEnd(item) ?? armEnd;
          body.push(item);
        } else {
          this.cursor += 1;
        }
        child = this.peek();
      }
      end = armEnd;
      arms.push({ pattern: pattern.trim(), body, span: span(armStart, armEnd) });
      line = this.peek();
    }
    return { scrutinee, arms, span: span(scannedLineSpan(opener).start, end) };
  }

  parseEachVisit(opener: ScannedLine): EachVisit {
    const openerIndent = opener.indent;
    this.cursor += 1;
    const peeked = this.peek();
    const armIndent =
      peeked !== undefined && peeked.indent > openerIndent ? peeked.indent : openerIndent + 1;
    const out: EachVisit = { first: [], then: [], finally: [], span: scannedLineSpan(opener) };
    let end = scannedLineSpan(opener).end;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent <= openerIndent) break;
      if (line.indent !== armIndent) {
        this.cursor += 1;
        line = this.peek();
        continue;
      }
      const k = line.kind;
      let label: string;
      if (k.kind === "prose" || k.kind === "sceneHeading") {
        label = k.text.trim();
      } else if (k.kind === "speaker") {
        label = k.text.trim();
      } else {
        break;
      }
      this.cursor += 1;
      const body: BodyItem[] = [];
      let armEnd = scannedLineSpan(line).end;
      let child = this.peek();
      while (child !== undefined) {
        if (child.indent <= armIndent) break;
        const item = this.parseBodyItem(armIndent);
        if (item !== null) {
          armEnd = bodyItemEnd(item) ?? armEnd;
          body.push(item);
        } else {
          this.cursor += 1;
        }
        child = this.peek();
      }
      end = armEnd;
      if (label === "first") out.first = body;
      else if (label === "then") out.then = body;
      else if (label === "finally") out.finally = body;
      line = this.peek();
    }
    out.span = span(scannedLineSpan(opener).start, end);
    return out;
  }

  parseAfterMorph(opener: ScannedLine): AfterMorph {
    const openerIndent = opener.indent;
    if (opener.kind.kind !== "directive") throw new Error("unreachable");
    const condition = trimStartMatches(opener.kind.text.replace(/^\s+/u, ""), "after:").trim();
    const start = scannedLineSpan(opener).start;
    this.cursor += 1;
    const bodyIndentFloor = openerIndent + 1;
    const afterBody: BodyItem[] = [];
    let end = scannedLineSpan(opener).end;
    let child = this.peek();
    while (child !== undefined) {
      if (child.indent < bodyIndentFloor) break;
      const item = this.parseBodyItem(bodyIndentFloor);
      if (item !== null) {
        end = bodyItemEnd(item) ?? end;
        afterBody.push(item);
      } else {
        this.cursor += 1;
      }
      child = this.peek();
    }
    const otherwiseBody: BodyItem[] = [];
    const line = this.peek();
    if (line !== undefined && line.indent === openerIndent) {
      const k = line.kind;
      if (k.kind === "directive" && k.text.trim() === "otherwise") {
        this.cursor += 1;
        let c = this.peek();
        while (c !== undefined) {
          if (c.indent < bodyIndentFloor) break;
          const item = this.parseBodyItem(bodyIndentFloor);
          if (item !== null) {
            end = bodyItemEnd(item) ?? end;
            otherwiseBody.push(item);
          } else {
            this.cursor += 1;
          }
          c = this.peek();
        }
      }
    }
    return { condition, after: afterBody, otherwise: otherwiseBody, span: span(start, end) };
  }

  collectDivertSlots(openerIndent: number, slots: Map<string, BodyItem[]>): void {
    const slotIndentFloor = openerIndent + 1;
    let line = this.peek();
    while (line !== undefined) {
      if (line.indent < slotIndentFloor) break;
      const k = line.kind;
      if (k.kind !== "property") break;
      const key = k.key;
      const value = k.value;
      if (value.length > 0) break;
      this.cursor += 1;
      const bodyIndentFloor = line.indent + 1;
      const body: BodyItem[] = [];
      let child = this.peek();
      while (child !== undefined) {
        if (child.indent < bodyIndentFloor) break;
        const item = this.parseBodyItem(bodyIndentFloor);
        if (item !== null) {
          body.push(item);
        } else {
          this.cursor += 1;
        }
        child = this.peek();
      }
      slots.set(key, body);
      line = this.peek();
    }
  }

  parseDirectiveBlock(raw: string, opener: ScannedLine): DirectiveBlock {
    this.cursor += 1;
    const bodyIndentFloor = opener.indent + 1;
    const body: BodyItem[] = [];
    let end = scannedLineSpan(opener).end;
    let child = this.peek();
    while (child !== undefined) {
      if (child.indent < bodyIndentFloor) break;
      const item = this.parseBodyItem(bodyIndentFloor);
      if (item !== null) {
        end = bodyItemEnd(item) ?? end;
        body.push(item);
      } else {
        this.cursor += 1;
      }
      child = this.peek();
    }
    return {
      directive: { raw, span: scannedLineSpan(opener) },
      body,
      span: span(scannedLineSpan(opener).start, end),
    };
  }
}

function bodyItemEnd(item: BodyItem): Position | null {
  switch (item.kind) {
    case "sceneHeading":
    case "action":
    case "metadata":
      return item.value.span.end;
    case "dialogue":
      return item.value.span.end;
    case "choice":
      return item.value.span.end;
    case "divert":
      return item.value.span.end;
    case "directive":
      return item.value.span.end;
    case "conditional":
      return item.value.span.end;
    case "match":
      return item.value.span.end;
    case "eachVisit":
      return item.value.span.end;
    case "afterMorph":
      return item.value.span.end;
    case "inlineLet":
      return item.value.span.end;
    case "directiveBlock":
      return item.value.span.end;
    case "slotPlaceholder":
      return item.value.span.end;
  }
}

function splitChoiceSuppression(raw: string): [string, string | null] {
  const open = raw.indexOf("[");
  if (open >= 0) {
    const closeRel = raw.slice(open + 1).indexOf("]");
    if (closeRel >= 0) {
      const close = open + 1 + closeRel;
      const visible = raw.slice(0, open) + raw.slice(close + 1);
      const suppressed = raw.slice(open + 1, close);
      return [visible.trim(), suppressed];
    }
  }
  return [raw.trim(), null];
}

function parseDivertText(text: string, sp: Span): Divert {
  text = text.trim();
  if (text === "END") {
    return { kind: "end", span: sp };
  }
  // Tunnel call form: `(name) ->` or `(name with k: v) ->`.
  {
    const rest = stripPrefix(text, "(");
    if (rest !== null) {
      const endParen = rest.indexOf(")");
      if (endParen >= 0) {
        const inner = rest.slice(0, endParen).trim();
        return { kind: "tunnel", target: parseDivertTarget(inner), span: sp };
      }
    }
  }
  // Strip a trailing `as <ident>` before splitting on ` with `.
  let scopeAs: string | null = null;
  {
    const idx = text.lastIndexOf(" as ");
    if (idx >= 0) {
      const head = text.slice(0, idx).replace(/\s+$/u, "");
      const tail = text.slice(idx + 4).trim();
      if (tail.length > 0 && /^[0-9A-Za-z_]+$/.test(tail)) {
        text = head;
        scopeAs = tail;
      }
    }
  }
  let head: string;
  let paramsText: string;
  {
    const idx = text.indexOf(" with ");
    if (idx >= 0) {
      head = text.slice(0, idx);
      paramsText = text.slice(idx + 6);
    } else {
      head = text;
      paramsText = "";
    }
  }
  const target = parseDivertTarget(head.trim());
  const params = parseDivertParams(paramsText);
  return { kind: "to", target, params, slots: new Map(), scopeAs, span: sp };
}

function parseDivertTarget(text: string): DivertTarget {
  let filePart: string;
  let knot: string | null;
  const hash = text.indexOf("#");
  if (hash >= 0) {
    filePart = text.slice(0, hash);
    knot = text.slice(hash + 1).trim();
  } else {
    filePart = text;
    knot = null;
  }
  const slash = filePart.lastIndexOf("/");
  if (slash >= 0) {
    return {
      qualifier: filePart.slice(0, slash),
      name: filePart.slice(slash + 1),
      knot,
    };
  }
  return { qualifier: null, name: filePart, knot };
}

function parseDivertParams(text: string): Map<string, string> {
  const out = new Map<string, string>();
  if (text.trim().length === 0) return out;
  for (const chunk of text.split(",")) {
    const trimmed = chunk.trim();
    const kv = splitOnce(trimmed, ":");
    if (kv !== null) {
      out.set(kv[0].trim(), kv[1].trim());
    }
  }
  return out;
}

function parseImprovParenthetical(
  text: string,
  sp: Span,
  diagnostics: Diagnostic[],
): ImprovDirective {
  const rest = (stripPrefix(text.replace(/^\s+/u, ""), "improv") ?? text).trim();
  let duration: ImprovDuration | null = null;
  let quorum: QuorumOp = { kind: "any" };
  let advanceOn: AdvanceSignal[] = [];

  for (let chunk of splitImprovTopLevel(rest)) {
    chunk = chunk.trim();
    if (chunk.length === 0) continue;
    const durRest = stripPrefix(chunk, "duration:");
    const advRest = stripPrefix(chunk, "advance on:");
    if (durRest !== null) {
      duration = parseImprovDuration(durRest.trim());
    } else if (advRest !== null) {
      const [q, signals] = parseAdvanceOn(advRest.trim(), sp, diagnostics);
      quorum = q;
      advanceOn = signals;
    }
  }
  if (duration === null) {
    diagnostics.push(
      errorDiagnostic(
        Code.L1140ImprovMissingDuration,
        sp,
        "`(improv …)` is missing a `duration:` field",
      ),
    );
  }
  return { duration, quorum, advanceOn, span: sp };
}

/** Comma-split respecting bracket depth so `any [a, b, c]` stays intact. */
function splitImprovTopLevel(text: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let last = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === "(" || ch === "[" || ch === "{") depth += 1;
    else if (ch === ")" || ch === "]" || ch === "}") depth -= 1;
    else if (ch === "," && depth === 0) {
      out.push(text.slice(last, i));
      last = i + 1;
    }
  }
  out.push(text.slice(last));
  return out;
}

function parseImprovDuration(raw: string): ImprovDuration | null {
  raw = raw.trim();
  if (raw.length === 0) return null;
  let split: number | null = null;
  for (let i = 0; i < raw.length; i++) {
    if (/[A-Za-z]/.test(raw[i]!)) {
      split = i;
      break;
    }
  }
  let numPart: string;
  let unitPart: string;
  if (split !== null) {
    numPart = raw.slice(0, split);
    unitPart = raw.slice(split).trim();
  } else {
    numPart = raw;
    unitPart = "s";
  }
  const value = Number(numPart.trim());
  if (numPart.trim().length === 0 || Number.isNaN(value)) return null;
  const unit =
    unitPart === "ms" ? "ms" : unitPart === "m" || unitPart === "min" ? "minutes" : "seconds";
  return { value, unit };
}

function parseAdvanceOn(
  raw: string,
  sp: Span,
  diagnostics: Diagnostic[],
): [QuorumOp, AdvanceSignal[]] {
  raw = raw.trim();
  let quorum: QuorumOp;
  let listText: string;
  const allRest = stripPrefix(raw, "all");
  const anyRest = stripPrefix(raw, "any");
  const quorumRest = stripPrefix(raw, "quorum");
  if (allRest !== null) {
    quorum = { kind: "all" };
    listText = allRest.replace(/^\s+/u, "");
  } else if (anyRest !== null) {
    quorum = { kind: "any" };
    listText = anyRest.replace(/^\s+/u, "");
  } else if (quorumRest !== null) {
    const rest = quorumRest.replace(/^\s+/u, "");
    let n: number;
    let after: string;
    const paren = stripPrefix(rest, "(");
    if (paren !== null) {
      const endIdx = paren.indexOf(")");
      const end = endIdx >= 0 ? endIdx : paren.length;
      const parsed = Number.parseInt(paren.slice(0, end).trim(), 10);
      n = Number.isNaN(parsed) ? 0 : parsed;
      const afterIdx = Math.min(end + 1, paren.length);
      after = paren.slice(afterIdx);
    } else {
      n = 0;
      after = rest;
    }
    quorum = { kind: "n", value: n };
    listText = after.replace(/^\s+/u, "");
  } else {
    quorum = { kind: "any" };
    listText = raw;
  }
  let inside: string;
  {
    const trimmed = listText.trim();
    const open = stripPrefix(trimmed, "[");
    if (open !== null) {
      const close = rsplitOnceLocal(open, "]");
      inside = close !== null ? close[0] : listText;
    } else {
      inside = listText;
    }
  }
  const signals: AdvanceSignal[] = [];
  for (let chunk of splitImprovTopLevel(inside)) {
    chunk = chunk.trim();
    if (chunk.length === 0) continue;
    const sig = parseAdvanceSignal(chunk);
    if (sig !== null) {
      signals.push(sig);
    } else {
      diagnostics.push(
        errorDiagnostic(Code.L1141ImprovBadSignal, sp, `unknown advance signal \`${chunk}\``),
      );
    }
  }
  return [quorum, signals];
}

function rsplitOnceLocal(s: string, sep: string): [string, string] | null {
  const idx = s.lastIndexOf(sep);
  if (idx < 0) return null;
  return [s.slice(0, idx), s.slice(idx + sep.length)];
}

function parseAdvanceSignal(text: string): AdvanceSignal | null {
  text = text.trim();
  if (text === "pedal") {
    return { kind: "pedal" };
  }
  {
    const rest = stripPrefix(text, "speech");
    if (rest !== null) {
      const a = stripPrefix(rest.trim(), "(");
      if (a === null) return null;
      const inner = a.endsWith(")") ? a.slice(0, a.length - 1) : null;
      if (inner === null) return null;
      return { kind: "speech", anchor: inner.trim() };
    }
  }
  {
    const rest = stripPrefix(text, "gesture");
    if (rest !== null) {
      const a = stripPrefix(rest.trim(), "(");
      if (a === null) return null;
      const inner = a.endsWith(")") ? a.slice(0, a.length - 1) : null;
      if (inner === null) return null;
      return { kind: "gesture", name: inner.trim() };
    }
  }
  return null;
}

// Re-export so the editor / runtime can build positions if needed.
export { pos, span };
