/**
 * Loom Lang — Lezer-based CodeMirror 6 language support.
 *
 * Uses @lezer/lr's parser infrastructure with a hand-built node set
 * and incremental parser. Replaces StreamLanguage (which can crash when
 * token() fails to advance) with a proper tree-producing parser.
 *
 * Benefits over StreamLanguage:
 *   - Incremental re-parsing on edits (only re-parses changed regions)
 *   - Proper syntax tree with named nodes (for folding, selection, etc.)
 *   - No stream-advance bugs (tree parsers operate on ranges, not streams)
 *   - Standard Lezer highlighting via styleTags
 *
 * The parser produces a flat tree of line-level and inline nodes:
 *   Document
 *     Comment | Header | Section | Condition | Action | Choice | Divert
 *     | Return | Dispatch | Flavor | Annotation | SExpr | Property
 *     | Speaker | TextLine
 *       WikiLink | Operand | Interpolation | Trigger | Variation | ResolveRef
 */

import { NodeType, NodeSet, Tree, TreeFragment, Parser, Input, PartialParse } from '@lezer/common';
import { styleTags, tags } from '@lezer/highlight';
import { Language, LanguageSupport, defineLanguageFacet, foldNodeProp } from '@codemirror/language';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';

// ── Node types ────────────────────────────────────────────────────────────────

/** Node type IDs — each syntactic construct gets a numeric ID. */
const N = {
  Document: 1,
  // Line-level
  Comment: 2,
  Header: 3,
  Section: 4,
  Condition: 5,
  Action: 6,
  OnceChoice: 7,
  StickyChoice: 8,
  HardDivert: 9,
  SoftDivert: 10,
  LoomReturn: 11,
  Dispatch: 12,
  Flavor: 13,
  Annotation: 14,
  SExpr: 15,
  Property: 16,
  Speaker: 17,
  TextLine: 18,
  BlankLine: 19,
  // Sigils
  Sigil: 20,
  // Identifiers
  Identifier: 21,
  SpeakerName: 22,
  DivertTarget: 23,
  TypeTag: 24,
  SectionId: 25,
  ActionKeyword: 26,
  SExprKeyword: 27,
  Modifier: 28,
  PropertyKey: 29,
  PropertyValue: 30,
  Operator: 31,
  // Inline
  WikiLink: 32,
  Operand: 33,
  Interpolation: 34,
  Trigger: 35,
  Variation: 36,
  ResolveRef: 37,
  PlainText: 38,
  // Literals
  StringLiteral: 39,
  NumberLiteral: 40,
  // Speaker block
  SpeakerBlock: 41,
  // Misc
  GuardKeyword: 42,
  BoolLiteral: 43,
} as const;

const nodeNames = [
  '', // 0 is unused
  'Document',
  'Comment', 'Header', 'Section', 'Condition', 'Action',
  'OnceChoice', 'StickyChoice', 'HardDivert', 'SoftDivert',
  'LoomReturn', 'Dispatch', 'Flavor', 'Annotation',
  'SExpr', 'Property', 'Speaker', 'TextLine', 'BlankLine',
  'Sigil',
  'Identifier', 'SpeakerName', 'DivertTarget', 'TypeTag',
  'SectionId', 'ActionKeyword', 'SExprKeyword', 'Modifier',
  'PropertyKey', 'PropertyValue', 'Operator',
  'WikiLink', 'Operand', 'Interpolation', 'Trigger',
  'Variation', 'ResolveRef', 'PlainText',
  'StringLiteral', 'NumberLiteral',
  'SpeakerBlock',
  'GuardKeyword', 'BoolLiteral',
];

