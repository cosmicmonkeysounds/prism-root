//! Recursive-descent expression parser.
//!
//! Port of `language/expression/parser.ts`. Precedence chain:
//!
//!   or → and → not → comparison → additive → multiplicative →
//!   power → unary → primary
//!
//! Errors are collected into `ParseResult.errors` so callers get
//! the full list in one pass rather than fail-fast.

use super::expression_types::{
    AnyExprNode, BinaryOp, ExprError, ExprType, ExprValue, ParseResult, UnaryOp,
};
use super::scanner::{tokenize, Token, TokenKind};

pub fn parse(source: &str) -> ParseResult {
    let tokens = tokenize(source);
    let mut parser = Parser {
        tokens,
        pos: 0,
        errors: Vec::new(),
    };

    if parser.check(TokenKind::Eof) {
        return ParseResult {
            node: None,
            errors: Vec::new(),
        };
    }

    let node = parser.parse_expr();

    if !parser.check(TokenKind::Eof) {
        let tok = parser.peek().clone();
        parser.errors.push(ExprError {
            message: format!("Unexpected token: '{}'", tok.raw),
            offset: Some(tok.offset),
        });
    }

    ParseResult {
        node: Some(node),
        errors: parser.errors,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ExprError>,
}

impl Parser {
    fn peek(&self) -> &Token {
        let idx = self.pos.min(self.tokens.len() - 1);
        &self.tokens[idx]
    }

    fn advance(&mut self) -> Token {
        if self.pos >= self.tokens.len() {
            return self.tokens.last().cloned().unwrap();
        }
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        if self.check(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, message: &str) {
        if self.check(kind) {
            self.advance();
        } else {
            let tok = self.peek().clone();
            self.errors.push(ExprError {
                message: message.to_string(),
                offset: Some(tok.offset),
            });
        }
    }

    // ── Precedence chain ──────────────────────────────────────────

    fn parse_expr(&mut self) -> AnyExprNode {
        self.parse_ternary()
    }

    /// `cond ? then : else_` — right-associative. Trees as
    /// `a ? b : c ? d : e` → `a ? b : (c ? d : e)`. Eats only one
    /// `?` per call; the recursive `parse_ternary()` for the else
    /// branch chains the rest. Returns the head when there's no `?`,
    /// so non-ternary expressions stay shape-identical.
    fn parse_ternary(&mut self) -> AnyExprNode {
        let cond = self.parse_or();
        if self.eat(TokenKind::Question).is_some() {
            let then_branch = self.parse_or();
            self.expect(TokenKind::Colon, "Expected ':' in ternary expression");
            let else_branch = self.parse_ternary();
            return AnyExprNode::Conditional {
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            };
        }
        cond
    }

    fn parse_or(&mut self) -> AnyExprNode {
        let mut left = self.parse_and();
        while self.eat(TokenKind::Or).is_some() {
            let right = self.parse_and();
            left = AnyExprNode::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_and(&mut self) -> AnyExprNode {
        let mut left = self.parse_not();
        while self.eat(TokenKind::And).is_some() {
            let right = self.parse_not();
            left = AnyExprNode::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_not(&mut self) -> AnyExprNode {
        if self.eat(TokenKind::Not).is_some() {
            let operand = self.parse_not();
            return AnyExprNode::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            };
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> AnyExprNode {
        let left = self.parse_additive();
        let kind = self.peek().kind;
        let op = match kind {
            TokenKind::Eq => Some(BinaryOp::Eq),
            TokenKind::Neq => Some(BinaryOp::Ne),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Lte => Some(BinaryOp::Lte),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Gte => Some(BinaryOp::Gte),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let right = self.parse_additive();
            return AnyExprNode::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_additive(&mut self) -> AnyExprNode {
        let mut left = self.parse_multiplicative();
        while matches!(self.peek().kind, TokenKind::Plus | TokenKind::Minus) {
            let op = if self.advance().kind == TokenKind::Plus {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let right = self.parse_multiplicative();
            left = AnyExprNode::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_multiplicative(&mut self) -> AnyExprNode {
        let mut left = self.parse_power();
        while matches!(
            self.peek().kind,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent
        ) {
            let op = match self.advance().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => BinaryOp::Mod,
            };
            let right = self.parse_power();
            left = AnyExprNode::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_power(&mut self) -> AnyExprNode {
        let base = self.parse_unary();
        if self.eat(TokenKind::Caret).is_some() {
            let exp = self.parse_power();
            return AnyExprNode::Binary {
                op: BinaryOp::Pow,
                left: Box::new(base),
                right: Box::new(exp),
            };
        }
        base
    }

    fn parse_unary(&mut self) -> AnyExprNode {
        if self.eat(TokenKind::Minus).is_some() {
            let operand = self.parse_unary();
            return AnyExprNode::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(operand),
            };
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> AnyExprNode {
        if self.check(TokenKind::Number) {
            let t = self.advance();
            return AnyExprNode::Literal {
                value: ExprValue::Number(t.number_value.unwrap_or(0.0)),
                expr_type: ExprType::Number,
            };
        }
        if self.check(TokenKind::String) {
            let t = self.advance();
            return AnyExprNode::Literal {
                value: ExprValue::String(t.string_value.unwrap_or_default()),
                expr_type: ExprType::String,
            };
        }
        if self.check(TokenKind::Bool) {
            let t = self.advance();
            return AnyExprNode::Literal {
                value: ExprValue::Boolean(t.bool_value.unwrap_or(false)),
                expr_type: ExprType::Boolean,
            };
        }
        if self.check(TokenKind::Operand) {
            let t = self.advance();
            let d = t.operand_data.unwrap();
            return AnyExprNode::Operand {
                operand_type: d.operand_type,
                id: d.id,
                subfield: d.subfield,
            };
        }
        if self.check(TokenKind::Ident) {
            let t = self.advance();
            if self.eat(TokenKind::LParen).is_some() {
                let mut args = Vec::new();
                if !self.check(TokenKind::RParen) {
                    args.push(self.parse_expr());
                    while self.eat(TokenKind::Comma).is_some() {
                        args.push(self.parse_expr());
                    }
                }
                self.expect(TokenKind::RParen, "Expected ')' after function arguments");
                return AnyExprNode::Call { name: t.raw, args };
            }
            // Chain dotted-path segments into `subfield` — `item.label`
            // and `tabs.0.name` both parse to a single Operand whose
            // ValueStore traverses the path. `Ident.Number.Ident` is
            // accepted so JSON array index segments work uniformly.
            let mut subfield: Option<String> = None;
            while self.eat(TokenKind::Dot).is_some() {
                // Identifier or numeric (array-index) segment both
                // capture as the segment's raw text; the ValueStore
                // walks the dotted path uniformly. Anything else
                // terminates the chain with an error.
                let seg = if matches!(self.peek().kind, TokenKind::Ident | TokenKind::Number) {
                    self.advance().raw
                } else {
                    let tok = self.peek().clone();
                    self.errors.push(ExprError {
                        message: format!(
                            "Expected identifier or index after '.' in operand path, got '{}'",
                            tok.raw
                        ),
                        offset: Some(tok.offset),
                    });
                    break;
                };
                subfield = Some(match subfield {
                    Some(prev) => format!("{prev}.{seg}"),
                    None => seg,
                });
            }
            return AnyExprNode::Operand {
                operand_type: "field".to_string(),
                id: t.raw,
                subfield,
            };
        }
        if self.eat(TokenKind::LParen).is_some() {
            let expr = self.parse_expr();
            self.expect(TokenKind::RParen, "Expected ')'");
            return expr;
        }

        let tok = self.peek().clone();
        self.errors.push(ExprError {
            message: format!("Unexpected token: '{}'", tok.raw),
            offset: Some(tok.offset),
        });
        self.advance();
        AnyExprNode::Literal {
            value: ExprValue::Number(0.0),
            expr_type: ExprType::Number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_to_none() {
        let r = parse("");
        assert!(r.node.is_none());
        assert!(r.errors.is_empty());
    }

    #[test]
    fn parses_arithmetic_precedence() {
        let r = parse("1 + 2 * 3");
        let node = r.node.unwrap();
        match node {
            AnyExprNode::Binary {
                op: BinaryOp::Add,
                right,
                ..
            } => match *right {
                AnyExprNode::Binary {
                    op: BinaryOp::Mul, ..
                } => {}
                _ => panic!("expected mul on right"),
            },
            _ => panic!("expected add at root"),
        }
    }

    #[test]
    fn power_is_right_associative() {
        let r = parse("2 ^ 3 ^ 2");
        let node = r.node.unwrap();
        match node {
            AnyExprNode::Binary {
                op: BinaryOp::Pow,
                right,
                ..
            } => match *right {
                AnyExprNode::Binary {
                    op: BinaryOp::Pow, ..
                } => {}
                _ => panic!("expected nested pow on right"),
            },
            _ => panic!("expected pow at root"),
        }
    }

    #[test]
    fn parses_function_call() {
        let r = parse("abs(-5)");
        let node = r.node.unwrap();
        match node {
            AnyExprNode::Call { name, args } => {
                assert_eq!(name, "abs");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn parses_bare_ident_as_field_operand() {
        let r = parse("foo");
        match r.node.unwrap() {
            AnyExprNode::Operand {
                operand_type,
                id,
                subfield,
            } => {
                assert_eq!(operand_type, "field");
                assert_eq!(id, "foo");
                assert!(subfield.is_none());
            }
            _ => panic!("expected operand"),
        }
    }

    #[test]
    fn parses_operand_with_subfield() {
        let r = parse("[field:foo.bar]");
        match r.node.unwrap() {
            AnyExprNode::Operand { subfield, .. } => {
                assert_eq!(subfield.as_deref(), Some("bar"));
            }
            _ => panic!("expected operand"),
        }
    }

    #[test]
    fn reports_unclosed_paren() {
        let r = parse("(1 + 2");
        assert!(!r.errors.is_empty());
    }

    #[test]
    fn parses_dotted_path_into_operand_subfield() {
        let r = parse("item.label");
        match r.node.unwrap() {
            AnyExprNode::Operand { id, subfield, .. } => {
                assert_eq!(id, "item");
                assert_eq!(subfield.as_deref(), Some("label"));
            }
            _ => panic!("expected operand"),
        }
    }

    #[test]
    fn parses_deep_dotted_path_with_numeric_segment() {
        let r = parse("tabs.0.name");
        match r.node.unwrap() {
            AnyExprNode::Operand { id, subfield, .. } => {
                assert_eq!(id, "tabs");
                assert_eq!(subfield.as_deref(), Some("0.name"));
            }
            _ => panic!("expected operand"),
        }
    }

    #[test]
    fn parses_ternary_right_associative() {
        let r = parse("a ? b : c ? d : e");
        match r.node.unwrap() {
            AnyExprNode::Conditional { else_branch, .. } => match *else_branch {
                AnyExprNode::Conditional { .. } => {}
                other => panic!("expected nested ternary in else branch, got {other:?}"),
            },
            other => panic!("expected ternary at root, got {other:?}"),
        }
    }

    #[test]
    fn parses_c_style_logical_operators() {
        let r = parse("a && b || !c");
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        // root is `||` due to lower precedence
        match r.node.unwrap() {
            AnyExprNode::Binary {
                op: BinaryOp::Or, ..
            } => {}
            other => panic!("expected `||` at root, got {other:?}"),
        }
    }
}
