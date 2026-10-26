//! Integration checks use generated images and isolated metadata paths only.
use anyhow::Result;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use wallpicker::{
    config::Config,
    model::{CategoryRef, Library, Source, scan},
    paths::Paths,
};

fn image(path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(path.parent().unwrap())?;
    image::RgbImage::from_pixel(3, 2, image::Rgb([64, 90, 120])).save(path)?;
    Ok(path.canonicalize()?)
}
fn source(path: &Path) -> Source {
    Source {
        path: path.to_path_buf(),
        recursive: true,
        auto_categories: true,
    }
}
fn fixture() -> Result<(TempDir, Library, PathBuf, PathBuf)> {
    let root = tempfile::tempdir()?;
    let a = image(&root.path().join("source/a.png"))?;
    let b = image(&root.path().join("source/b.png"))?;
    let mut library = Library::default();
    library.add_source(source(a.parent().unwrap()))?;
    Ok((root, library, a, b))
}

#[test]
fn categories_validate_names_and_keep_favorites_protected() -> Result<()> {
    let mut library = Library::default();
    for invalid in ["", "  ", "favorites", " FaVoRiTeS ", "ALL"] {
        assert!(library.add_category(invalid).is_err());
    }
    let first = library.add_category("  Dark  ")?;
    assert_eq!(library.categories[0].name, "Dark");
    assert!(library.add_category("dARK").is_err());
    let second = library.add_category("Warm")?;
    assert!(library.rename_category(second, "DARK").is_err());
    assert!(library.rename_category(first, "Favorites").is_err());
    assert!(library.rename_category(0, "Builtin").is_err());
    assert!(library.delete_category(0).is_err());
    library.rename_category(first, "DARK")?;
    assert_eq!(
        library.resolve_category("favorites")?,
        CategoryRef::Favorites
    );
    assert_eq!(
        library.resolve_category("dark")?,
        CategoryRef::Manual { id: first }
    );
    Ok(())
}

#[test]
fn many_to_many_memberships_are_deduplicated_and_metadata_only() -> Result<()> {
    let (_root, mut library, a, b) = fixture()?;
    let dark = library.add_category("Dark")?;
    let warm = library.add_category("Warm")?;
    library.add_images(dark, &[a.clone(), a.clone(), b.clone()])?;
    library.add_images(warm, std::slice::from_ref(&a))?;
    assert_eq!(library.categories[0].images.len(), 2);
    assert_eq!(library.categories[1].images, vec![a.clone()]);
    assert!(library.toggle_favorite(&a)?);
    assert!(!library.toggle_favorite(&a)?);
    library.remove_images(dark, std::slice::from_ref(&a))?;
    assert_eq!(library.categories[0].images, vec![b.clone()]);
    library.set_last_category(CategoryRef::Manual { id: warm });
    library.delete_category(warm)?;
    assert_eq!(library.last_category, CategoryRef::All);
    assert!(a.is_file() && b.is_file());
    Ok(())
}

#[test]
fn metadata_paths_are_canonical_and_outside_images_are_rejected() -> Result<()> {
    let (root, mut library, a, _) = fixture()?;
    let id = library.add_category("Examples")?;
    let alias = a.parent().unwrap().join("alias.png");
    symlink(&a, &alias)?;
    library.add_images(id, &[alias])?;
    assert_eq!(library.categories[0].images, vec![a]);
    let outside = image(&root.path().join("outside.png"))?;
    assert!(library.add_images(id, &[outside]).is_err());
    let document = root.path().join("source/notes.txt");
    fs::write(&document, "not a wallpaper")?;
    assert!(library.add_images(id, &[document]).is_err());
    Ok(())
}

#[test]
fn source_addition_rejects_duplicate_alias_and_nested_paths() -> Result<()> {
    let (root, mut library, a, _) = fixture()?;
    let base = a.parent().unwrap();
    assert!(library.add_source(source(base)).is_err());
    let nested = base.join("nested");
    fs::create_dir(&nested)?;
    assert!(library.add_source(source(&nested)).is_err());
    assert!(library.add_source(source(root.path())).is_err());
    let alias = root.path().join("alias");
    symlink(base, &alias)?;
    assert!(library.add_source(source(&alias)).is_err());
    assert!(library.add_source(source(&a)).is_err());
    Ok(())
}

