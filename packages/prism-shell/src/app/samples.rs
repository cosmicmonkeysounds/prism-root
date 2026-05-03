use prism_builder::app::{AppIcon, NavigationConfig, Page, PrismApp};
use prism_builder::layout::{
    FlexDirection, FlowDisplay, FlowProps, GridCell, LayoutMode, PageLayout, PageSize,
    SplitDirection, TrackSize,
};
use prism_builder::{BuilderDocument, Node, StyleProperties};
use prism_core::foundation::geometry::Edges;
use serde_json::json;

pub(super) fn sample_apps() -> Vec<PrismApp> {
    vec![
        PrismApp {
            id: "app-1".into(),
            name: "Lattice".into(),
            description: "Collaborative workspace with real-time CRDT sync.".into(),
            icon: AppIcon::Globe,
            pages: vec![
                Page {
                    id: "p1".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    source: String::new(),
                    document: sample_document(),
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p2".into(),
                    title: "Dashboard".into(),
                    route: "/dashboard".into(),
                    source: String::new(),
                    document: sample_dashboard_document(),
                    style: StyleProperties::default(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        },
        PrismApp {
            id: "app-2".into(),
            name: "Musica".into(),
            description: "Audio workstation with timeline and MIDI.".into(),
            icon: AppIcon::Music,
            pages: vec![Page {
                id: "p1".into(),
                title: "Studio".into(),
                route: "/".into(),
                source: String::new(),
                document: BuilderDocument::page_shell(),
                style: StyleProperties::default(),
            }],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        },
        PrismApp {
            id: "app-3".into(),
            name: "Flux".into(),
            description: "Visual dataflow editor for creative coding.".into(),
            icon: AppIcon::Zap,
            pages: vec![
                Page {
                    id: "p1".into(),
                    title: "Canvas".into(),
                    route: "/".into(),
                    source: String::new(),
                    document: BuilderDocument::page_shell(),
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p2".into(),
                    title: "Settings".into(),
                    route: "/settings".into(),
                    source: String::new(),
                    document: BuilderDocument::page_shell(),
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p3".into(),
                    title: "Preview".into(),
                    route: "/preview".into(),
                    source: String::new(),
                    document: BuilderDocument::page_shell(),
                    style: StyleProperties::default(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        },
    ]
}

fn sample_dashboard_document() -> BuilderDocument {
    BuilderDocument {
        root: Some(Node {
            id: "dash-root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            layout_mode: LayoutMode::Flow(FlowProps {
                display: FlowDisplay::Flex,
                flex_direction: FlexDirection::Column,
                gap: 16.0,
                ..Default::default()
            }),
            children: vec![
                Node {
                    id: "dash-title".into(),
                    component: "text".into(),
                    props: json!({ "body": "Dashboard", "level": "h2" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "dash-text".into(),
                    component: "text".into(),
                    props: json!({ "body": "Overview of your workspace metrics and activity." }),
                    children: vec![],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(super) fn sample_document() -> BuilderDocument {
    BuilderDocument {
        root: Some(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            layout_mode: LayoutMode::Flow(FlowProps {
                display: FlowDisplay::Flex,
                flex_direction: FlexDirection::Column,
                gap: 16.0,
                ..Default::default()
            }),
            children: vec![
                Node {
                    id: "hero".into(),
                    component: "text".into(),
                    props: json!({ "body": "Welcome to Prism", "level": "h1" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "intro".into(),
                    component: "text".into(),
                    props: json!({
                        "body": "The distributed visual operating system. Pick a panel to start editing."
                    }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "cols".into(),
                    component: "columns".into(),
                    props: json!({ "gap": 16 }),
                    layout_mode: LayoutMode::Flow(FlowProps {
                        display: FlowDisplay::Flex,
                        flex_direction: FlexDirection::Row,
                        gap: 16.0,
                        ..Default::default()
                    }),
                    children: vec![
                        Node {
                            id: "col1".into(),
                            component: "card".into(),
                            props: json!({ "title": "Build", "body": "Create pages visually with drag-and-drop components." }),
                            layout_mode: LayoutMode::Flow(FlowProps {
                                flex_grow: 1.0,
                                ..Default::default()
                            }),
                            children: vec![],
                            ..Default::default()
                        },
                        Node {
                            id: "col2".into(),
                            component: "card".into(),
                            props: json!({ "title": "Collaborate", "body": "Real-time CRDT sync across all connected peers." }),
                            layout_mode: LayoutMode::Flow(FlowProps {
                                flex_grow: 1.0,
                                ..Default::default()
                            }),
                            children: vec![],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        page_layout: PageLayout {
            size: PageSize::Responsive,
            margins: Edges::new(32.0, 48.0, 32.0, 48.0),
            grid: Some(GridCell::split(
                SplitDirection::Vertical,
                vec![
                    TrackSize::Fr { value: 1.0 },
                    TrackSize::Fr { value: 2.0 },
                    TrackSize::Fr { value: 1.0 },
                ],
                24.0,
                vec![GridCell::leaf(), GridCell::leaf(), GridCell::leaf()],
            )),
            column_gap: 24.0,
            ..Default::default()
        },
        ..Default::default()
    }
}
