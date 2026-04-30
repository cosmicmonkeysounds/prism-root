use std::path::PathBuf;
use std::sync::Arc;

use prism_builder::app::PrismApp;
use prism_builder::project::ProjectFile;
use prism_builder::ComponentRegistry;
use prism_core::design_tokens::DesignTokens;

pub struct ProjectPersistence {
    current_path: Option<PathBuf>,
}

impl Default for ProjectPersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectPersistence {
    pub fn new() -> Self {
        Self { current_path: None }
    }

    pub fn current_path(&self) -> Option<&PathBuf> {
        self.current_path.as_ref()
    }

    pub fn has_path(&self) -> bool {
        self.current_path.is_some()
    }

    pub fn project_name(&self) -> Option<String> {
        self.current_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
    }

    pub fn save(
        &mut self,
        apps: &[PrismApp],
        registry: &Arc<ComponentRegistry>,
        tokens: &DesignTokens,
    ) -> Result<PathBuf, PersistenceError> {
        let path = self.current_path.clone().ok_or(PersistenceError::NoPath)?;
        write_project(&path, apps, registry, tokens)?;
        Ok(path)
    }

    pub fn save_as(
        &mut self,
        apps: &[PrismApp],
        registry: &Arc<ComponentRegistry>,
        tokens: &DesignTokens,
    ) -> Result<PathBuf, PersistenceError> {
        let path = pick_save_path()?;
        write_project(&path, apps, registry, tokens)?;
        self.current_path = Some(path.clone());
        Ok(path)
    }

    pub fn open(&mut self) -> Result<Vec<PrismApp>, PersistenceError> {
        let path = pick_open_path()?;
        let apps = read_project(&path)?;
        self.current_path = Some(path);
        Ok(apps)
    }

    pub fn clear_path(&mut self) {
        self.current_path = None;
    }
}

fn write_project(
    path: &PathBuf,
    apps: &[PrismApp],
    registry: &Arc<ComponentRegistry>,
    tokens: &DesignTokens,
) -> Result<(), PersistenceError> {
    let file = ProjectFile::from_apps(apps, registry, tokens);
    let json = file.to_json().map_err(PersistenceError::Serialize)?;
    std::fs::write(path, json).map_err(PersistenceError::Io)
}

fn read_project(path: &PathBuf) -> Result<Vec<PrismApp>, PersistenceError> {
    let content = std::fs::read_to_string(path).map_err(PersistenceError::Io)?;
    let file = ProjectFile::from_json(&content).map_err(PersistenceError::Deserialize)?;
    Ok(file.into_apps())
}

#[cfg(feature = "native")]
fn pick_save_path() -> Result<PathBuf, PersistenceError> {
    rfd::FileDialog::new()
        .add_filter("Prism Project", &[prism_builder::FILE_EXTENSION])
        .add_filter("All files", &["*"])
        .save_file()
        .ok_or(PersistenceError::Cancelled)
}

#[cfg(not(feature = "native"))]
fn pick_save_path() -> Result<PathBuf, PersistenceError> {
    Err(PersistenceError::NotSupported)
}

#[cfg(feature = "native")]
fn pick_open_path() -> Result<PathBuf, PersistenceError> {
    rfd::FileDialog::new()
        .add_filter("Prism Project", &[prism_builder::FILE_EXTENSION])
        .add_filter("All files", &["*"])
        .pick_file()
        .ok_or(PersistenceError::Cancelled)
}

#[cfg(not(feature = "native"))]
fn pick_open_path() -> Result<PathBuf, PersistenceError> {
    Err(PersistenceError::NotSupported)
}

#[derive(Debug)]
pub enum PersistenceError {
    NoPath,
    Cancelled,
    NotSupported,
    Io(std::io::Error),
    Serialize(serde_json::Error),
    Deserialize(serde_json::Error),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPath => write!(f, "No file path set — use Save As first"),
            Self::Cancelled => write!(f, "File dialog cancelled"),
            Self::NotSupported => write!(f, "File dialogs not available on this platform"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Serialize(e) => write!(f, "Serialization error: {e}"),
            Self::Deserialize(e) => write!(f, "Failed to parse project file: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::app::{AppIcon, NavigationConfig, Page};
    use prism_builder::document::{BuilderDocument, Node};
    use prism_builder::starter::register_builtins;
    use prism_builder::style::StyleProperties;
    use prism_core::design_tokens::DEFAULT_TOKENS;
    use serde_json::json;

    fn test_app() -> PrismApp {
        PrismApp {
            id: "test".into(),
            name: "Test App".into(),
            description: "desc".into(),
            icon: AppIcon::Cube,
            pages: vec![Page {
                id: "p1".into(),
                title: "Home".into(),
                route: "/".into(),
                source: String::new(),
                document: BuilderDocument {
                    root: Some(Node {
                        id: "root".into(),
                        component: "text".into(),
                        props: json!({ "body": "Hello" }),
                        children: vec![],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                style: StyleProperties::default(),
            }],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let reg = Arc::new(reg);
        let apps = vec![test_app()];

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("prism-persistence-test-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.prism");

        let mut persistence = ProjectPersistence::new();
        persistence.current_path = Some(path.clone());

        persistence.save(&apps, &reg, &DEFAULT_TOKENS).unwrap();
        assert!(path.exists());

        let mut persistence2 = ProjectPersistence::new();
        persistence2.current_path = Some(path.clone());
        let content = std::fs::read_to_string(&path).unwrap();
        let file = ProjectFile::from_json(&content).unwrap();
        let loaded = file.into_apps();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Test App");
        assert!(!loaded[0].pages[0].source.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_without_path_errors() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let reg = Arc::new(reg);
        let mut persistence = ProjectPersistence::new();
        let result = persistence.save(&[], &reg, &DEFAULT_TOKENS);
        assert!(matches!(result, Err(PersistenceError::NoPath)));
    }

    #[test]
    fn project_name_from_path() {
        let mut p = ProjectPersistence::new();
        assert!(p.project_name().is_none());
        p.current_path = Some(PathBuf::from("/tmp/my-app.prism"));
        assert_eq!(p.project_name().unwrap(), "my-app");
    }

    #[test]
    fn clear_path_resets() {
        let mut p = ProjectPersistence::new();
        p.current_path = Some(PathBuf::from("/tmp/test.prism"));
        assert!(p.has_path());
        p.clear_path();
        assert!(!p.has_path());
    }
}
