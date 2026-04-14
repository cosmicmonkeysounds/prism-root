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
        self.parse_or()
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
            return AnyExprNode::Operand {
                operand_type: "field".to_string(),
                id: t.raw,
                subfield: None,
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
}