#[test]
fn removing_source_prunes_only_its_memberships_and_keeps_files() -> Result<()> {
    let (root, mut library, a, b) = fixture()?;
    let other = image(&root.path().join("second/c.png"))?;
    library.add_source(source(other.parent().unwrap()))?;
    let id = library.add_category("Mixed")?;
    library.add_images(id, &[a.clone(), b.clone(), other.clone()])?;
    library.toggle_favorite(&a)?;
    library.toggle_favorite(&other)?;
    let report = library.remove_source(a.parent().unwrap())?;
    assert_eq!(report.removed, 3);
    assert_eq!(library.categories[0].images, vec![other.clone()]);
    assert_eq!(library.favorites, vec![other]);
    assert_eq!(scan(&library.sources).pool.len(), 1);
    assert!(a.is_file() && b.is_file());
    Ok(())
}

#[test]
fn missing_entries_are_hidden_until_explicit_cleanup() -> Result<()> {
    let (_root, mut library, a, b) = fixture()?;
    let id = library.add_category("Keep")?;
    library.add_images(id, &[a.clone(), b.clone()])?;
    library.toggle_favorite(&a)?;
    fs::remove_file(&a)?;
    let result = scan(&library.sources);
    assert_eq!(
        library.category_paths(&CategoryRef::Manual { id }, &result),
        vec![b]
    );
    assert_eq!(library.categories[0].images.len(), 2);
    assert!(
        library
            .category_paths(&CategoryRef::Favorites, &result)
            .is_empty()
    );
    assert_eq!(library.prune_missing().removed, 2);
    assert!(library.favorites.is_empty());
    assert_eq!(library.categories[0].images.len(), 1);
    Ok(())
}

#[test]
fn scan_merges_immediate_folder_names_and_hiding_preserves_all_pool() -> Result<()> {
    let root = tempfile::tempdir()?;
    let a = image(&root.path().join("one/Dark/a.png"))?;
    let b = image(&root.path().join("two/Dark/deeper/b.png"))?;
    image(&root.path().join("one/root.png"))?;
    fs::create_dir(root.path().join("one/Empty"))?;
    fs::write(root.path().join("one/Dark/ignore.txt"), "ignore")?;
    let mut library = Library::default();
    library.add_source(source(&root.path().join("one")))?;
    library.add_source(source(&root.path().join("two")))?;
    let result = scan(&library.sources);
    assert_eq!(result.pool.len(), 3);
    assert_eq!(result.auto_categories.len(), 1);
    assert!(result.auto_categories["Dark"].contains(&a));
    assert!(result.auto_categories["Dark"].contains(&b));
    assert!(!result.auto_categories.contains_key("deeper"));
    library.hidden_auto_categories.insert("Dark".into());
    assert!(
        library
            .category_paths(
                &CategoryRef::Auto {
                    name: "Dark".into()
                },
                &result
            )
            .is_empty()
    );
    assert_eq!(library.category_paths(&CategoryRef::All, &result).len(), 3);
    library.hidden_auto_categories.clear();
    assert_eq!(
        library
            .category_paths(
                &CategoryRef::Auto {
                    name: "Dark".into()
                },
                &result
            )
            .len(),
        2
    );
    Ok(())
}

#[test]
fn scan_nonrecursive_no_auto_and_missing_sources() -> Result<()> {
    let root = tempfile::tempdir()?;
    let direct = image(&root.path().join("source/direct.png"))?;
    image(&root.path().join("source/child/child.png"))?;
    let mut entry = source(&root.path().join("source"));
    entry.recursive = false;
    let result = scan(&[entry.clone()]);
    assert_eq!(result.pool.len(), 1);
    assert_eq!(result.pool[0].path, direct);
    assert!(result.auto_categories.is_empty());
    entry.recursive = true;
    entry.auto_categories = false;
    assert!(scan(&[entry]).auto_categories.is_empty());
    let missing = scan(&[source(&root.path().join("missing"))]);
    assert!(missing.pool.is_empty());
    assert_eq!(missing.skipped.len(), 1);
    Ok(())
}

#[test]
fn scanner_deduplicates_overlapping_inputs_and_does_not_follow_directory_loops() -> Result<()> {
    let (root, library, _, _) = fixture()?;
    let base = root.path().join("source");
    symlink(&base, base.join("loop"))?;
    let result = scan(&[library.sources[0].clone(), library.sources[0].clone()]);
    assert_eq!(result.pool.len(), 2);
    assert!(
        result
            .pool
            .iter()
            .all(|entry| entry.path.is_absolute() && entry.size > 0)
    );
    Ok(())
}

