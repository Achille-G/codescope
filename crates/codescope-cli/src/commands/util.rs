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

pub(crate) fn filter_by_extensions(files: Vec<FileEntry>, extensions: &[String]) -> Vec<FileEntry> {
    if extensions.is_empty() {
        return files;
    }

    let allowed: HashSet<String> = extensions
        .iter()
        .map(|ext| ext.trim_start_matches('.').to_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect();

    if allowed.is_empty() {
        return files;
    }

    files
        .into_iter()
        .filter(|entry| {
            entry
                .path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| allowed.contains(&ext.to_lowercase()))
                .unwrap_or(false)
        })
        .collect()
}

pub(crate) fn collect_indexable_files(project: &Project) -> Result<Vec<FileEntry>> {
    let config = project.config();
    let walker_config = WalkerConfig {
        max_file_size: config.indexing.max_file_size,
        follow_symlinks: config.indexing.follow_symlinks,
        exclude_patterns: config.indexing.ignore_patterns.clone(),
        ..Default::default()
    };

    let walker = Walker::with_config(project.root().to_path_buf(), walker_config);
    let mut files = walker.walk()?;
    files = filter_by_extensions(files, &config.indexing.include_extensions);
    files.retain(|entry| entry.has_supported_language());
    Ok(files)
}

