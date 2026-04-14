/**
 * Custom React Flow node types for the Prism spatial graph.
 *
 * - CodeMirrorNode: renders an embedded CodeMirror editor inside a graph node
 * - MarkdownNode: renders Markdown content with react-markdown
 * - DefaultPrismNode: labeled node showing object name + type icon
 * - SitemapNode: route node for the sitemap lens (path pill, home glyph)
 *
 * Visual styling lives in the colocated `prism-graph.css` so the markup
 * stays presentation-free and host apps can re-theme via CSS variables.
 */

import React, { memo, useMemo } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

// ─── CodeMirror Node ────────────────────────────────────────

type CodeMirrorNodeData = {
  label: string;
  code: string;
  language?: string;
};

export type CodeMirrorNode = Node<CodeMirrorNodeData, "codemirror">;

function CodeMirrorNodeComponent({ data }: NodeProps<CodeMirrorNode>) {
  return (
    <div className="prism-node prism-node-codemirror">
      <div className="prism-node-header">{data.label}</div>
      <Handle type="target" position={Position.Top} />
      <pre className="prism-node-code">
        <code>{data.code}</code>
      </pre>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

export const CodeMirrorNodeMemo = memo(CodeMirrorNodeComponent);

// ─── Markdown Node ──────────────────────────────────────────

type MarkdownNodeData = {
  label: string;
  content: string;
};

export type MarkdownNode = Node<MarkdownNodeData, "markdown">;

function MarkdownNodeComponent({ data }: NodeProps<MarkdownNode>) {
  const plugins = useMemo(() => [remarkGfm], []);

  return (
    <div className="prism-node prism-node-markdown">
      <div className="prism-node-header">{data.label}</div>
      <Handle type="target" position={Position.Top} />
      <div className="prism-node-content">
        <ReactMarkdown remarkPlugins={plugins}>{data.content}</ReactMarkdown>
      </div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

export const MarkdownNodeMemo = memo(MarkdownNodeComponent);

// ─── Default Prism Node ─────────────────────────────────────

type DefaultNodeData = {
  label: string;
  /** Optional kernel object type, e.g. "page", "section". Renders as a sub-label. */
  objectType?: string;
  /** Optional emoji/glyph rendered next to the label. */
  icon?: string;
};

export type DefaultPrismNode = Node<DefaultNodeData, "default">;

function DefaultNodeComponent({ data }: NodeProps<DefaultPrismNode>) {
  return (
    <div className="prism-node prism-node-default">
      <Handle type="target" position={Position.Top} />
      <div className="prism-node-header">
        {data.icon ? <span className="prism-node-icon">{data.icon}</span> : null}
        <span className="prism-node-title">{data.label}</span>
      </div>
      {data.objectType ? (
        <div className="prism-node-content">{data.objectType}</div>
      ) : null}
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

export const DefaultNodeMemo = memo(DefaultNodeComponent);

// ─── Sitemap Node ───────────────────────────────────────────

type SitemapNodeData = {
  /** Display name. */
  label: string;
  /** URL path, e.g. `/`, `/tasks/:id`. */
  path: string;
  /** True when this route is the app's home route. */
  isHome?: boolean;
};

export type SitemapNodePrism = Node<SitemapNodeData, "sitemap">;

function SitemapNodeComponent({ data, selected }: NodeProps<SitemapNodePrism>) {
  const className = [
    "prism-node",
    "prism-node-sitemap",
    data.isHome ? "is-home" : "",
    selected ? "is-selected" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={className}>
      <Handle type="target" position={Position.Top} />
      <div className="prism-node-header">
        {data.isHome ? (
          <span className="prism-node-home-glyph" aria-label="Home route">
            ⌂
          </span>
        ) : (
          <span className="prism-node-icon">↪</span>
        )}
        <span className="prism-node-title">{data.label}</span>
      </div>
      <div className="prism-node-meta">
        <span className="prism-node-path">{data.path || "/"}</span>
      </div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

export const SitemapNodeMemo = memo(SitemapNodeComponent);

// ─── Node type registry ─────────────────────────────────────

export const prismNodeTypes = {
  codemirror: CodeMirrorNodeMemo,
  markdown: MarkdownNodeMemo,
  default: DefaultNodeMemo,
  sitemap: SitemapNodeMemo,
} as const;