#[test]
fn store_round_trip_is_private_and_preserves_last_category() -> Result<()> {
    let (root, mut library, a, _) = fixture()?;
    let paths = Paths::under(root.path());
    let id = library.add_category("Dark")?;
    library.add_images(id, std::slice::from_ref(&a))?;
    library.toggle_favorite(&a)?;
    library.hidden_auto_categories.insert("Hidden".into());
    library.set_last_category(CategoryRef::Manual { id });
    library.save(&paths.library)?;
    let loaded = Library::load(&paths.library)?;
    assert_eq!(loaded.favorites, vec![a]);
    assert_eq!(loaded.categories[0].name, "Dark");
    assert_eq!(loaded.last_category, CategoryRef::Manual { id });
    assert!(loaded.hidden_auto_categories.contains("Hidden"));
    assert_eq!(
        fs::metadata(&paths.library)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_dir(paths.library.parent().unwrap())?.count(), 1);
    Ok(())
}

#[test]
fn corrupt_store_is_backed_up_and_original_is_not_overwritten() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("library.json");
    let original = b"{\"favorites\": [broken";
    fs::write(&path, original)?;
    assert!(Library::load(&path).is_err());
    assert_eq!(fs::read(&path)?, original);
    let backups: Vec<_> = fs::read_dir(root.path())?
        .filter_map(Result::ok)
        .filter(|e| e.path() != path)
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(backups[0].path())?, original);
    Ok(())
}

#[test]
fn future_store_versions_are_refused_without_rewriting() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("library.json");
    let original = b"{\"version\": 99}";
    fs::write(&path, original)?;
    assert!(Library::load(&path).is_err());
    assert_eq!(fs::read(path)?, original);
    Ok(())
}

#[test]
fn config_defaults_round_trip_and_invalid_values_fall_back_without_rewriting() -> Result<()> {
    let root = tempfile::tempdir()?;
    let paths = Paths::under(root.path());
    let default = Config::load(&paths.config)?;
    assert_eq!(default.thumbs.width, 400);
    assert_eq!(default.thumbs.height, 225);
    assert_eq!(default.thumbs.quality, 82);
    assert!(!paths.config.exists());
    default.save(&paths.config)?;
    assert_eq!(Config::load(&paths.config)?.theme.accent, "#69d4cd");
    for text in [
        "this is not TOML",
        "[thumbs]\nworkers = 0",
        "[window]\nwidth_percent = 101",
        "[behavior]\nduration = nan",
        "[theme]\naccent = 'orange'",
    ] {
        fs::write(&paths.config, text)?;
        assert_eq!(Config::load(&paths.config)?.thumbs.workers, 4);
        assert_eq!(fs::read_to_string(&paths.config)?, text);
    }
    Ok(())
}

#[test]
fn invalid_config_is_never_saved() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("config.toml");
    let mut config = Config::default();
    config.thumbs.workers = 0;
    assert!(config.save(&path).is_err());
    assert!(!path.exists());
    Ok(())
}

#[test]
fn malformed_stored_paths_and_duplicate_categories_are_backed_up() -> Result<()> {
    let (root, library, _, _) = fixture()?;
    let path = root.path().join("bad-library.json");
    for malformed in [
        serde_json::json!({"version":1,"favorites":["relative.png"]}),
        serde_json::json!({"version":1,"sources":library.sources,"categories":[{"id":1,"name":"Favorites","images":[]}]}),
        serde_json::json!({"version":1,"categories":[{"id":1,"name":"Dark","images":[]},{"id":1,"name":"Warm","images":[]}]}),
    ] {
        let bytes = serde_json::to_vec(&malformed)?;
        fs::write(&path, &bytes)?;
        assert!(Library::load(&path).is_err());
        assert_eq!(fs::read(&path)?, bytes);
    }
    Ok(())
}

#[test]
fn save_refuses_library_corrupted_after_initial_load() -> Result<()> {
    let (root, library, _, _) = fixture()?;
    let path = root.path().join("library.json");
    library.save(&path)?;
    fs::write(&path, b"corrupted while open")?;
    assert!(library.save(&path).is_err());
    assert_eq!(fs::read(&path)?, b"corrupted while open");
    Ok(())
}

#[test]
fn turning_recursion_off_preserves_existing_tags_for_later() -> Result<()> {
    let root = tempfile::tempdir()?;
    let nested = image(&root.path().join("source/sub/a.png"))?;
    let mut library = Library::default();
    library.add_source(source(&root.path().join("source")))?;
    let id = library.add_category("Keep")?;
    library.add_images(id, std::slice::from_ref(&nested))?;
    library.sources[0].recursive = false;
    let path = root.path().join("library.json");
    library.save(&path)?;
    let loaded = Library::load(&path)?;
    assert_eq!(loaded.categories[0].images, vec![nested]);
    assert!(scan(&loaded.sources).pool.is_empty());
    let report = library.remove_source(&root.path().join("source"))?;
    assert_eq!(report.removed, 1);
    assert!(library.categories[0].images.is_empty());
    library.save(&path)?;
    Ok(())
}
