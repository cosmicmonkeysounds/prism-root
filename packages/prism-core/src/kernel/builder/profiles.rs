//! `kernel::builder::profiles` — built-in `AppProfile`s for the
//! self-replicating Studio, plus canonical JSON encode/decode.
//!
//! Port of `kernel/builder/profiles.ts` at 8426588. Each of the four
//! ecosystem apps (Flux, Lattice, Cadence, Grip) has a pinned profile;
//! a universal Studio host profile and a headless Relay profile round
//! out the set.

use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;

use super::types::{
    AppProfile, AppThemeConfig, BuiltInProfileId, PageTemplateKind, StarterAppTemplate,
    StarterRouteTemplate, StarterShellChild, StarterShellSlot,
};

fn shell_data<const N: usize>(pairs: [(&str, JsonValue); N]) -> BTreeMap<String, JsonValue> {
    pairs.into_iter().map(|(k, v)| (k.into(), v)).collect()
}

fn shell_child(type_name: &str, slot: &str, name: &str, data: JsonValue) -> StarterShellChild {
    StarterShellChild {
        type_name: type_name.into(),
        slot: slot.into(),
        name: Some(name.into()),
        data: data.as_object().map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<_, _>>()
        }),
    }
}

fn route(
    path: &str,
    label: &str,
    template: PageTemplateKind,
    is_home: bool,
) -> StarterRouteTemplate {
    StarterRouteTemplate {
        path: path.into(),
        label: label.into(),
        page_template: template,
        is_home: if is_home { Some(true) } else { None },
        show_in_nav: Some(true),
    }
}

// ── Built-in profiles ───────────────────────────────────────────────────────

pub fn studio_profile() -> AppProfile {
    AppProfile {
        id: "studio".into(),
        name: "Prism Studio".into(),
        version: "0.1.0".into(),
        plugins: None,
        lenses: None,
        default_lens: Some("editor".into()),
        theme: Some(AppThemeConfig {
            display_name: Some("Prism Studio".into()),
            ..Default::default()
        }),
        kbar_commands: None,
        manifest: None,
        allow_glass_flip: Some(true),
        relay_modules: None,
        starter_app: Some(StarterAppTemplate {
            label: "Studio".into(),
            description: Some("Universal host — all lenses, minimal chrome.".into()),
            app_shell: StarterShellSlot {
                data: shell_data([
                    ("brand", json!("Prism Studio")),
                    ("brandIcon", json!("\u{25A6}")),
                    ("topBarHeight", json!(48)),
                    ("leftBarWidth", json!(220)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(true)),
                ]),
                children: Some(vec![
                    shell_child(
                        "heading",
                        "topBar",
                        "Brand",
                        json!({ "text": "Prism Studio", "level": "h3", "align": "left" }),
                    ),
                    shell_child(
                        "site-nav",
                        "leftBar",
                        "Site nav",
                        json!({ "orientation": "vertical", "source": "pages" }),
                    ),
                ]),
            },
            default_page_shell: StarterShellSlot {
                data: shell_data([
                    ("topBarHeight", json!(0)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(false)),
                ]),
                children: None,
            },
            routes: vec![
                route("/", "Home", PageTemplateKind::Landing, true),
                route("/docs", "Docs", PageTemplateKind::Blog, false),
            ],
        }),
    }
}

pub fn flux_profile() -> AppProfile {
    AppProfile {
        id: "flux".into(),
        name: "Flux".into(),
        version: "0.1.0".into(),
        plugins: Some(vec!["work".into(), "finance".into(), "crm".into()]),
        lenses: Some(vec![
            "editor".into(),
            "canvas".into(),
            "record-browser".into(),
            "automation".into(),
            "work".into(),
            "finance".into(),
            "crm".into(),
        ]),
        default_lens: Some("record-browser".into()),
        theme: Some(AppThemeConfig {
            primary: Some("#6C5CE7".into()),
            display_name: Some("Flux".into()),
            brand_icon: Some("flux.svg".into()),
            ..Default::default()
        }),
        kbar_commands: Some(vec![
            "new-task".into(),
            "new-invoice".into(),
            "new-contact".into(),
            "start-timer".into(),
        ]),
        manifest: None,
        allow_glass_flip: Some(true),
        relay_modules: None,
        starter_app: Some(StarterAppTemplate {
            label: "Flux".into(),
            description: Some(
                "Productivity — tasks, contacts, invoices in a record-browser chrome.".into(),
            ),
            app_shell: StarterShellSlot {
                data: shell_data([
                    ("brand", json!("Flux")),
                    ("brandIcon", json!("\u{26A1}")),
                    ("topBarHeight", json!(56)),
                    ("leftBarWidth", json!(280)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(true)),
                ]),
                children: Some(vec![
                    shell_child(
                        "heading",
                        "topBar",
                        "Brand",
                        json!({ "text": "Flux", "level": "h2", "align": "left" }),
                    ),
                    shell_child(
                        "site-nav",
                        "leftBar",
                        "Record nav",
                        json!({ "orientation": "vertical", "source": "pages", "showIcons": true }),
                    ),
                ]),
            },
            default_page_shell: StarterShellSlot {
                data: shell_data([
                    ("topBarHeight", json!(0)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(320)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(false)),
                ]),
                children: None,
            },
            routes: vec![
                route("/", "Inbox", PageTemplateKind::Blank, true),
                route("/tasks", "Tasks", PageTemplateKind::Blank, false),
                route("/contacts", "Contacts", PageTemplateKind::Blank, false),
                route("/invoices", "Invoices", PageTemplateKind::Blank, false),
            ],
        }),
    }
}

