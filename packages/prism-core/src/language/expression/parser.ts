import type {
  AnyExprNode,
  BinaryOp,
  ExprError,
  ParseResult,
} from "./expression-types.js";
import { tokenize, type Token, type TokenKind } from "./scanner.js";

export function parse(source: string): ParseResult {
  const tokens = tokenize(source);
  const errors: ExprError[] = [];
  let pos = 0;

  const eof: Token = tokens[tokens.length - 1] as Token;

  function peek(): Token {
    return pos < tokens.length ? (tokens[pos] as Token) : eof;
  }

  function advance(): Token {
    if (pos >= tokens.length) return eof;
    const t = tokens[pos] as Token;
    pos++;
    return t;
  }

  function check(kind: TokenKind): boolean {
    return peek().kind === kind;
  }

  function eat(kind: TokenKind): Token | null {
    if (check(kind)) return advance();
    return null;
  }

  function expect(kind: TokenKind, message: string): Token {
    if (check(kind)) return advance();
    errors.push({ message, offset: peek().offset });
    return peek();
  }

  // ── Precedence chain ──────────────────────────────────────────────────────

  function parseExpr(): AnyExprNode {
    return parseOr();
  }

  function parseOr(): AnyExprNode {
    let left = parseAnd();
    while (eat("OR")) {
      const right = parseAnd();
      left = { kind: "binary", op: "or", left, right };
    }
    return left;
  }

  function parseAnd(): AnyExprNode {
    let left = parseNot();
    while (eat("AND")) {
      const right = parseNot();
      left = { kind: "binary", op: "and", left, right };
    }
    return left;
  }

  function parseNot(): AnyExprNode {
    if (eat("NOT")) {
      const operand = parseNot();
      return { kind: "unary", op: "not", operand };
    }
    return parseComparison();
  }

  function parseComparison(): AnyExprNode {
    let left = parseAdditive();
    const compOps: Record<string, BinaryOp> = {
      EQ: "==",
      NEQ: "!=",
      LT: "<",
      LTE: "<=",
      GT: ">",
      GTE: ">=",
    };
    const kind = peek().kind;
    if (compOps[kind]) {
      advance();
      const right = parseAdditive();
      left = { kind: "binary", op: compOps[kind], left, right };
    }
    return left;
  }

  function parseAdditive(): AnyExprNode {
    let left = parseMultiplicative();
    while (check("PLUS") || check("MINUS")) {
      const op: BinaryOp = advance().kind === "PLUS" ? "+" : "-";
      const right = parseMultiplicative();
      left = { kind: "binary", op, left, right };
    }
    return left;
  }

  function parseMultiplicative(): AnyExprNode {
    let left = parsePower();
    while (check("STAR") || check("SLASH") || check("PERCENT")) {
      const t = advance();
      const op: BinaryOp = t.kind === "STAR" ? "*" : t.kind === "SLASH" ? "/" : "%";
      const right = parsePower();
      left = { kind: "binary", op, left, right };
    }
    return left;
  }

  function parsePower(): AnyExprNode {
    const base = parseUnary();
    if (eat("CARET")) {
      const exp = parsePower(); // right-associative
      return { kind: "binary", op: "^", left: base, right: exp };
    }
    return base;
  }

  function parseUnary(): AnyExprNode {
    if (eat("MINUS")) {
      const operand = parseUnary();
      return { kind: "unary", op: "-", operand };
    }
    return parsePrimary();
  }

  function parsePrimary(): AnyExprNode {
    // Number literal
    if (check("NUMBER")) {
      const t = advance();
      return { kind: "literal", value: t.numberValue ?? 0, exprType: "number" };
    }

    // String literal
    if (check("STRING")) {
      const t = advance();
      return { kind: "literal", value: t.stringValue ?? "", exprType: "string" };
    }

    // Boolean literal
    if (check("BOOL")) {
      const t = advance();
      return { kind: "literal", value: t.boolValue ?? false, exprType: "boolean" };
    }

    // Operand [type:id] or [type:id.subfield]
    if (check("OPERAND")) {
      const t = advance();
      const d = t.operandData as NonNullable<typeof t.operandData>;
      const operandNode: AnyExprNode = d.subfield
        ? { kind: "operand", operandType: d.operandType, id: d.id, subfield: d.subfield }
        : { kind: "operand", operandType: d.operandType, id: d.id };
      return operandNode;
    }

    // Identifier — function call or bare name
    if (check("IDENT")) {
      const t = advance();
      if (eat("LPAREN")) {
        // Function call
        const args: AnyExprNode[] = [];
        if (!check("RPAREN")) {
          args.push(parseExpr());
          while (eat("COMMA")) {
            args.push(parseExpr());
          }
        }
        expect("RPAREN", "Expected ')' after function arguments");
        return { kind: "call", name: t.raw, args };
      }
      // Bare identifier treated as operand with type "field"
      return { kind: "operand", operandType: "field", id: t.raw };
    }

    // Parenthesized expression
    if (eat("LPAREN")) {
      const expr = parseExpr();
      expect("RPAREN", "Expected ')'");
      return expr;
    }

    // Error recovery
    errors.push({ message: `Unexpected token: '${peek().raw}'`, offset: peek().offset });
    advance();
    return { kind: "literal", value: 0, exprType: "number" };
  }

  // ── Entry point ───────────────────────────────────────────────────────────

  if (check("EOF")) {
    return { node: null, errors: [] };
  }

  const node = parseExpr();

  if (!check("EOF")) {
    errors.push({ message: `Unexpected token: '${peek().raw}'`, offset: peek().offset });
  }

  return { node, errors };
}
