//! Metadata operations never move or delete wallpaper files.
use super::{scan, store};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
pub type CategoryId = u64;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CategoryRef {
    #[default]
    All,
    Favorites,
    Manual {
        id: CategoryId,
    },
    Auto {
        name: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub path: PathBuf,
    pub recursive: bool,
    pub auto_categories: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub images: Vec<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub sources: Vec<Source>,
    pub categories: Vec<Category>,
    pub favorites: Vec<PathBuf>,
    pub hidden_auto_categories: BTreeSet<String>,
    pub last_category: CategoryRef,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            sources: vec![],
            categories: vec![],
            favorites: vec![],
            hidden_auto_categories: BTreeSet::new(),
            last_category: CategoryRef::All,
        }
    }
}
#[derive(Debug, Default)]
pub struct PruneReport {
    pub removed: usize,
}
pub type RemovedMemberships = PruneReport;
impl Library {
    pub fn load(path: &Path) -> Result<Self> {
        let mut value: Self = store::load(path)?;
        if let Err(error) = value.normalize() {
            if path.exists() {
                let saved = store::backup(path)?;
                bail!(
                    "Invalid library: {error:#}. Original preserved; backup: {}",
                    saved.display()
                )
            }
            return Err(error);
        }
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut checked = self.clone();
        checked.normalize()?;
        // A file changed or corrupted while the picker was open is never blindly replaced.
        if path.exists() {
            Self::load(path)?;
        }
        store::save(path, &checked)
    }
    fn normalize(&mut self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "Unsupported library version {}; refusing to overwrite",
                self.version
            )
        }
        fn normalize_path(path: &mut PathBuf) -> Result<()> {
            if !path.is_absolute() {
                bail!("Stored paths must be absolute: {}", path.display())
            }
            if path.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            }) {
                bail!("Stored paths must be canonical: {}", path.display())
            }
            match path.canonicalize() {
                Ok(canonical) => *path = canonical,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            };
            Ok(())
        }
        for source in &mut self.sources {
            normalize_path(&mut source.path)?;
            if source.path.exists() && !source.path.is_dir() {
                bail!("Source must be a directory")
            }
        }
        for (index, source) in self.sources.iter().enumerate() {
            if self.sources[..index].iter().any(|other| {
                source.path.starts_with(&other.path) || other.path.starts_with(&source.path)
            }) {
                bail!("Duplicate or nested sources in library")
            }
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for category in &mut self.categories {
            category.name = category.name.trim().into();
            if category.id == 0
                || !ids.insert(category.id)
                || category.name.is_empty()
                || ["all", "favorites"]
                    .iter()
                    .any(|name| category.name.eq_ignore_ascii_case(name))
                || !names.insert(category.name.to_lowercase())
            {
                bail!("Invalid or duplicate manual category")
            }
        }
        for paths in std::iter::once(&mut self.favorites)
            .chain(self.categories.iter_mut().map(|c| &mut c.images))
        {
            for path in paths.iter_mut() {
                normalize_path(path)?;
                if !super::scan::supported(path)
                    || !self.sources.iter().any(|s| path.starts_with(&s.path))
                {
                    bail!("Stored image must belong to a source: {}", path.display())
                }
            }
            let mut seen = BTreeSet::new();
            paths.retain(|path| seen.insert(path.clone()));
        }
        if let CategoryRef::Manual { id } = self.last_category
            && !ids.contains(&id)
        {
            self.last_category = CategoryRef::All;
        }
        Ok(())
    }
    fn valid_name(&self, name: &str, except: Option<CategoryId>) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Category name cannot be empty")
        };
        if ["all", "favorites"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n))
            || self
                .categories
                .iter()
                .any(|c| Some(c.id) != except && c.name.to_lowercase() == name.to_lowercase())
        {
            bail!("Category name is reserved or already exists")
        };
        Ok(name.into())
    }
    pub fn add_category(&mut self, name: &str) -> Result<CategoryId> {
        let name = self.valid_name(name, None)?;
        let id = self
            .categories
            .iter()
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .context("Category ID exhausted")?;
        self.categories.push(Category {
            id,
            name,
            images: vec![],
        });
        Ok(id)
    }
    pub fn rename_category(&mut self, id: CategoryId, name: &str) -> Result<()> {
        let name = self.valid_name(name, Some(id))?;
        self.category_mut(id)?.name = name;
        Ok(())
    }
    pub fn delete_category(&mut self, id: CategoryId) -> Result<()> {
        self.category_mut(id)?;
        self.categories.retain(|c| c.id != id);
        if self.last_category == (CategoryRef::Manual { id }) {
            self.last_category = CategoryRef::All
        };
        Ok(())
    }
    fn category_mut(&mut self, id: CategoryId) -> Result<&mut Category> {
        self.categories
            .iter_mut()
            .find(|c| c.id == id)
            .context("Manual category not found; built-in categories cannot be changed")
    }
    fn checked_images(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
        paths
            .iter()
            .map(|p| {
                let p = p.canonicalize()?;
                if !p.is_file()
                    || !super::scan::supported(&p)
                    || !self.sources.iter().any(|s| super::scan::belongs(&p, s))
                {
                    bail!("Image must belong to a source: {}", p.display())
                };
                Ok(p)
            })
            .collect()
    }
    pub fn add_images(&mut self, id: CategoryId, paths: &[PathBuf]) -> Result<()> {
        let paths = self.checked_images(paths)?;
        let c = self.category_mut(id)?;
        for p in paths {
            if !c.images.contains(&p) {
                c.images.push(p)
            }
        }
        Ok(())
    }
    pub fn remove_images(&mut self, id: CategoryId, paths: &[PathBuf]) -> Result<()> {
        self.category_mut(id)?.images.retain(|p| !paths.contains(p));
        Ok(())
    }
    pub fn toggle_favorite(&mut self, path: &Path) -> Result<bool> {
        let paths = self.checked_images(&[path.into()])?;
        let p = &paths[0];
        if self.favorites.contains(p) {
            self.favorites.retain(|v| v != p);
            Ok(false)
        } else {
            self.favorites.push(p.clone());
            Ok(true)
        }
    }
    pub fn set_last_category(&mut self, category: CategoryRef) {
        self.last_category = category;
    }
    pub fn add_source(&mut self, mut source: Source) -> Result<()> {
        source.path = source.path.canonicalize()?;
        if !source.path.is_dir() {
            bail!("Source must be a directory")
        };
        if self
            .sources
            .iter()
            .any(|s| s.path.starts_with(&source.path) || source.path.starts_with(&s.path))
        {
            bail!("Duplicate or nested source")
        };
        self.sources.push(source);
        Ok(())
    }
    pub fn remove_source(&mut self, path: &Path) -> Result<RemovedMemberships> {
        let p = path.canonicalize().unwrap_or_else(|_| path.into());
        let index = self
            .sources
            .iter()
            .position(|s| s.path == p)
            .context("Source not found")?;
        let removed = self.sources.remove(index);
        Ok(self.prune_where(|p| !p.starts_with(&removed.path)))
    }
    fn prune_where(&mut self, keep: impl Fn(&Path) -> bool) -> PruneReport {
        let mut removed = 0;
        for paths in std::iter::once(&mut self.favorites)
            .chain(self.categories.iter_mut().map(|c| &mut c.images))
        {
            let before = paths.len();
            paths.retain(|p| keep(p));
            removed += before - paths.len()
        }
        PruneReport { removed }
    }
    pub fn prune_missing(&mut self) -> PruneReport {
        self.prune_where(|p| p.is_file())
    }
    pub fn resolve_category(&self, name: &str) -> Result<CategoryRef> {
        if name.eq_ignore_ascii_case("all") {
            return Ok(CategoryRef::All);
        }
        if name.eq_ignore_ascii_case("favorites") {
            return Ok(CategoryRef::Favorites);
        }
        if let Some(c) = self
            .categories
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
        {
            return Ok(CategoryRef::Manual { id: c.id });
        }
        if scan(&self.sources).auto_categories.contains_key(name) {
            return Ok(CategoryRef::Auto { name: name.into() });
        }
        bail!("Unknown category: {name}")
    }
    pub fn category_paths(
        &self,
        category: &CategoryRef,
        result: &super::scan::ScanResult,
    ) -> Vec<PathBuf> {
        let paths = match category {
            CategoryRef::All => result.pool.iter().map(|e| e.path.clone()).collect(),
            CategoryRef::Favorites => self.favorites.clone(),
            CategoryRef::Manual { id } => self
                .categories
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.images.clone())
                .unwrap_or_default(),
            CategoryRef::Auto { name } => {
                if self.hidden_auto_categories.contains(name) {
                    vec![]
                } else {
                    result
                        .auto_categories
                        .get(name)
                        .cloned()
                        .unwrap_or_default()
                }
            }
        };
        paths.into_iter().filter(|p| p.is_file()).collect()
    }
}
