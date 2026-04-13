/**
 * Prometheus text-format parser built on Prism's syntax Scanner.
 *
 * Prism convention: all external-format parsing routes through
 * `@prism/core/syntax`'s Scanner/Token primitives rather than hand-rolled
 * regex/string indexing. That gives us line/column tracking, backtracking,
 * and consistent error handling with the rest of the language tooling.
 *
 * This is a partial implementation of the Prometheus exposition format —
 * enough to extract the counters/gauges/histograms that Prism Relay emits.
 */

import { Scanner, ScanError, isDigit, isIdentChar, isIdentStart } from "@prism/core/syntax";

/** Prometheus allows `:` in metric names on top of the usual identifier charset. */
const isPromNameStart = (ch: string): boolean => isIdentStart(ch) || ch === ":";
const isPromNameChar = (ch: string): boolean => isIdentChar(ch) || ch === ":";

export interface PromSample {
  name: string;
  labels: Record<string, string>;
  value: number;
}

export function parsePrometheus(text: string): PromSample[] {
  const scanner = new Scanner(text);
  const samples: PromSample[] = [];

  while (!scanner.isAtEnd) {
    skipLineWhitespace(scanner);

    // Blank line or comment line — skip.
    if (scanner.peek() === "\n" || scanner.peek() === "\r") {
      scanner.advance();
      continue;
    }
    if (scanner.peek() === "#") {
      skipToEndOfLine(scanner);
      continue;
    }
    if (scanner.isAtEnd) break;

    try {
      const sample = scanSample(scanner);
      if (sample) samples.push(sample);
    } catch (err) {
      if (err instanceof ScanError) {
        // Swallow parse errors on a per-line basis — metrics is a best-effort feed.
        skipToEndOfLine(scanner);
        continue;
      }
      throw err;
    }
    skipToEndOfLine(scanner);
  }

  return samples;
}

function skipLineWhitespace(scanner: Scanner): void {
  while (!scanner.isAtEnd) {
    const ch = scanner.peek();
    if (ch === " " || ch === "\t") scanner.advance();
    else break;
  }
}

function skipToEndOfLine(scanner: Scanner): void {
  while (!scanner.isAtEnd && scanner.peek() !== "\n") scanner.advance();
  if (!scanner.isAtEnd) scanner.advance();
}

function scanSample(scanner: Scanner): PromSample | null {
  if (!isPromNameStart(scanner.peek())) return null;
  const name = scanner.scanWhile(isPromNameChar);

  const labels: Record<string, string> = {};
  if (scanner.peek() === "{") {
    scanner.advance();
    scanLabels(scanner, labels);
    if (scanner.peek() !== "}") {
      throw scanner.error("Expected '}' at end of labels");
    }
    scanner.advance();
  }

  skipLineWhitespace(scanner);
  const value = scanNumberLiteral(scanner);
  return { name, labels, value };
}

function scanLabels(scanner: Scanner, out: Record<string, string>): void {
  skipLineWhitespace(scanner);
  if (scanner.peek() === "}") return;

  while (!scanner.isAtEnd) {
    skipLineWhitespace(scanner);
    if (!isIdentStart(scanner.peek())) {
      throw scanner.error("Expected label name");
    }
    const key = scanner.scanWhile(isIdentChar);
    skipLineWhitespace(scanner);
    scanner.expect("=", "Expected '=' after label name");
    skipLineWhitespace(scanner);
    if (scanner.peek() !== '"') {
      throw scanner.error("Label value must be quoted");
    }
    const value = scanner.scanString('"');
    out[key] = value;
    skipLineWhitespace(scanner);
    if (scanner.peek() === ",") {
      scanner.advance();
      continue;
    }
    break;
  }
}

/**
 * Scan a Prometheus number literal. The Scanner helper only handles finite
 * numbers, so we special-case the Prometheus tokens `Nan`/`+Inf`/`-Inf`
 * first and hand everything else to `Scanner.scanNumber`.
 */
function scanNumberLiteral(scanner: Scanner): number {
  const state = scanner.save();
  // Special tokens
  if (scanner.match("+Inf") || scanner.match("Inf")) return Number.POSITIVE_INFINITY;
  if (scanner.match("-Inf")) return Number.NEGATIVE_INFINITY;
  if (scanner.match("NaN") || scanner.match("Nan")) return Number.NaN;
  scanner.restore(state);

  // Allow leading sign before handing off to Scanner.scanNumber (which
  // already understands minus but not an explicit plus).
  if (scanner.peek() === "+") scanner.advance();
  if (!isDigit(scanner.peek()) && scanner.peek() !== "-" && scanner.peek() !== ".") {
    throw scanner.error("Expected metric value");
  }
  return scanner.scanNumber();
}

/** Find the first sample matching a name (and optional label filter). */
export function findSample(
  samples: PromSample[],
  name: string,
  filter?: Record<string, string>,
): PromSample | undefined {
  return samples.find((s) => {
    if (s.name !== name) return false;
    if (!filter) return true;
    for (const [k, v] of Object.entries(filter)) {
      if (s.labels[k] !== v) return false;
    }
    return true;
  });
}