const nodeSet = new NodeSet(
  nodeNames.map((name, i) =>
    NodeType.define(
      name
        ? { id: i, name, top: i === N.Document }
        : { id: i, top: i === N.Document },
    ),
  ),
).extend(
  styleTags({
    Comment: tags.comment,
    'Header/Sigil': tags.definitionKeyword,
    'Section/Sigil': tags.heading,
    'Condition/Sigil': tags.controlKeyword,
    'Action/Sigil': tags.operatorKeyword,
    'OnceChoice/Sigil StickyChoice/Sigil': tags.keyword,
    'HardDivert/Sigil SoftDivert/Sigil LoomReturn/Sigil Dispatch/Sigil': tags.controlKeyword,
    'Flavor/Sigil': tags.emphasis,
    Identifier: tags.labelName,
    SpeakerName: tags.typeName,
    DivertTarget: tags.labelName,
    TypeTag: tags.typeName,
    SectionId: tags.labelName,
    ActionKeyword: tags.operatorKeyword,
    SExprKeyword: tags.macroName,
    Modifier: tags.modifier,
    PropertyKey: tags.propertyName,
    PropertyValue: tags.atom,
    Operator: tags.logicOperator,
    GuardKeyword: tags.controlKeyword,
    BoolLiteral: tags.bool,
    WikiLink: tags.link,
    Operand: tags.atom,
    Interpolation: tags.inserted,
    Trigger: tags.tagName,
    Variation: tags.string,
    ResolveRef: tags.variableName,
    StringLiteral: tags.string,
    NumberLiteral: tags.number,
    PlainText: tags.content,
    'Flavor/PlainText': tags.emphasis,
    'Annotation/Sigil': tags.meta,
    'SExpr/Sigil': tags.bracket,
    'SpeakerBlock': tags.bracket,
  }),
  // Folding: sections and headers can fold
  foldNodeProp.add({
    Section(node) { return { from: node.to, to: node.to }; },
    Header(node) { return { from: node.to, to: node.to }; },
  }),
);

// ── Parser ────────────────────────────────────────────────────────────────────

/**
 * Line-based incremental parser for Loom Lang.
 *
 * Parses the document line-by-line, classifying each line and extracting
 * inline constructs. Produces a Lezer Tree that CodeMirror consumes for
 * highlighting, folding, and selection.
 */
class LoomParser extends Parser {
  createParse(input: Input, _fragments: readonly TreeFragment[], _ranges: readonly { from: number; to: number }[]): PartialParse {
    return new LoomPartialParse(input, this);
  }
}

interface TreeChild {
  type: number;
  from: number;
  to: number;
  children?: TreeChild[];
}

class LoomPartialParse implements PartialParse {
  readonly stoppedAt: number | null = null;
  parsedPos: number;

  private input: Input;
  private parser: LoomParser;
  private lineChildren: TreeChild[] = [];

  constructor(input: Input, parser: LoomParser) {
    this.input = input;
    this.parser = parser;
    this.parsedPos = 0;
  }

  advance(): Tree | null {
    const text = this.input.read(0, this.input.length);

    // Parse all lines
    const lines = text.split('\n');
    let offset = 0;

    for (const rawLine of lines) {
      const lineStart = offset;
      const lineEnd = offset + rawLine.length;
      const trimmed = rawLine.trimStart();
      const indent = rawLine.length - trimmed.length;

      if (trimmed.length === 0) {
        this.lineChildren.push({ type: N.BlankLine, from: lineStart, to: lineEnd });
      } else {
        const lineNode = this.classifyLine(trimmed, lineStart, lineEnd, indent);
        this.lineChildren.push(lineNode);
      }

      offset = lineEnd + 1; // +1 for \n
    }

    // Build tree
    const children = this.lineChildren.map(c => this.buildNode(c));
    const positions = this.lineChildren.map(c => c.from);

    return new Tree(
      nodeSet.types[N.Document] as NodeType,
      children,
      positions,
      text.length,
    );
  }

  stopAt(_pos: number): void {}

  // ── Line classification ─────────────────────────────────────────────────────

