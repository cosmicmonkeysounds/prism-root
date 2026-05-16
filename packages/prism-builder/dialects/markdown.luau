--!strict
-- Bundled `markdown` sub-dialect (prui-luau-fusion.md §7.8 / Wave E.3).
-- Registered into every document's Luau frame by the host; a
-- `<markdown> … </markdown>` block (or `~md{…}` sigil) routes its
-- raw body through `parse`, which returns PRUI source the runtime
-- re-parses + lowers. Deliberately minimal — block-level only
-- (headings, bullets, paragraphs); inline emphasis is a follow-up.

prism.dialect {
  name = "markdown",
  parse = function(source)
    local kids = {}
    for line in (tostring(source) .. "\n"):gmatch("([^\n]*)\n") do
      local t = line:gsub("^%s+", ""):gsub("%s+$", "")
      if t == "" then
        -- blank line — paragraph break, nothing to emit
      elseif t:sub(1, 4) == "### " then
        kids[#kids + 1] = prui_ast.heading(t:sub(5), { level = 3 })
      elseif t:sub(1, 3) == "## " then
        kids[#kids + 1] = prui_ast.heading(t:sub(4), { level = 2 })
      elseif t:sub(1, 2) == "# " then
        kids[#kids + 1] = prui_ast.heading(t:sub(3), { level = 1 })
      elseif t:sub(1, 2) == "- " or t:sub(1, 2) == "* " then
        kids[#kids + 1] = prui_ast.text("\u{2022} " .. t:sub(3))
      else
        kids[#kids + 1] = prui_ast.text(t)
      end
    end
    return prui_ast.column { gap = 8, class = "markdown", children = kids }
  end,
}
