/**
 * Scanner — generic character-level cursor for building tokenizers.
 *
 * Provides position tracking (line/column), peek/advance/match operations,
 * save/restore for backtracking, and common scanning helpers (identifiers,
 * numbers, quoted strings). Language-specific tokenizers compose on top of
 * this rather than reimplementing character-level logic with regexes.
 *
 * @example
 *   const scanner = new Scanner('hello + 42');
 *   const ident = scanner.scanIdentifier();  // 'hello'
 *   scanner.skipWhitespace();
 *   scanner.expect('+');
 *   scanner.skipWhitespace();
 *   const num = scanner.scanNumber();         // 42
 */

import type { Position, SourceRange } from './ast-types';

// ── State snapshot ────────────────────────────────────────────────────────────

export interface ScannerState {
  readonly offset: number;
  readonly line: number;
  readonly column: number;
}

// ── Error ─────────────────────────────────────────────────────────────────────

export class ScanError extends Error {
  readonly position: Position;

  constructor(message: string, position: Position) {
    super(`${message} at ${position.line}:${position.column}`);
    this.name = 'ScanError';
    this.position = position;
  }
}

// ── Scanner ───────────────────────────────────────────────────────────────────

export class Scanner {
  readonly source: string;
  private _offset: number;
  private _line: number;
  private _column: number;

  constructor(source: string) {
    this.source = source;
    this._offset = 0;
    this._line = 1;
    this._column = 0;
  }

  // ── Position ──────────────────────────────────────────────────────────────

  get offset(): number { return this._offset; }
  get line(): number { return this._line; }
  get column(): number { return this._column; }

  get position(): Position {
    return { offset: this._offset, line: this._line, column: this._column };
  }

  get isAtEnd(): boolean {
    return this._offset >= this.source.length;
  }

  // ── Save / restore (backtracking) ─────────────────────────────────────────

  save(): ScannerState {
    return { offset: this._offset, line: this._line, column: this._column };
  }

  restore(state: ScannerState): void {
    this._offset = state.offset;
    this._line = state.line;
    this._column = state.column;
  }

  // ── Character access ──────────────────────────────────────────────────────

  /** Return character at current position + ahead, without consuming. Empty string if past end. */
  peek(ahead = 0): string {
    const idx = this._offset + ahead;
    return idx < this.source.length ? (this.source[idx] ?? '') : '';
  }

  /** Consume and return current character. Empty string if at end. */
  advance(): string {
    if (this._offset >= this.source.length) return '';
    const ch = this.source[this._offset] ?? '';
    this._offset++;
    if (ch === '\n') {
      this._line++;
      this._column = 0;
    } else {
      this._column++;
    }
    return ch;
  }

  /** Return a substring of the source. */
  slice(start: number, end: number): string {
    return this.source.slice(start, end);
  }

  // ── Matching ──────────────────────────────────────────────────────────────

  /** If the upcoming characters match `expected`, consume them and return true. */
  match(expected: string): boolean {
    if (this.source.startsWith(expected, this._offset)) {
      for (let i = 0; i < expected.length; i++) this.advance();
      return true;
    }
    return false;
  }

  /**
   * Try to match a regex at the current position.
   * The regex is anchored to the current offset. Returns the match or null.
   * On success, the scanner advances past the match.
   */
  matchRegex(re: RegExp): RegExpExecArray | null {
    // Create a sticky copy so we match at exactly this offset
    const sticky = new RegExp(re.source, re.flags.replace(/[gy]/g, '') + 'y');
    sticky.lastIndex = this._offset;
    const m = sticky.exec(this.source);
    if (m) {
      for (let i = 0; i < m[0].length; i++) this.advance();
    }
    return m;
  }

  /** Consume expected string or throw ScanError. */
  expect(expected: string, errorMsg?: string): void {
    if (!this.match(expected)) {
      throw this.error(errorMsg ?? `Expected '${expected}'`);
    }
  }

  // ── Scanning helpers ──────────────────────────────────────────────────────