pub fn lattice_profile() -> AppProfile {
    AppProfile {
        id: "lattice".into(),
        name: "Lattice".into(),
        version: "0.1.0".into(),
        plugins: Some(vec!["assets".into(), "platform".into()]),
        lenses: Some(vec![
            "editor".into(),
            "graph".into(),
            "spatial-canvas".into(),
            "visual-script".into(),
            "luau-facet".into(),
            "assets-mgmt".into(),
        ]),
        default_lens: Some("graph".into()),
        theme: Some(AppThemeConfig {
            primary: Some("#00B894".into()),
            display_name: Some("Lattice".into()),
            brand_icon: Some("lattice.svg".into()),
            ..Default::default()
        }),
        kbar_commands: Some(vec![
            "new-dialogue".into(),
            "compile-bank".into(),
            "open-entity".into(),
        ]),
        manifest: None,
        allow_glass_flip: Some(true),
        relay_modules: None,
        starter_app: Some(StarterAppTemplate {
            label: "Lattice".into(),
            description: Some("Game middleware — graph-first editor with minimal chrome.".into()),
            app_shell: StarterShellSlot {
                data: shell_data([
                    ("brand", json!("Lattice")),
                    ("brandIcon", json!("\u{2B22}")),
                    ("topBarHeight", json!(40)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(300)),
                    ("bottomBarHeight", json!(120)),
                    ("stickyTopBar", json!(true)),
                ]),
                children: Some(vec![shell_child(
                    "heading",
                    "topBar",
                    "Brand",
                    json!({ "text": "Lattice", "level": "h3", "align": "left" }),
                )]),
            },
            default_page_shell: StarterShellSlot {
                data: shell_data([
                    ("topBarHeight", json!(0)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(false)),
                ]),
                children: None,
            },
            routes: vec![
                route("/", "Graph", PageTemplateKind::Blank, true),
                route("/assets", "Assets", PageTemplateKind::Blank, false),
                route("/scripts", "Scripts", PageTemplateKind::Blank, false),
            ],
        }),
    }
}

pub fn cadence_profile() -> AppProfile {
    AppProfile {
        id: "cadence".into(),
        name: "Cadence".into(),
        version: "0.1.0".into(),
        plugins: Some(vec!["life".into(), "platform".into()]),
        lenses: Some(vec![
            "editor".into(),
            "canvas".into(),
            "luau-facet".into(),
            "record-browser".into(),
            "life".into(),
        ]),
        default_lens: Some("canvas".into()),
        theme: Some(AppThemeConfig {
            primary: Some("#FD79A8".into()),
            display_name: Some("Cadence".into()),
            brand_icon: Some("cadence.svg".into()),
            ..Default::default()
        }),
        kbar_commands: Some(vec![
            "new-lesson".into(),
            "transcribe-session".into(),
            "open-course".into(),
        ]),
        manifest: None,
        allow_glass_flip: Some(true),
        relay_modules: None,
        starter_app: Some(StarterAppTemplate {
            label: "Cadence".into(),
            description: Some("Music education — lesson canvas with hero landing.".into()),
            app_shell: StarterShellSlot {
                data: shell_data([
                    ("brand", json!("Cadence")),
                    ("brandIcon", json!("\u{266B}")),
                    ("topBarHeight", json!(72)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(true)),
                ]),
                children: Some(vec![shell_child(
                    "heading",
                    "topBar",
                    "Brand",
                    json!({ "text": "Cadence", "level": "h1", "align": "center" }),
                )]),
            },
            default_page_shell: StarterShellSlot {
                data: shell_data([
                    ("topBarHeight", json!(0)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(false)),
                ]),
                children: None,
            },
            routes: vec![
                route("/", "Welcome", PageTemplateKind::Landing, true),
                route("/lessons", "Lessons", PageTemplateKind::Blank, false),
                route("/courses", "Courses", PageTemplateKind::Blank, false),
                route("/about", "About", PageTemplateKind::Blog, false),
            ],
        }),
    }
}