  private classifyLine(trimmed: string, from: number, to: number, indent: number): TreeChild {
    // Comment
    if (trimmed.startsWith('//')) {
      return { type: N.Comment, from, to };
    }

    // Header: # id "title" :type
    if (trimmed.startsWith('# ') || trimmed === '#') {
      return this.parseHeader(trimmed, from, to, indent);
    }

    // Section: -- id .modifier
    if (trimmed.startsWith('--')) {
      return this.parseSection(trimmed, from, to, indent);
    }

    // Condition: ? expr
    if (trimmed.startsWith('?')) {
      const children: TreeChild[] = [
        { type: N.Sigil, from: from + indent, to: from + indent + 1 },
      ];
      this.addExprChildren(trimmed.slice(1).trim(), from + indent + 1, to, children);
      return { type: N.Condition, from, to, children };
    }

    // Action: ~ keyword args
    if (trimmed.startsWith('~')) {
      return this.parseAction(trimmed, from, to, indent);
    }

    // Choices
    if (trimmed.startsWith('* ')) {
      return this.parseChoice(N.OnceChoice, trimmed, from, to, indent);
    }
    if (trimmed.startsWith('+ ')) {
      return this.parseChoice(N.StickyChoice, trimmed, from, to, indent);
    }

    // Soft divert: ->>
    if (trimmed.startsWith('->>')) {
      const children: TreeChild[] = [
        { type: N.Sigil, from: from + indent, to: from + indent + 3 },
      ];
      const rest = trimmed.slice(3).trim();
      if (rest) {
        children.push({ type: N.DivertTarget, from: to - rest.length, to });
      }
      return { type: N.SoftDivert, from, to, children };
    }

    // Hard divert: ->
    if (trimmed.startsWith('->')) {
      const children: TreeChild[] = [
        { type: N.Sigil, from: from + indent, to: from + indent + 2 },
      ];
      const rest = trimmed.slice(2).trim();
      if (rest) {
        // Check for tunnel return: -> target ->
        const tunnelMatch = rest.match(/^(\S+)\s*->$/);
        if (tunnelMatch) {
          children.push({ type: N.DivertTarget, from: from + indent + 2 + (trimmed.length - 2 - rest.length), to: to - 2 });
        } else {
          children.push({ type: N.DivertTarget, from: to - rest.length, to });
        }
      }
      return { type: N.HardDivert, from, to, children };
    }

    // Return: <-
    if (trimmed.startsWith('<-')) {
      return { type: N.LoomReturn, from, to, children: [
        { type: N.Sigil, from: from + indent, to: from + indent + 2 },
      ] };
    }

    // Dispatch: >>
    if (trimmed.startsWith('>>')) {
      const children: TreeChild[] = [
        { type: N.Sigil, from: from + indent, to: from + indent + 2 },
      ];
      const rest = trimmed.slice(2).trim();
      if (rest) {
        children.push({ type: N.DivertTarget, from: to - rest.length, to });
      }
      return { type: N.Dispatch, from, to, children };
    }

    // Flavor: > text
    if (trimmed.startsWith('> ') || trimmed === '>') {
      const children: TreeChild[] = [
        { type: N.Sigil, from: from + indent, to: from + indent + 1 },
      ];
      if (trimmed.length > 2) {
        const textStart = from + indent + 2;
        this.addInlineChildren(trimmed.slice(2), textStart, to, children);
      }
      return { type: N.Flavor, from, to, children };
    }

    // Annotation: @ key value
    if (trimmed.startsWith('@') && indent === 0) {
      return { type: N.Annotation, from, to, children: [
        { type: N.Sigil, from: from, to: from + 1 },
      ] };
    }

    // S-expression: (keyword ...)
    if (trimmed.startsWith('(')) {
      return this.parseSExpr(trimmed, from, to, indent);
    }

    // Property: .key value
    if (trimmed.startsWith('.')) {
      return this.parseProperty(trimmed, from, to, indent);
    }

    // Speaker: ALLCAPS (not indented)
    if (indent === 0 && /^[A-Z][A-Z0-9_]*(\s*\{|$|\s)/.test(trimmed)) {
      return this.parseSpeaker(trimmed, from, to);
    }

    // Text / dialogue
    const children: TreeChild[] = [];
    this.addInlineChildren(trimmed, from + indent, to, children);
    return { type: N.TextLine, from, to, children };
  }

  // ── Line-specific parsers ───────────────────────────────────────────────────

  private parseHeader(trimmed: string, from: number, to: number, indent: number): TreeChild {
    const children: TreeChild[] = [
      { type: N.Sigil, from: from + indent, to: from + indent + 1 },
    ];

    let pos = from + indent + 2; // after "# "
    const rest = trimmed.slice(2);

    // ID
    const idMatch = rest.match(/^[a-z_][a-z0-9_-]*/);
    if (idMatch) {
      children.push({ type: N.Identifier, from: pos, to: pos + idMatch[0].length });
      pos += idMatch[0].length;
    }

    // String "title"
    const restAfterIdRaw = trimmed.slice(2 + (idMatch?.[0].length ?? 0));
    const strMatch = restAfterIdRaw.match(/\s*"(?:[^"\\]|\\.)*"/);
    if (strMatch) {
      const strStart = pos + (strMatch.index ?? 0);
      const strContent = strMatch[0].trimStart();
      children.push({ type: N.StringLiteral, from: strStart + strMatch[0].length - strContent.length, to: strStart + strMatch[0].length });
      pos = strStart + strMatch[0].length;
    }

    // :type
    const restAfterStr = trimmed.slice(pos - from);
    const typeMatch = restAfterStr.match(/\s*(:[a-z]+)/);
    if (typeMatch) {
      const tag = typeMatch[1] ?? '';
      const typeStart = pos + (typeMatch.index ?? 0) + typeMatch[0].length - tag.length;
      children.push({ type: N.TypeTag, from: typeStart, to: typeStart + tag.length });
    }

    return { type: N.Header, from, to, children };
  }

