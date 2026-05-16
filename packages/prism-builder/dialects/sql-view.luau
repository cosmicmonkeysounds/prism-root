--!strict
-- Bundled `sql-view` sub-dialect (Wave E.3). Live query execution
-- is a data-layer concern (it needs `prism.objects` + a capability
-- grant); this bundled dialect surfaces the query as an inspectable
-- block so `~sql{select * from tasks}` is a valid first-class node
-- today. A query-executing backend swaps in behind the same
-- `prism.dialect` seam.

prism.dialect {
  name = "sql-view",
  parse = function(source)
    local q = tostring(source):gsub("^%s+", ""):gsub("%s+$", "")
    return prui_ast.column {
      class = "sql-view",
      gap = 4,
      children = {
        prui_ast.heading("query", { level = 4 }),
        prui_ast.text(q),
      },
    }
  end,
}
