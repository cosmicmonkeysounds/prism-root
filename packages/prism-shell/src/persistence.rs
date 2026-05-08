use std::path::PathBuf;
use std::sync::Arc;

use prism_builder::app::PrismApp;
use prism_builder::project::ProjectFile;
use prism_builder::ComponentRegistry;
use prism_core::design_tokens::DesignTokens;

pub struct ProjectPersistence {
    current_path: Option<PathBuf>,
    dirty: bool,
}

impl Default for ProjectPersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectPersistence {
    pub fn new() -> Self {
        Self {
            current_path: None,
            dirty: false,
        }
    }

    pub fn current_path(&self) -> Option<&PathBuf> {
        self.current_path.as_ref()
    }

    pub fn has_path(&self) -> bool {
        self.current_path.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
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
        self.dirty = false;
        Ok(path)
    }

    pub fn save_as(
        &mut self,
        apps: &[PrismApp],
        registry: &Arc<ComponentRegistry>,
        tokens: &DesignTokens,
    ) -> Result<PathBuf, PersistenceError> {
        let default_name = self
            .current_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.prism".into());
        let mut path = pick_save_path(&default_name)?;
        if path.extension().is_none() {
            path.set_extension(prism_builder::FILE_EXTENSION);
        }
        write_project(&path, apps, registry, tokens)?;
        self.current_path = Some(path.clone());
        self.dirty = false;
        Ok(path)
    }

    pub fn open(&mut self) -> Result<Vec<PrismApp>, PersistenceError> {
        let path = pick_open_path()?;
        let apps = read_project(&path)?;
        self.current_path = Some(path);
        self.dirty = false;
        Ok(apps)
    }

    pub fn open_path(&mut self, path: &std::path::Path) -> Result<Vec<PrismApp>, PersistenceError> {
        let apps = read_project(path)?;
        self.current_path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(apps)
    }

    pub fn clear_path(&mut self) {
        self.current_path = None;
        self.dirty = false;
    }
}

#[cfg(feature = "native")]
pub fn confirm_discard_changes() -> bool {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved Changes")
        .set_description("You have unsaved changes. Discard them?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

#[cfg(not(feature = "native"))]
pub fn confirm_discard_changes() -> bool {
    true
}

fn write_project(
    path: &std::path::Path,
    apps: &[PrismApp],
    registry: &Arc<ComponentRegistry>,
    tokens: &DesignTokens,
) -> Result<(), PersistenceError> {
    let file = ProjectFile::from_apps(apps, registry, tokens);
    let json = file.to_json().map_err(PersistenceError::Serialize)?;
    std::fs::write(path, json).map_err(PersistenceError::Io)
}

fn read_project(path: &std::path::Path) -> Result<Vec<PrismApp>, PersistenceError> {
    let content = std::fs::read_to_string(path).map_err(PersistenceError::Io)?;
    let file = ProjectFile::from_json(&content).map_err(PersistenceError::Deserialize)?;
    if file.version > prism_builder::FORMAT_VERSION {
        return Err(PersistenceError::FutureVersion {
            file_version: file.version,
            app_version: prism_builder::FORMAT_VERSION,
        });
    }
    Ok(file.into_apps())
}

#[cfg(feature = "native")]
fn pick_save_path(default_name: &str) -> Result<PathBuf, PersistenceError> {
    rfd::FileDialog::new()
        .set_file_name(default_name)
        .add_filter("Prism Project", &[prism_builder::FILE_EXTENSION])
        .add_filter("All files", &["*"])
        .save_file()
        .ok_or(PersistenceError::Cancelled)
}

#[cfg(not(feature = "native"))]
fn pick_save_path(_default_name: &str) -> Result<PathBuf, PersistenceError> {
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
    FutureVersion { file_version: u32, app_version: u32 },
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
            Self::FutureVersion {
                file_version,
                app_version,
            } => write!(
                f,
                "This project was created with a newer version of Prism (format v{file_version}, \
                 this app supports v{app_version}). Please upgrade Prism Studio."
            ),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Serialize(e) => write!(f, "Serialization error: {e}"),
            Self::Deserialize(e) => write!(f, "Failed to parse project file: {e}"),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serialize(e) | Self::Deserialize(e) => Some(e),
            _ => None,
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
        assert!(!p.is_dirty());
    }

    #[test]
    fn dirty_tracking() {
        let mut p = ProjectPersistence::new();
        assert!(!p.is_dirty());
        p.mark_dirty();
        assert!(p.is_dirty());
        p.clear_path();
        assert!(!p.is_dirty());
    }

    #[test]
    fn save_clears_dirty() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let reg = Arc::new(reg);

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("prism-dirty-test-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dirty.prism");

        let mut p = ProjectPersistence::new();
        p.current_path = Some(path);
        p.mark_dirty();
        assert!(p.is_dirty());

        p.save(&[], &reg, &DEFAULT_TOKENS).unwrap();
        assert!(!p.is_dirty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_path_loads_project() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let reg = Arc::new(reg);
        let apps = vec![test_app()];

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("prism-open-path-test-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.prism");

        let mut p1 = ProjectPersistence::new();
        p1.current_path = Some(path.clone());
        p1.save(&apps, &reg, &DEFAULT_TOKENS).unwrap();

        let mut p2 = ProjectPersistence::new();
        let loaded = p2.open_path(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Test App");
        assert_eq!(p2.project_name().unwrap(), "test");
        assert!(!p2.is_dirty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn future_version_rejected() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("prism-version-test-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future.prism");

        let json = r#"{"version":999,"apps":[]}"#;
        std::fs::write(&path, json).unwrap();

        let mut p = ProjectPersistence::new();
        let result = p.open_path(&path);
        assert!(matches!(
            result,
            Err(PersistenceError::FutureVersion { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }
}