  /** Skip whitespace (spaces, tabs) but NOT newlines. */
  skipWhitespace(): void {
    while (!this.isAtEnd) {
      const ch = this.peek();
      if (ch === ' ' || ch === '\t') {
        this.advance();
      } else {
        break;
      }
    }
  }

  /** Skip whitespace including newlines. */
  skipWhitespaceAndNewlines(): void {
    while (!this.isAtEnd) {
      const ch = this.peek();
      if (ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r') {
        this.advance();
      } else {
        break;
      }
    }
  }

  /** Consume characters while predicate returns true. Returns consumed string. */
  scanWhile(predicate: (ch: string) => boolean): string {
    const start = this._offset;
    while (!this.isAtEnd && predicate(this.peek())) {
      this.advance();
    }
    return this.source.slice(start, this._offset);
  }

  /** Scan a quoted string with escape handling. Assumes scanner is ON the opening quote. */
  scanString(quote: string): string {
    this.expect(quote);
    let str = '';
    while (!this.isAtEnd && this.peek() !== quote) {
      if (this.peek() === '\\' && this._offset + 1 < this.source.length) {
        this.advance(); // consume backslash
        const esc = this.advance();
        switch (esc) {
          case 'n': str += '\n'; break;
          case 't': str += '\t'; break;
          case 'r': str += '\r'; break;
          case '\\': str += '\\'; break;
          case '0': str += '\0'; break;
          default:
            if (esc === quote) { str += quote; }
            else { str += '\\' + esc; }
            break;
        }
      } else {
        str += this.advance();
      }
    }
    this.expect(quote, `Unterminated string literal`);
    return str;
  }

  /** Scan a numeric literal (integer or float, optional exponent). Returns the parsed number. */
  scanNumber(): number {
    const start = this._offset;

    // Optional leading minus
    if (this.peek() === '-') this.advance();

    // Integer part
    if (this.peek() === '.') {
      // Leading dot: .5
      this.advance();
      this._scanDigits();
    } else {
      this._scanDigits();
      // Fractional part
      if (this.peek() === '.' && isDigit(this.peek(1))) {
        this.advance(); // consume dot
        this._scanDigits();
      }
    }

    // Exponent
    if (this.peek() === 'e' || this.peek() === 'E') {
      this.advance();
      if (this.peek() === '+' || this.peek() === '-') this.advance();
      this._scanDigits();
    }

    const raw = this.source.slice(start, this._offset);
    const value = parseFloat(raw);
    if (isNaN(value)) throw this.error(`Invalid number '${raw}'`);
    return value;
  }

  private _scanDigits(): void {
    while (!this.isAtEnd && isDigit(this.peek())) {
      this.advance();
    }
  }

  /** Scan an identifier: [a-zA-Z_][a-zA-Z0-9_]*. Returns the identifier string. */
  scanIdentifier(): string {
    const ch = this.peek();
    if (!isIdentStart(ch)) {
      throw this.error(`Expected identifier, got '${ch || 'EOF'}'`);
    }
    return this.scanWhile(isIdentChar);
  }

  // ── Source mapping ────────────────────────────────────────────────────────

  /** Build a SourceRange from a start offset to current position. */
  rangeFrom(startOffset: number): SourceRange {
    return {
      start: this.posAt(startOffset),
      end: this.position,
    };
  }

  /** Compute Position for any offset in the source. */
  posAt(offset: number): Position {
    let line = 1;
    let column = 0;
    for (let i = 0; i < offset && i < this.source.length; i++) {
      if (this.source[i] === '\n') {
        line++;
        column = 0;
      } else {
        column++;
      }
    }
    return { offset, line, column };
  }

  // ── Error helpers ─────────────────────────────────────────────────────────

  /** Create a ScanError at the current position. */
  error(message: string): ScanError {
    return new ScanError(message, this.position);
  }
}

// ── Character classification ────────────────────────────────────────────────

function isDigit(ch: string): boolean {
  return ch >= '0' && ch <= '9';
}

function isIdentStart(ch: string): boolean {
  return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch === '_';
}

function isIdentChar(ch: string): boolean {
  return isIdentStart(ch) || isDigit(ch);
}

export { isDigit, isIdentStart, isIdentChar };
