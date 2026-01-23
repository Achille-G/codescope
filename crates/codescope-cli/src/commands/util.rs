use anyhow::Result;
use codescope_core::{FileEntry, Project, Walker, WalkerConfig};
use std::collections::HashSet;
use std::path::Path;

pub(crate) fn relative_path(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn build_walker_config(project: &Project) -> WalkerConfig {
    let config = project.config();
    WalkerConfig {
        max_file_size: config.indexing.max_file_size,
        follow_symlinks: config.indexing.follow_symlinks,
        exclude_patterns: config.indexing.ignore_patterns.clone(),
        ..Default::default()
    }
}

pub(crate) fn build_extension_set(extensions: &[String]) -> Option<HashSet<String>> {
    if extensions.is_empty() {
        return None;
    }

    let allowed: HashSet<String> = extensions
        .iter()
        .map(|ext| ext.trim_start_matches('.').to_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect();

    if allowed.is_empty() {
        None
    } else {
        Some(allowed)
    }
}

pub(crate) fn extension_allowed(path: &Path, allowed: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| allowed.contains(&ext.to_lowercase()))
        .unwrap_or(false)
}

pub(crate) fn filter_by_extensions(files: Vec<FileEntry>, extensions: &[String]) -> Vec<FileEntry> {
    let allowed = match build_extension_set(extensions) {
        Some(allowed) => allowed,
        None => return files,
    };

    files
        .into_iter()
        .filter(|entry| extension_allowed(&entry.path, &allowed))
        .collect()
}

pub(crate) fn collect_indexable_files(project: &Project) -> Result<Vec<FileEntry>> {
    let walker = Walker::with_config(project.root().to_path_buf(), build_walker_config(project));
    let mut files = walker.walk()?;
    files = filter_by_extensions(files, &project.config().indexing.include_extensions);
    files.retain(|entry| entry.has_supported_language());
    Ok(files)
}
