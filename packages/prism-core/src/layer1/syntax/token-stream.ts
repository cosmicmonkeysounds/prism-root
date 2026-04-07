/**
 * TokenStream — generic typed token stream for building recursive descent parsers.
 *
 * Works with any token type that extends the base Token interface.
 * Provides peek/advance/check/eat/expect plus save/restore for backtracking.
 *
 * @example
 *   interface MyToken extends BaseToken<'NUM' | 'PLUS' | 'EOF'> {}
 *   const stream = new TokenStream<MyToken>(tokens);
 *   while (!stream.isAtEnd()) {
 *     if (stream.check('PLUS')) { stream.advance(); }
 *   }
 */

// ── Base token ────────────────────────────────────────────────────────────────

/**
 * Base token interface for use with TokenStream.
 * Language-specific tokenizers extend this with their own TKind union.
 */
export interface BaseToken<TKind extends string = string> {
  readonly kind: TKind;
  readonly raw: string;
  readonly offset: number;
  readonly line: number;
  readonly column: number;
}

/** Alias for BaseToken with default type parameter. Used in error types. */
export type Token = BaseToken;

// ── Stream error ──────────────────────────────────────────────────────────────

export class TokenError extends Error {
  readonly token: Token;

  constructor(message: string, token: Token) {
    super(`${message} at ${token.line}:${token.column} (got '${token.raw}')`);
    this.name = 'TokenError';
    this.token = token;
  }
}

// ── TokenStream ───────────────────────────────────────────────────────────────

export class TokenStream<T extends BaseToken> {
  private readonly _tokens: readonly T[];
  private _pos: number;

  constructor(tokens: readonly T[]) {
    this._tokens = tokens;
    this._pos = 0;
  }

  /** Current position index. */
  get pos(): number { return this._pos; }

  /** All tokens in the stream. */
  get tokens(): readonly T[] { return this._tokens; }

  /** Look at the current token (or ahead by N). Returns EOF token if past end. */
  peek(ahead = 0): T {
    const idx = this._pos + ahead;
    if (idx >= this._tokens.length) {
      return this._tokens[this._tokens.length - 1]!; // should be EOF
    }
    return this._tokens[idx]!;
  }

  /** Consume and return the current token. */
  advance(): T {
    const tok = this._tokens[this._pos]!;
    if (this._pos < this._tokens.length - 1) {
      this._pos++;
    }
    return tok;
  }

  /** Check if the current token has the given kind. */
  check(kind: T['kind']): boolean {
    return this.peek().kind === kind;
  }

  /** If the current token matches, consume and return it. Otherwise return null. */
  eat(kind: T['kind']): T | null {
    if (this.check(kind)) return this.advance();
    return null;
  }

  /** Consume the current token if it matches, otherwise throw TokenError. */
  expect(kind: T['kind'], message?: string): T {
    if (this.check(kind)) return this.advance();
    const tok = this.peek();
    throw new TokenError(message ?? `Expected '${kind}'`, tok);
  }

  /** True if we're at the last token (should be EOF). */
  isAtEnd(): boolean {
    return this._pos >= this._tokens.length - 1;
  }

  // ── Backtracking ──────────────────────────────────────────────────────────

  /** Save current position for backtracking. */
  save(): number {
    return this._pos;
  }

  /** Restore to a previously saved position. */
  restore(pos: number): void {
    this._pos = pos;
  }
}
