# Widget System

Core engines declare widgets; the builder renders them. One shared type
vocabulary (`FieldSpec`) drives property panels, facet schemas, dashboard
config, and entity field definitions. No duplicate types, no per-widget
render code.

## Architecture

```
prism-core                              prism-builder
┌────────────────────────────┐         ┌──────────────────────────┐
│  widget::FieldSpec          │◄────────│  re-exports FieldSpec    │
│  widget::WidgetContribution │         │                          │
│  widget::WidgetTemplate     │────────►│  CoreWidgetComponent     │
│  widget::ToolbarAction      │         │  (wraps contribution,    │
│  widget::DataQuery          │         │   implements Component)  │
│                              │         │                          │
│  calendar::widget_contributions()     │  register_core_widgets() │
│  timekeeping::widget_contributions()  │  (auto-registers all     │
│  ledger::widget_contributions()       │   contributions into     │
│  spreadsheet::widget_contributions()  │   ComponentRegistry)     │
│  comments::widget_contributions()     │                          │
│  dashboard::widget_contributions()    │                          │
└────────────────────────────┘         └──────────────────────────┘
```

## Type Unification

Before this change, four separate types described "a named, typed,
configurable property":

| Type | Location | Fate |
|------|----------|------|
| `FieldSpec` + `FieldKind` | prism-builder/registry.rs | Moved to prism-core/widget/field.rs |
| `WidgetConfigEntry` + `WidgetConfigType` | prism-core/interaction/dashboard | Retired, uses `FieldSpec` |
| `SchemaField` + `SchemaFieldKind` | prism-builder/facet.rs | Retired, uses `FieldSpec` |
| `EntityFieldDef` | prism-core/foundation/object_model | Unchanged (graph schema, different purpose) |

`EntityFieldDef` stays separate — it describes graph object schemas with
lookup/rollup/expression semantics that don't apply to widget config or
UI fields. The other three collapse into one `FieldSpec`.

### FieldSpec

Moved to `prism-core::widget::field`:

```rust
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub default: Value,
    pub required: bool,
    pub help: Option<String>,
    pub group: Option<String>,
}

pub enum FieldKind {
    Text,
    TextArea,
    Number(NumericBounds),
    Integer(NumericBounds),
    Boolean,
    Select(Vec<SelectOption>),
    Color,
    File(FileFieldConfig),
    Date,
    DateTime,
    Duration,
    Currency { currency_code: Option<String> },
    Calculation { formula: String },
}
```

New variants (`Date`, `DateTime`, `Duration`, `Currency`, `Calculation`)
absorb what `SchemaFieldKind` and domain engines need. `Calculation`
replaces `SchemaFieldKind::Calculation` — evaluated via
`prism_core::language::expression::evaluate_expression`.

## WidgetContribution

The declaration a core engine makes. Pure data, no rendering code,
no builder dependency.

```rust
pub struct WidgetContribution {
    pub id: String,
    pub label: String,
    pub description: String,
    pub icon: Option<String>,
    pub category: WidgetCategory,

    // Property panel fields
    pub config_fields: Vec<FieldSpec>,
    pub default_config: Value,

    // Data shape the widget works with
    pub data_fields: Vec<FieldSpec>,

    // How to fetch data from the graph
    pub data_query: Option<DataQuery>,

    // Context-specific bottom toolbar actions
    pub toolbar_actions: Vec<ToolbarAction>,

    // Events this widget emits
    pub signals: Vec<SignalSpec>,

    // Style/size variant axes
    pub variants: Vec<VariantSpec>,

    // Layout sizing
    pub default_size: WidgetSize,
    pub min_size: Option<WidgetSize>,
    pub max_size: Option<WidgetSize>,

    // What to render
    pub template: WidgetTemplate,
}
```

### WidgetCategory

Groups widgets in the builder's insertion palette:

```rust
pub enum WidgetCategory {
    Display,        // Read-only data views (stats, charts, badges)
    Input,          // Interactive data entry (forms, editors)
    Navigation,     // Links, tabs, menus
    DataTable,      // Grid/table/list displays
    Temporal,       // Time-related (calendars, timers, schedules)
    Communication,  // Comments, chat, activity feeds
    Finance,        // Ledger, invoices, currency displays
    Layout,         // Structural containers
    Custom,         // User-defined
}
```

### DataQuery

Declares what data a widget needs. Resolved by the shell/builder
before rendering, fed into the template as bound data.

```rust
pub struct DataQuery {
    pub object_type: Option<String>,
    pub filters: Vec<QueryFilter>,
    pub sort: Vec<QuerySort>,
    pub limit: Option<usize>,
}

pub struct QueryFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: Value,
}

pub enum FilterOp {
    Eq, Neq, Gt, Gte, Lt, Lte, Contains, In,
}

pub struct QuerySort {
    pub field: String,
    pub descending: bool,
}
```

This maps onto the existing `interaction::query` pipeline for resolution.

### ToolbarAction

Actions shown in the modal bottom toolbar when a widget is selected
in the builder:

```rust
pub struct ToolbarAction {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub group: Option<String>,
    pub shortcut: Option<String>,
    pub kind: ToolbarActionKind,
}

pub enum ToolbarActionKind {
    Signal { signal: String },
    SetConfig { key: String, value: Value },
    ToggleConfig { key: String },
    Custom { action_type: String },
}
```

### SignalSpec and VariantSpec

Lightweight versions of builder's `SignalDef` and `VariantAxis` that
don't depend on prism-builder types:

```rust
pub struct SignalSpec {
    pub name: String,
    pub description: String,
    pub payload_fields: Vec<FieldSpec>,
}

pub struct VariantSpec {
    pub key: String,
    pub label: String,
    pub options: Vec<VariantOptionSpec>,
}

pub struct VariantOptionSpec {
    pub value: String,
    pub label: String,
    pub overrides: Value,
}
```

The builder maps these to `SignalDef` and `VariantAxis` at registration.

## WidgetTemplate

Declarative rendering — widgets describe *what* to render, not *how*.
The builder converts templates to Slint DSL through the existing
component pipeline.

```rust
pub struct WidgetTemplate {
    pub root: TemplateNode,
}

pub enum TemplateNode {
    Container {
        direction: LayoutDirection,
        gap: Option<u32>,
        padding: Option<u32>,
        children: Vec<TemplateNode>,
    },
    Component {
        component_id: String,
        props: Value,
    },
    DataBinding {
        field: String,
        component_id: String,
        prop_key: String,
    },
    Repeater {
        source: String,
        item_template: Box<TemplateNode>,
        empty_label: Option<String>,
    },
    Conditional {
        field: String,
        child: Box<TemplateNode>,
        fallback: Option<Box<TemplateNode>>,
    },
}

pub enum LayoutDirection {
    Horizontal,
    Vertical,
}
```

### Template → Slint rendering

`CoreWidgetComponent::render_slint` walks the `TemplateNode` tree:

- **Container** → emits `HorizontalLayout` or `VerticalLayout` with
  gap/padding, recurses into children.
- **Component** → delegates to `ComponentRegistry` by component_id,
  passing props.
- **DataBinding** → reads a field from the resolved data, sets it as
  a prop on the target component.
- **Repeater** → emits a `for` repeater in Slint, cloning the item
  template per data item.
- **Conditional** → emits an `if` expression in Slint, rendering child
  or fallback.

No per-widget Slint code needed. Complex widgets that need custom
rendering can still implement `Component` directly — this system is
for the common case.

## Engine Widget Examples

### Calendar

```rust
pub fn widget_contributions() -> Vec<WidgetContribution> {
    vec![
        WidgetContribution {
            id: "calendar-agenda".into(),
            label: "Agenda".into(),
            category: WidgetCategory::Temporal,
            data_query: Some(DataQuery {
                object_type: Some("event".into()),
                sort: vec![QuerySort { field: "date".into(), descending: false }],
                ..Default::default()
            }),
            template: WidgetTemplate {
                root: TemplateNode::Repeater {
                    source: "items".into(),
                    item_template: Box::new(TemplateNode::Container {
                        direction: LayoutDirection::Horizontal,
                        gap: Some(8),
                        padding: None,
                        children: vec![
                            TemplateNode::DataBinding {
                                field: "date".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            },
                            TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            },
                        ],
                    }),
                    empty_label: Some("No upcoming events".into()),
                },
            },
            ..Default::default()
        },
    ]
}
```

