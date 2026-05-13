-- Phase 7 canary: Luau-authored Tabs widget.
--
-- The Rust `tabs` block in `prism-builder/src/starter.rs` lowers to
-- a strip of pill containers above a panel containing the children.
-- This Luau version produces a structurally-equivalent tree using
-- the same `container` / `text` built-ins; the visual semantics
-- (`role="tablist"` / `role="tab"` / `aria-selected="true"`) are an
-- accessibility-props expansion tracked separately because they
-- need first-class container fields.
--
-- The render function takes `(props, data, ctx)`:
--   * `props.labels`   — comma-separated tab titles
--   * `ctx.children`   — the panel content children (deferred —
--                        today the walker passes its rendered
--                        children through `data`, which we fan out
--                        directly under the panel)

prism.widget {
    id = "tabs-luau",
    label = "Tabs (Luau)",
    description = "Tabbed content panels authored in Luau. Phase 7 canary.",
    category = "navigation",

    schema = {
        prism.field.text("labels", { label = "Tab labels (comma-separated)" }),
    },

    signals = {
        prism.signal("tab-changed", {
            { name = "index", kind = "number" },
            { name = "label", kind = "text" },
        }),
    },

    render = function(props, _data, _ctx)
        local labels = props.labels or ""
        local tabs = {}
        local i = 0
        for label in string.gmatch(labels, "[^,]+") do
            -- Trim whitespace per pill so `"A, B"` and `"A,B"` look the same.
            local trimmed = label:match("^%s*(.-)%s*$")
            local bg = (i == 0) and "#2e3440" or "#1a1e28"
            table.insert(tabs, {
                component = "container",
                props = { background = bg, padding = 12, height = 32 },
                children = {
                    { component = "text", props = { body = trimmed } },
                },
            })
            i = i + 1
        end

        -- Strip of tab pills above an empty panel slot. The panel
        -- carries no children today — wiring real child fan-out
        -- through `ctx.children` is the follow-up that turns this
        -- into a full Tabs replacement.
        local strip = {
            component = "container",
            props = { direction = "row" },
            children = tabs,
        }
        local panel = {
            component = "container",
            props = { padding = 12, gap = 8 },
            children = {},
        }
        return {
            component = "container",
            props = { direction = "column" },
            children = { strip, panel },
        }
    end,
}
