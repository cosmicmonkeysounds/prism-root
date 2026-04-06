/**
 * Layout panel — Puck visual builder connected to Loro CRDT.
 * Drag -> Loro, Loro -> Puck data prop.
 */

import { useMemo } from "react";
import { Puck, type Config } from "@measured/puck";
import { createPuckLoroBridge } from "@prism/core/layer2/puck/loro-puck-bridge";
import { usePuckLoro } from "@prism/core/layer2/puck/use-puck-loro";

/**
 * Minimal Puck component config.
 * Real apps will define rich component libraries.
 */
const puckConfig: Config = {
  components: {
    Heading: {
      fields: {
        text: { type: "text" },
        level: {
          type: "select",
          options: [
            { label: "H1", value: "h1" },
            { label: "H2", value: "h2" },
            { label: "H3", value: "h3" },
          ],
        },
      },
      defaultProps: {
        text: "Heading",
        level: "h1",
      },
      render: (props) => {
        const { text, level } = props as unknown as {
          text: string;
          level: string;
        };
        if (level === "h1") return <h1>{text}</h1>;
        if (level === "h2") return <h2>{text}</h2>;
        return <h3>{text}</h3>;
      },
    },
    Text: {
      fields: {
        content: { type: "textarea" },
      },
      defaultProps: {
        content: "Edit this text...",
      },
      render: (props) => {
        const { content } = props as unknown as { content: string };
        return <p>{content}</p>;
      },
    },
    Card: {
      fields: {
        title: { type: "text" },
        body: { type: "textarea" },
      },
      defaultProps: {
        title: "Card Title",
        body: "Card body content.",
      },
      render: (props) => {
        const { title, body } = props as unknown as {
          title: string;
          body: string;
        };
        return (
          <div
            style={{
              border: "1px solid #ddd",
              borderRadius: 8,
              padding: 16,
              margin: "8px 0",
            }}
          >
            <h3 style={{ margin: "0 0 8px" }}>{title}</h3>
            <p style={{ margin: 0, color: "#666" }}>{body}</p>
          </div>
        );
      },
    },
  },
};

export function LayoutPanel() {
  const bridge = useMemo(() => createPuckLoroBridge(), []);
  const { data, onChange } = usePuckLoro({ bridge });

  return (
    <div style={{ height: "100%" }}>
      <Puck config={puckConfig} data={data} onPublish={onChange} />
    </div>
  );
}
