export type TokenKind =
  | "NUMBER"
  | "STRING"
  | "BOOL"
  | "OPERAND"
  | "IDENT"
  | "PLUS"
  | "MINUS"
  | "STAR"
  | "SLASH"
  | "CARET"
  | "PERCENT"
  | "EQ"
  | "NEQ"
  | "LT"
  | "LTE"
  | "GT"
  | "GTE"
  | "AND"
  | "OR"
  | "NOT"
  | "LPAREN"
  | "RPAREN"
  | "COMMA"
  | "EOF"
  | "UNKNOWN";

export interface OperandData {
  operandType: string;
  id: string;
  subfield?: string;
}

export interface Token {
  kind: TokenKind;
  raw: string;
  offset: number;
  numberValue?: number;
  stringValue?: string;
  boolValue?: boolean;
  operandData?: OperandData;
}

export function isDigit(ch: string): boolean {
  return ch >= "0" && ch <= "9";
}

export function isIdentStart(ch: string): boolean {
  return (ch >= "a" && ch <= "z") || (ch >= "A" && ch <= "Z") || ch === "_";
}

export function isIdentChar(ch: string): boolean {
  return isIdentStart(ch) || isDigit(ch);
}

export function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < source.length) {
    // Skip whitespace
    if (source[i] === " " || source[i] === "\t" || source[i] === "\n" || source[i] === "\r") {
      i++;
      continue;
    }

    const start = i;
    const ch = source[i] as string;

    // Operand: [type:id] or [type:id.subfield]
    if (ch === "[") {
      const close = source.indexOf("]", i + 1);
      if (close >= 0) {
        const inner = source.slice(i + 1, close);
        const colonIdx = inner.indexOf(":");
        if (colonIdx >= 0) {
          const operandType = inner.slice(0, colonIdx).trim();
          const rest = inner.slice(colonIdx + 1).trim();
          const dotIdx = rest.indexOf(".");
          i = close + 1;
          const data: OperandData = dotIdx >= 0
            ? { operandType, id: rest.slice(0, dotIdx), subfield: rest.slice(dotIdx + 1) }
            : { operandType, id: rest };
          tokens.push({
            kind: "OPERAND",
            raw: source.slice(start, i),
            offset: start,
            operandData: data,
          });
          continue;
        }
      }
    }

    // Number
    if (isDigit(ch) || (ch === "." && i + 1 < source.length && isDigit(source[i + 1] as string))) {
      let num = "";
      while (i < source.length && isDigit(source[i] as string)) {
        num += source[i];
        i++;
      }
      if (i < source.length && source[i] === ".") {
        num += ".";
        i++;
        while (i < source.length && isDigit(source[i] as string)) {
          num += source[i];
          i++;
        }
      }
      tokens.push({ kind: "NUMBER", raw: num, offset: start, numberValue: parseFloat(num) });
      continue;
    }

    // String
    if (ch === '"' || ch === "'") {
      const quote = ch;
      i++;
      let str = "";
      while (i < source.length && source[i] !== quote) {
        if (source[i] === "\\") {
          i++;
          const esc = source[i];
          if (esc === "n") str += "\n";
          else if (esc === "t") str += "\t";
          else if (esc === "r") str += "\r";
          else if (esc === "\\") str += "\\";
          else if (esc === quote) str += quote;
          else str += esc;
        } else {
          str += source[i];
        }
        i++;
      }
      if (i < source.length) i++; // skip closing quote
      tokens.push({
        kind: "STRING",
        raw: source.slice(start, i),
        offset: start,
        stringValue: str,
      });
      continue;
    }

    // Identifier or keyword
    if (isIdentStart(ch)) {
      let ident = "";
      while (i < source.length && isIdentChar(source[i] as string)) {
        ident += source[i];
        i++;
      }
      const lower = ident.toLowerCase();
      if (lower === "true" || lower === "false") {
        tokens.push({
          kind: "BOOL",
          raw: ident,
          offset: start,
          boolValue: lower === "true",
        });
      } else if (lower === "and") {
        tokens.push({ kind: "AND", raw: ident, offset: start });
      } else if (lower === "or") {
        tokens.push({ kind: "OR", raw: ident, offset: start });
      } else if (lower === "not") {
        tokens.push({ kind: "NOT", raw: ident, offset: start });
      } else {
        tokens.push({ kind: "IDENT", raw: ident, offset: start });
      }
      continue;
    }

    // Two-char operators
    if (i + 1 < source.length) {
      const two = source.slice(i, i + 2);
      if (two === "==") {
        tokens.push({ kind: "EQ", raw: two, offset: start });
        i += 2;
        continue;
      }
      if (two === "!=") {
        tokens.push({ kind: "NEQ", raw: two, offset: start });
        i += 2;
        continue;
      }
      if (two === "<=") {
        tokens.push({ kind: "LTE", raw: two, offset: start });
        i += 2;
        continue;
      }
      if (two === ">=") {
        tokens.push({ kind: "GTE", raw: two, offset: start });
        i += 2;
        continue;
      }
    }

    // Single-char operators
    const singles: Record<string, TokenKind> = {
      "+": "PLUS",
      "-": "MINUS",
      "*": "STAR",
      "/": "SLASH",
      "^": "CARET",
      "%": "PERCENT",
      "<": "LT",
      ">": "GT",
      "(": "LPAREN",
      ")": "RPAREN",
      ",": "COMMA",
    };
    if (singles[ch]) {
      tokens.push({ kind: singles[ch], raw: ch, offset: start });
      i++;
      continue;
    }

    // Unknown character
    tokens.push({ kind: "UNKNOWN", raw: ch, offset: start });
    i++;
  }

  tokens.push({ kind: "EOF", raw: "", offset: i });
  return tokens;
}