### Timekeeping

```rust
WidgetContribution {
    id: "timer-display".into(),
    label: "Timer".into(),
    category: WidgetCategory::Temporal,
    toolbar_actions: vec![
        ToolbarAction::signal("start", "Start", "play"),
        ToolbarAction::signal("pause", "Pause", "pause"),
        ToolbarAction::signal("stop", "Stop", "stop"),
        ToolbarAction::signal("reset", "Reset", "reset"),
    ],
    signals: vec![
        SignalSpec::new("start", "Timer started"),
        SignalSpec::new("pause", "Timer paused"),
        SignalSpec::new("stop", "Timer stopped"),
        SignalSpec::new("reset", "Timer reset"),
    ],
    ..Default::default()
}
```

## Registration Flow

1. **Boot**: `prism-builder::register_core_widgets(&mut registry)` calls
   each engine's `widget_contributions()`.
2. **Per contribution**: Wraps in `CoreWidgetComponent`, registers in
   `ComponentRegistry`.
3. **Document load**: Nodes with `component: "calendar-agenda"` etc.
   resolve through the registry like any other component.
4. **Render**: `CoreWidgetComponent::render_slint` walks the
   `WidgetTemplate`, delegating to existing components.
5. **Property panel**: Shell reads `Component::schema()` which returns
   the contribution's `config_fields`.
6. **Toolbar**: Shell reads `CoreWidgetComponent::toolbar_actions()`
   and renders the modal bottom toolbar.

## Widget Data Resolution

Widgets that need graph data declare `data_query` and `data_key` on
their `WidgetContribution`. The shell resolves these before rendering:

1. `resolve_widget_data(doc, collection)` walks the document tree.
2. For each node whose component matches a contribution with
   `data_query` + `data_key`, the shell queries `CollectionStore`
   using `DataQuery::apply()` (same filter/sort/limit pipeline
   facets use).
3. Results are stored in a `HashMap<NodeId, Value>` keyed by node ID,
   with the result array placed under the `data_key` prop name.
4. The `RenderSlintContext.widget_data` map is passed to the render
   walker, which merges resolved data into node props before template
   rendering.

This mirrors `resolve_facet_data` — both systems share `DataQuery` as
the single structured data-resolution primitive and resolve against the
same `CollectionStore`.

## Convergence with the Data Template System

The widget system and facets share `DataQuery` and converge on the same
data → template → expand → render child pipeline
(see `docs/dev/data-template-system.md`):

- **Engine widgets** use `WidgetTemplate` (declarative, code-authored)
  with `data_query` + `data_key` for automatic data resolution.
- **User facets** use `FacetTemplate` (inline or component-ref,
  GUI-authored) with `FacetKind` for data resolution.
- Both resolve through `DataQuery::apply()` against `CollectionStore`,
  then feed the result into their template expansion.

`DataQuery` is the single structured data-resolution primitive
shared by both systems. `FacetKind::ObjectQuery` and
`FacetDataSource::Query` both carry a `DataQuery`, using the same
`QueryFilter` matching (8 operators) and `QuerySort` ordering that
engine widgets use via `WidgetContribution.data_query`.

Inline templates (`FacetTemplate::Inline`) use `{{field}}` expression
binding directly in node props — the same concept as
`TemplateNode::DataBinding` but authored visually in the builder
canvas rather than declared in code.

## What Stays Unchanged

- `Component` trait — the Slint render contract.
- `PrefabDef` — internal storage for user-promoted components (no
  longer directly referenced by facets; see data-template-system.md).
- `ComponentRegistry` — just gets more registrations.
- Existing 16 built-in components — keep hand-written render.
- `EntityFieldDef` — graph object schema, different purpose.
