//! Test support for pax_data's integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A temporary copy of a scenario directory and the definitions directory, laid out
/// like the repository (so `data = "../../data"` still resolves). The copy is removed
/// when this is dropped, including when a test fails.
pub struct TempScenario {
    root: PathBuf,
    /// The copied scenario directory, to pass to `pax_data::load_scenario`.
    pub dir: PathBuf,
}

impl TempScenario {
    /// Copies `scenarios/<name>` and `data/` into a fresh directory named after `tag`.
    pub fn copy(name: &str, tag: &str) -> TempScenario {
        let repo = repo();
        let root = std::env::temp_dir().join(format!("pax-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(&repo.join("scenarios").join(name), &root.join("scenarios").join(name));
        copy_dir(&repo.join("data"), &root.join("data"));
        TempScenario { dir: root.join("scenarios").join(name), root }
    }

    /// A path inside the copy, relative to its root (e.g. `data/goods.toml`).
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for TempScenario {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The repository root.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// An empty temporary directory named after `tag`, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("pax-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
