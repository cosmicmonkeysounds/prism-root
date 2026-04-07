export class SourceBuilder {
  private _lines: string[] = [];
  private _depth = 0;
  private _indent: string;

  constructor(indent = '  ') { this._indent = indent; }

  line(text = ''): this {
    this._lines.push(text ? this._indent.repeat(this._depth) + text : '');
    return this;
  }

  blank(): this { return this.line(); }

  indent(): this { this._depth++; return this; }
  dedent(): this { this._depth = Math.max(0, this._depth - 1); return this; }

  block(header: string, body: (b: this) => void, close = '}'): this {
    this.line(header + ' {');
    this.indent();
    body(this);
    this.dedent();
    this.line(close);
    return this;
  }

  comment(text: string): this { return this.line(`// ${text}`); }

  /** Emit a TypeScript `export const name = { ... } as const;` block */
  constBlock(name: string, body: (b: this) => void): this {
    this.line(`export const ${name} = {`);
    this.indent();
    body(this);
    this.dedent();
    this.line('} as const;');
    return this;
  }

  build(): string { return this._lines.join('\n'); }
}
