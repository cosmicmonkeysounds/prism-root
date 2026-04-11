/**
 * Custom React Flow node types for the Prism spatial graph.
 *
 * - CodeMirrorNode: renders an embedded CodeMirror editor inside a graph node
 * - MarkdownNode: renders Markdown content with react-markdown
 * - DefaultPrismNode: simple labeled node for generic data
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
};

export type DefaultPrismNode = Node<DefaultNodeData, "default">;

function DefaultNodeComponent({ data }: NodeProps<DefaultPrismNode>) {
  return (
    <div className="prism-node prism-node-default">
      <Handle type="target" position={Position.Top} />
      <div className="prism-node-label">{data.label}</div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

export const DefaultNodeMemo = memo(DefaultNodeComponent);

// ─── Node type registry ─────────────────────────────────────

export const prismNodeTypes = {
  codemirror: CodeMirrorNodeMemo,
  markdown: MarkdownNodeMemo,
  default: DefaultNodeMemo,
} as const;