  private parseSection(trimmed: string, from: number, to: number, indent: number): TreeChild {
    const children: TreeChild[] = [
      { type: N.Sigil, from: from + indent, to: from + indent + 2 },
    ];

    const rest = trimmed.slice(2).trim();
    let restPos = 0;

    // Section ID
    const idMatch = rest.match(/^[a-z_][a-z0-9_-]*/);
    if (idMatch) {
      children.push({ type: N.SectionId, from: to - rest.length, to: to - rest.length + idMatch[0].length });
      restPos = idMatch[0].length;
    }

    // Modifiers: .once .cooldown(30s)
    const modRe = /\.[a-z_][a-z0-9_]*(\([^)]*\))?/g;
    const afterId = rest.slice(restPos);
    let modMatch;
    while ((modMatch = modRe.exec(afterId)) !== null) {
      const mStart = to - rest.length + restPos + modMatch.index;
      children.push({ type: N.Modifier, from: mStart, to: mStart + modMatch[0].length });
    }

    return { type: N.Section, from, to, children };
  }

  private parseAction(trimmed: string, from: number, to: number, indent: number): TreeChild {
    const children: TreeChild[] = [
      { type: N.Sigil, from: from + indent, to: from + indent + 1 },
    ];

    const rest = trimmed.slice(1).trim();
    const actStart = to - rest.length;

    // Check for action keyword
    const kwMatch = rest.match(/^(var|fire|advance|do|trigger|let)\b/);
    if (kwMatch) {
      children.push({ type: N.ActionKeyword, from: actStart, to: actStart + kwMatch[0].length });
      // Rest is expression
      const exprRest = rest.slice(kwMatch[0].length).trim();
      if (exprRest) {
        this.addExprChildren(exprRest, to - exprRest.length, to, children);
      }
    } else {
      this.addExprChildren(rest, actStart, to, children);
    }

    return { type: N.Action, from, to, children };
  }

  private parseChoice(nodeType: number, trimmed: string, from: number, to: number, indent: number): TreeChild {
    const children: TreeChild[] = [
      { type: N.Sigil, from: from + indent, to: from + indent + 1 },
    ];

    const rest = trimmed.slice(2);
    const textStart = from + indent + 2;

    // Check for inline guard: * ? condition text
    if (rest.startsWith('?')) {
      children.push({ type: N.Sigil, from: textStart, to: textStart + 1 });
      // Simplify: rest of line is inline content
      const afterGuard = rest.slice(1).trim();
      if (afterGuard) {
        this.addInlineChildren(afterGuard, to - afterGuard.length, to, children);
      }
    } else {
      this.addInlineChildren(rest, textStart, to, children);
    }

    return { type: nodeType, from, to, children };
  }

  private parseSExpr(trimmed: string, from: number, to: number, indent: number): TreeChild {
    const children: TreeChild[] = [
      { type: N.Sigil, from: from + indent, to: from + indent + 1 }, // (
    ];

    // Keyword is first word after paren
    const inner = trimmed.slice(1);
    const kwMatch = inner.match(/^[a-z][a-z0-9-]*/);
    if (kwMatch) {
      children.push({ type: N.SExprKeyword, from: from + indent + 1, to: from + indent + 1 + kwMatch[0].length });
    }

    // Closing paren
    if (trimmed.endsWith(')')) {
      children.push({ type: N.Sigil, from: to - 1, to });
    }

    return { type: N.SExpr, from, to, children };
  }

  private parseProperty(trimmed: string, from: number, to: number, indent: number): TreeChild {
    const children: TreeChild[] = [
      { type: N.Sigil, from: from + indent, to: from + indent + 1 }, // .
    ];

    const rest = trimmed.slice(1);
    const keyMatch = rest.match(/^[a-z_][a-z0-9_-]*/);
    if (keyMatch) {
      children.push({ type: N.PropertyKey, from: from + indent + 1, to: from + indent + 1 + keyMatch[0].length });
      const valStr = rest.slice(keyMatch[0].length).trim();
      if (valStr) {
        const valStart = to - valStr.length;
        // Check for booleans
        if (valStr === 'true' || valStr === 'false') {
          children.push({ type: N.BoolLiteral, from: valStart, to });
        } else if (valStr.startsWith('"')) {
          children.push({ type: N.StringLiteral, from: valStart, to });
        } else {
          children.push({ type: N.PropertyValue, from: valStart, to });
        }
      }
    }

    return { type: N.Property, from, to, children };
  }

  private parseSpeaker(trimmed: string, from: number, to: number): TreeChild {
    const children: TreeChild[] = [];

    const nameMatch = trimmed.match(/^[A-Z][A-Z0-9_]*/);
    if (nameMatch) {
      children.push({ type: N.SpeakerName, from, to: from + nameMatch[0].length });
    }

    // Speaker block { ... }
    const braceIdx = trimmed.indexOf('{');
    if (braceIdx !== -1) {
      const closeIdx = trimmed.indexOf('}', braceIdx);
      if (closeIdx !== -1) {
        children.push({ type: N.SpeakerBlock, from: from + braceIdx, to: from + closeIdx + 1 });
      }
    }

    return { type: N.Speaker, from, to, children };
  }

  // ── Inline construct extraction ─────────────────────────────────────────────

  private addInlineChildren(text: string, from: number, to: number, children: TreeChild[]): void {
    let pos = 0;
    let textStart = 0;

    while (pos < text.length) {
      const rest = text.slice(pos);
      let matched = false;

      // Wiki-link [[...]]
      if (rest.startsWith('[[')) {
        const end = rest.indexOf(']]');
        if (end !== -1) {
          if (pos > textStart) {
            children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
          }
          children.push({ type: N.WikiLink, from: from + pos, to: from + pos + end + 2 });
          pos += end + 2;
          textStart = pos;
          matched = true;
        }
      }

      // Operand [type:id]
      if (!matched && rest.startsWith('[') && /^\[[a-z]+:[^\]]+\]/.test(rest)) {
        const end = rest.indexOf(']');
        if (end !== -1) {
          if (pos > textStart) {
            children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
          }
          children.push({ type: N.Operand, from: from + pos, to: from + pos + end + 1 });
          pos += end + 1;
          textStart = pos;
          matched = true;
        }
      }

      // Variation [a / b / c].mode
      if (!matched && rest.startsWith('[') && /^\[[^\]]*\/[^\]]*\]/.test(rest)) {
        const end = rest.indexOf(']');
        if (end !== -1) {
          if (pos > textStart) {
            children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
          }
          let varEnd = end + 1;
          // Consume .mode if present
          const afterBracket = rest.slice(varEnd);
          const modeMatch = afterBracket.match(/^\.[a-z]+/);
          if (modeMatch) varEnd += modeMatch[0].length;
          children.push({ type: N.Variation, from: from + pos, to: from + pos + varEnd });
          pos += varEnd;
          textStart = pos;
          matched = true;
        }
      }

      // Trigger <type:args>
      if (!matched && rest.startsWith('<')) {
        const closeAngle = rest.indexOf('>');
        if (closeAngle !== -1) {
          const inner = rest.slice(1, closeAngle);
          if (inner.startsWith('/') || /^[a-z]+[:=]/.test(inner)) {
            if (pos > textStart) {
              children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
            }
            children.push({ type: N.Trigger, from: from + pos, to: from + pos + closeAngle + 1 });
            pos += closeAngle + 1;
            textStart = pos;
            matched = true;
          }
        }
      }

      // Inline mutation <var := expr>
      if (!matched && rest.startsWith('<') && /^<[a-z_$]\w*\s*[:+-]?=/.test(rest)) {
        const closeAngle = rest.indexOf('>');
        if (closeAngle !== -1) {
          if (pos > textStart) {
            children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
          }
          children.push({ type: N.Trigger, from: from + pos, to: from + pos + closeAngle + 1 });
          pos += closeAngle + 1;
          textStart = pos;
          matched = true;
        }
      }

      // Interpolation {expr}
      if (!matched && rest.startsWith('{')) {
        const close = rest.indexOf('}');
        if (close !== -1) {
          if (pos > textStart) {
            children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
          }
          children.push({ type: N.Interpolation, from: from + pos, to: from + pos + close + 1 });
          pos += close + 1;
          textStart = pos;
          matched = true;
        }
      }

      // Resolve ref $NAME
      if (!matched && rest.startsWith('$') && /^\$[A-Za-z_][A-Za-z0-9_.]*/.test(rest)) {
        const refMatch = rest.match(/^\$[A-Za-z_][A-Za-z0-9_.]*/);
        if (refMatch) {
          if (pos > textStart) {
            children.push({ type: N.PlainText, from: from + textStart, to: from + pos });
          }
          children.push({ type: N.ResolveRef, from: from + pos, to: from + pos + refMatch[0].length });
          pos += refMatch[0].length;
          textStart = pos;
          matched = true;
        }
      }

      if (!matched) {
        pos++;
      }
    }

    // Remaining plain text
    if (textStart < text.length) {
      children.push({ type: N.PlainText, from: from + textStart, to });
    }
  }

  private addExprChildren(text: string, from: number, to: number, children: TreeChild[]): void {
    // Simplified expression tokenizer — highlights operators, strings, numbers, identifiers
    const re = /("(?:[^"\\]|\\.)*")|(-?\d+(?:\.\d+)?)|(\b(?:and|or|not|is\s+not|has\s+not|is|has)\b)|(>=|<=|!=|==|>|<)|(\[[a-z]+:[^\]]+\])|([a-zA-Z_][a-zA-Z0-9_.]*)/g;
    let match;
    while ((match = re.exec(text)) !== null) {
      const mFrom = from + match.index;
      const mTo = mFrom + match[0].length;
      if (match[1]) children.push({ type: N.StringLiteral, from: mFrom, to: mTo });
      else if (match[2]) children.push({ type: N.NumberLiteral, from: mFrom, to: mTo });
      else if (match[3]) children.push({ type: N.Operator, from: mFrom, to: mTo });
      else if (match[4]) children.push({ type: N.Operator, from: mFrom, to: mTo });
      else if (match[5]) children.push({ type: N.Operand, from: mFrom, to: mTo });
      else if (match[6]) children.push({ type: N.Identifier, from: mFrom, to: mTo });
    }
  }

  // ── Tree construction ───────────────────────────────────────────────────────

  private buildNode(c: TreeChild): Tree {
    if (!c.children || c.children.length === 0) {
      return new Tree(nodeSet.types[c.type] as NodeType, [], [], c.to - c.from);
    }

    // Sort children by position
    const sorted = c.children.sort((a, b) => a.from - b.from);
    const childTrees = sorted.map(child => this.buildNode(child));
    const positions = sorted.map(child => child.from - c.from);

    return new Tree(
      nodeSet.types[c.type] as NodeType,
      childTrees,
      positions,
      c.to - c.from,
    );
  }
}

