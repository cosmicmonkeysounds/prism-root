--!strict
-- Bundled `mermaid` sub-dialect (Wave E.3). Mermaid diagram
-- rendering is a renderer concern; this bundled dialect captures
-- author intent as a structured, inspectable node tree (a labelled
-- container holding the raw graph source) so `<mermaid>` is a valid
-- first-class block today. A real graph layout backend swaps in
-- behind the same `prism.dialect` seam without touching authors.

prism.dialect {
  name = "mermaid",
  parse = function(source)
    return prui_ast.column {
      class = "mermaid",
      gap = 4,
      children = {
        prui_ast.heading("diagram", { level = 4 }),
        prui_ast.text(tostring(source)),
      },
    }
  end,
}
