use std::rc::Rc;

use prism_builder::app::{AppIcon, NavigationConfig, Page, PrismApp};
use prism_builder::{BuilderDocument, StyleProperties};
use slint::ComponentHandle;

use super::super::{clear_href_on_node, execute_command, sync_ui_from_shared, Shell, ShellView};
use super::with_shell;

impl Shell {
    pub(super) fn wire_navigation_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

        // App navigation: open app from launchpad
        self.window.on_open_app({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |app_id| {
                let app_id = app_id.to_string();
                with_shell(&inner, &weak, |s| {
                    s.store.mutate(|state| {
                        state.shell_view = ShellView::App {
                            app_id: app_id.clone(),
                        };
                        state.workspace.switch_page_by_id("edit");
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                    s.dock_dirty.set(true);
                });
            }
        });

        // App navigation: go back to launchpad
        self.window.on_go_home({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        state.shell_view = ShellView::Launchpad;
                        state.selection.clear();
                    });
                    s.live = None;
                    s.dock_dirty.set(true);
                });
            }
        });

        // Explorer: node clicked — navigate to app/page
        self.window.on_explorer_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let nid = node_id.to_string();
                with_shell(&inner, &weak, |s| {
                    if let Some(app_id) = nid.strip_prefix("app:") {
                        s.save_to_active_page();
                        s.store.mutate(|state| {
                            state.shell_view = ShellView::App {
                                app_id: app_id.into(),
                            };
                            state.workspace.switch_page_by_id("edit");
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                        s.load_active_page();
                        s.dock_dirty.set(true);
                    } else if let Some(rest) = nid.strip_prefix("page:") {
                        let parts: Vec<&str> = rest.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            let aid = parts[0].to_string();
                            let pid = parts[1].to_string();
                            s.save_to_active_page();
                            s.store.mutate(|state| {
                                state.shell_view = ShellView::App { app_id: aid };
                                if let Some(app) = state.active_app_mut() {
                                    if let Some(idx) = app.pages.iter().position(|p| p.id == pid) {
                                        app.active_page = idx;
                                    }
                                }
                                state.selection.clear();
                                state.sync_document_from_app();
                            });
                            s.load_active_page();
                            s.dock_dirty.set(true);
                        }
                    }
                });
            }
        });

        // Explorer: toggle expand/collapse
        self.window.on_explorer_toggle_expand({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let nid = node_id.to_string();
                inner.borrow_mut().store.mutate(|state| {
                    if state.explorer_expanded.contains(&nid) {
                        state.explorer_expanded.remove(&nid);
                    } else {
                        state.explorer_expanded.insert(nid);
                    }
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // App navigation: create new app
        self.window.on_create_app({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    let id_num = s.store.state().next_app_id;
                    let naid = format!("app-{id_num}");
                    s.store.mutate(|state| {
                        state.next_app_id += 1;
                        state.apps.push(PrismApp {
                            id: naid.clone(),
                            name: format!("App {id_num}"),
                            description: "A new Prism app.".into(),
                            icon: AppIcon::Cube,
                            pages: vec![Page {
                                id: "page-1".into(),
                                title: "Home".into(),
                                route: "/".into(),
                                source: String::new(),
                                document: BuilderDocument::page_shell(),
                                style: StyleProperties::default(),
                            }],
                            active_page: 0,
                            navigation: NavigationConfig::default(),
                            style: StyleProperties::default(),
                        });
                        state.shell_view = ShellView::App { app_id: naid };
                        state.workspace.switch_page_by_id("edit");
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                    s.dock_dirty.set(true);
                    s.add_toast(
                        "App created",
                        &format!("App {id_num} is ready to edit"),
                        "success",
                    );
                });
            }
        });

        // Page navigation: switch page within an app
        self.window.on_switch_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Page navigation: add new page to active app
        self.window.on_add_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "add_page");
            }
        });

        // Breadcrumb navigation
        self.window.on_breadcrumb_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // ── Navigation panel callbacks ──────────────────────────────
        self.window.on_nav_add_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "add_page");
            }
        });
        self.window.on_nav_delete_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    let deleted = s.store.state().active_app().is_some_and(|app| {
                        app.pages.len() > 1 && (page_index as usize) < app.pages.len()
                    });
                    if deleted {
                        s.push_undo("Delete page");
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                crate::panels::navigation::NavigationPanel::delete_page(
                                    app,
                                    page_index as usize,
                                );
                            }
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                        s.load_active_page();
                    }
                });
            }
        });
        self.window.on_nav_rename_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index, new_title| {
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Rename page");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::rename_page(
                                app,
                                page_index as usize,
                                &new_title,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_set_route({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index, new_route| {
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Set page route");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::set_page_route(
                                app,
                                page_index as usize,
                                &new_route,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_move_page_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    s.push_undo("Move page up");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::move_page_up(
                                app,
                                page_index as usize,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_move_page_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    s.push_undo("Move page down");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::move_page_down(
                                app,
                                page_index as usize,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_select_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_cycle_style({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Change navigation style");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            use prism_builder::app::NavigationStyle;
                            let next = match app.navigation.style {
                                NavigationStyle::Tabs => NavigationStyle::Sidebar,
                                NavigationStyle::Sidebar => NavigationStyle::BottomBar,
                                NavigationStyle::BottomBar => NavigationStyle::None,
                                NavigationStyle::None => NavigationStyle::Tabs,
                            };
                            crate::panels::navigation::NavigationPanel::set_navigation_style(
                                app, next,
                            );
                        }
                    });
                });
            }
        });

        // Navigation graph: click node = select page
        self.window.on_nav_graph_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Navigation graph: begin link drag
        self.window.on_nav_graph_link_start({
            let inner = Rc::clone(&inner);
            move |source_idx| {
                inner.borrow_mut().nav_link_source = Some(source_idx as usize);
            }
        });

        // Navigation graph: complete link (add NavigateTo connection)
        self.window.on_nav_graph_link_end({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |target_idx| {
                let source_idx;
                {
                    let mut s = inner.borrow_mut();
                    source_idx = s.nav_link_source.take();
                }
                let Some(src) = source_idx else { return };
                let tgt = target_idx as usize;
                if src == tgt {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let target_route = s
                        .store
                        .state()
                        .active_app()
                        .and_then(|app| app.pages.get(tgt))
                        .map(|p| p.route.clone());
                    if let Some(route) = target_route {
                        use prism_builder::signal::{ActionKind, Connection};
                        let conn_id = format!("nav-{src}-{tgt}");
                        s.push_undo("Add navigation link");
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                if let Some(page) = app.pages.get_mut(src) {
                                    let already = page.document.connections.iter().any(|c| {
                                        matches!(&c.action, ActionKind::NavigateTo { target } if target == &route)
                                    });
                                    if !already {
                                        let source_node = page
                                            .document
                                            .root
                                            .as_ref()
                                            .map(|r| r.id.clone())
                                            .unwrap_or_default();
                                        page.document.connections.push(Connection {
                                            id: conn_id,
                                            source_node,
                                            signal: "clicked".into(),
                                            target_node: String::new(),
                                            action: ActionKind::NavigateTo {
                                                target: route,
                                            },
                                            params: serde_json::Value::Null,
                                        });
                                    }
                                }
                            }
                        });
                    }
                });
            }
        });

        // Navigation graph: remove a link edge
        self.window.on_nav_graph_link_remove({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |edge_id| {
                let edge_id = edge_id.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Remove navigation link");
                    if edge_id.starts_with("href:") {
                        // href:<page_idx>:<node_id> — clear the href prop
                        let parts: Vec<&str> = edge_id.splitn(3, ':').collect();
                        if parts.len() == 3 {
                            if let Ok(page_idx) = parts[1].parse::<usize>() {
                                let node_id = parts[2].to_string();
                                s.store.mutate(|state| {
                                    if let Some(app) = state.active_app_mut() {
                                        if let Some(page) = app.pages.get_mut(page_idx) {
                                            if let Some(root) = &mut page.document.root {
                                                clear_href_on_node(root, &node_id);
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    } else {
                        // Signal connection — remove by connection ID
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                for page in &mut app.pages {
                                    page.document.connections.retain(|c| c.id != edge_id);
                                }
                            }
                        });
                    }
                });
            }
        });

        // Schema designer: select a schema
        self.window.on_schema_select({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |schema_id| {
                let sid = schema_id.to_string();
                inner.borrow_mut().store.mutate(|state| {
                    state.selected_schema_id = if sid.is_empty() { None } else { Some(sid) };
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
    }
}