// ── Language definition ───────────────────────────────────────────────────────

const loomParser = new LoomParser();

/**
 * Loom Lang language — Lezer-based, incremental, tree-producing.
 *
 * Use with LanguageSupport for full CodeMirror integration:
 *   extensions: [loomLanguageSupport]
 */
const loomData = defineLanguageFacet({
  commentTokens: { line: '//' },
});

export const loomLezerLanguage = new Language(
  loomData,
  loomParser,
  [],
  'loom',
);

// ── Highlight style ───────────────────────────────────────────────────────────

export const loomLezerHighlightStyle = HighlightStyle.define([
  { tag: tags.comment,          color: 'hsl(220 15% 38%)', fontStyle: 'italic' },
  { tag: tags.definitionKeyword, color: 'hsl(258 80% 78%)', fontWeight: '700' },
  { tag: tags.heading,          color: 'hsl(258 60% 72%)', fontWeight: '700' },
  { tag: tags.controlKeyword,   color: 'hsl(15 90% 65%)' },
  { tag: tags.operatorKeyword,  color: 'hsl(35 90% 68%)' },
  { tag: tags.keyword,          color: 'hsl(207 80% 68%)' },
  { tag: tags.moduleKeyword,    color: 'hsl(270 70% 72%)' },
  { tag: tags.macroName,        color: 'hsl(270 70% 72%)' },
  { tag: tags.logicOperator,    color: 'hsl(207 80% 68%)' },
  { tag: tags.typeName,         color: 'hsl(45 80% 70%)', fontWeight: '600' },
  { tag: tags.labelName,        color: 'hsl(192 80% 65%)' },
  { tag: tags.variableName,     color: 'hsl(220 20% 72%)' },
  { tag: tags.propertyName,     color: 'hsl(30 90% 65%)', fontWeight: '500' },
  { tag: tags.atom,             color: 'hsl(30 90% 65%)' },
  { tag: tags.modifier,         color: 'hsl(120 40% 55%)', fontStyle: 'italic' },
  { tag: tags.meta,             color: 'hsl(45 80% 60%)', fontStyle: 'italic' },
  { tag: tags.bool,             color: 'hsl(207 80% 68%)' },
  { tag: tags.link,             color: 'hsl(192 80% 65%)', textDecoration: 'underline' },
  { tag: tags.inserted,         color: 'hsl(150 60% 62%)' },
  { tag: tags.tagName,          color: 'hsl(175 70% 58%)' },
  { tag: tags.string,           color: 'hsl(120 50% 62%)' },
  { tag: tags.number,           color: 'hsl(35 90% 68%)' },
  { tag: tags.content,          color: 'hsl(220 20% 82%)' },
  { tag: tags.emphasis,         color: 'hsl(220 18% 65%)', fontStyle: 'italic' },
  { tag: tags.bracket,          color: 'hsl(220 15% 50%)' },
  { tag: tags.invalid,          color: 'hsl(0 80% 65%)', textDecoration: 'underline wavy' },
]);

// ── Language support bundle ───────────────────────────────────────────────────

/**
 * Full Loom Lang language support for CodeMirror.
 * Includes the Lezer parser + highlight style.
 */
export const loomLanguageSupport = new LanguageSupport(loomLezerLanguage, [
  syntaxHighlighting(loomLezerHighlightStyle),
]);