pub fn grip_profile() -> AppProfile {
    AppProfile {
        id: "grip".into(),
        name: "Grip".into(),
        version: "0.1.0".into(),
        plugins: Some(vec!["work".into(), "assets".into(), "platform".into()]),
        lenses: Some(vec![
            "editor".into(),
            "graph".into(),
            "spatial-canvas".into(),
            "canvas".into(),
            "automation".into(),
            "work".into(),
            "assets-mgmt".into(),
        ]),
        default_lens: Some("spatial-canvas".into()),
        theme: Some(AppThemeConfig {
            primary: Some("#E17055".into()),
            display_name: Some("Grip".into()),
            brand_icon: Some("grip.svg".into()),
            ..Default::default()
        }),
        kbar_commands: Some(vec![
            "new-cue".into(),
            "open-stage-plot".into(),
            "arm-transport".into(),
        ]),
        manifest: None,
        allow_glass_flip: Some(true),
        relay_modules: None,
        starter_app: Some(StarterAppTemplate {
            label: "Grip".into(),
            description: Some("Live production — cue list driven spatial canvas.".into()),
            app_shell: StarterShellSlot {
                data: shell_data([
                    ("brand", json!("Grip")),
                    ("brandIcon", json!("\u{2726}")),
                    ("topBarHeight", json!(48)),
                    ("leftBarWidth", json!(320)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(160)),
                    ("stickyTopBar", json!(true)),
                ]),
                children: Some(vec![shell_child(
                    "heading",
                    "topBar",
                    "Brand",
                    json!({ "text": "Grip", "level": "h2", "align": "left" }),
                )]),
            },
            default_page_shell: StarterShellSlot {
                data: shell_data([
                    ("topBarHeight", json!(0)),
                    ("leftBarWidth", json!(0)),
                    ("rightBarWidth", json!(0)),
                    ("bottomBarHeight", json!(0)),
                    ("stickyTopBar", json!(false)),
                ]),
                children: None,
            },
            routes: vec![
                route("/", "Stage", PageTemplateKind::Blank, true),
                route("/cues", "Cues", PageTemplateKind::Blank, false),
                route("/assets", "Assets", PageTemplateKind::Blank, false),
            ],
        }),
    }
}

pub fn relay_profile() -> AppProfile {
    AppProfile {
        id: "relay".into(),
        name: "Prism Relay".into(),
        version: "0.1.0".into(),
        plugins: Some(Vec::new()),
        lenses: Some(Vec::new()),
        default_lens: None,
        theme: Some(AppThemeConfig {
            display_name: Some("Prism Relay".into()),
            ..Default::default()
        }),
        kbar_commands: None,
        manifest: None,
        allow_glass_flip: Some(false),
        relay_modules: Some(vec![
            "blind-mailbox".into(),
            "relay-router".into(),
            "relay-timestamp".into(),
            "capability-tokens".into(),
            "sovereign-portals".into(),
            "webrtc-signaling".into(),
        ]),
        starter_app: None,
    }
}

pub fn list_builtin_profiles() -> Vec<AppProfile> {
    vec![
        studio_profile(),
        flux_profile(),
        lattice_profile(),
        cadence_profile(),
        grip_profile(),
        relay_profile(),
    ]
}

pub fn get_builtin_profile(id: BuiltInProfileId) -> AppProfile {
    match id {
        BuiltInProfileId::Studio => studio_profile(),
        BuiltInProfileId::Flux => flux_profile(),
        BuiltInProfileId::Lattice => lattice_profile(),
        BuiltInProfileId::Cadence => cadence_profile(),
        BuiltInProfileId::Grip => grip_profile(),
        BuiltInProfileId::Relay => relay_profile(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileParseError {
    #[error("app profile is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("app profile must be a JSON object")]
    NotObject,
    #[error("app profile requires id/name/version strings")]
    MissingFields,
}

pub fn serialize_app_profile(profile: &AppProfile) -> String {
    let mut out = serde_json::to_string_pretty(profile).expect("profile serialises cleanly");
    out.push('\n');
    out
}

pub fn parse_app_profile(source: &str) -> Result<AppProfile, ProfileParseError> {
    let value: JsonValue = serde_json::from_str(source)?;
    let obj = value.as_object().ok_or(ProfileParseError::NotObject)?;
    let has_id = obj.get("id").and_then(|v| v.as_str()).is_some();
    let has_name = obj.get("name").and_then(|v| v.as_str()).is_some();
    let has_version = obj.get("version").and_then(|v| v.as_str()).is_some();
    if !(has_id && has_name && has_version) {
        return Err(ProfileParseError::MissingFields);
    }
    let profile: AppProfile = serde_json::from_value(value)?;
    Ok(profile)
}
