import type { SyntaxNode, RootNode } from '../syntax/ast-types.js';
import { SourceBuilder } from './source-builder.js';

/**
 * TextEmitter -- base class for serializing ASTs back to source text.
 *
 * Lighter than SymbolEmitter (which targets codegen). TextEmitter is for
 * round-tripping: parse source -> AST -> serialize back to source text.
 * Each format subclass overrides `emitNode()` to handle its node types.
 *
 * Pattern:
 *   class MarkdownTextEmitter extends TextEmitter {
 *     protected emitNode(node: SyntaxNode, b: SourceBuilder): void {
 *       switch (node.type) {
 *         case 'heading': b.line('#'.repeat(node.data?.level ?? 1) + ' ' + this.text(node)); break;
 *         case 'paragraph': b.line(this.text(node)); b.blank(); break;
 *         default: this.emitChildren(node, b);
 *       }
 *     }
 *   }
 *
 * Usage:
 *   const emitter = new MarkdownTextEmitter();
 *   const output = emitter.serialize(tree);
 */
export abstract class TextEmitter {
  /** Indent string used by SourceBuilder (default: 2 spaces). */
  protected indent = '  ';

  /** Serialize an AST back to source text. */
  serialize(tree: RootNode): string {
    const b = new SourceBuilder(this.indent);
    this.emitRoot(tree, b);
    return this.postProcess(b.build());
  }

  // -- Override points ------------------------------------------------------

  /** Emit the root node. Default: emit all children. */
  protected emitRoot(tree: RootNode, b: SourceBuilder): void {
    this.emitChildren(tree, b);
  }

  /** Emit a single node. Subclasses MUST override this. */
  protected abstract emitNode(node: SyntaxNode, b: SourceBuilder): void;

  /**
   * Post-process the final output string.
   * Default: trim trailing whitespace, ensure single trailing newline.
   */
  protected postProcess(output: string): string {
    return output.replace(/\s+$/, '') + '\n';
  }

  // -- Helpers --------------------------------------------------------------

  /** Emit all children of a node. */
  protected emitChildren(node: SyntaxNode, b: SourceBuilder): void {
    for (const child of node.children ?? []) {
      this.emitNode(child, b);
    }
  }

  /** Extract concatenated text values from a node's leaf descendants. */
  protected text(node: SyntaxNode): string {
    if (node.value !== undefined) return node.value;
    return (node.children ?? []).map(c => this.text(c)).join('');
  }

  /** Emit children as inline text (no line breaks between them). */
  protected inlineChildren(node: SyntaxNode): string {
    return (node.children ?? []).map(c => this.inlineNode(c)).join('');
  }

  /**
   * Render a node as inline text (single string, no line breaks).
   * Default: return value or recurse into children.
   * Override for nodes that need wrapping (bold -> `**text**`).
   */
  protected inlineNode(node: SyntaxNode): string {
    if (node.value !== undefined) return node.value;
    return this.inlineChildren(node);
  }
}
