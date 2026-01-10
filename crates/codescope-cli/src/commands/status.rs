//! `codescope status` command

use anyhow::{Context, Result};
use codescope_core::{
    FileEntry, FileParseConfig, FileParseOutcome, FileParser, FileReadConfig, FileReader,
    FileSkipReason, Project, Walker, WalkerConfig,
};
use std::collections::HashSet;
use std::env;

#[derive(Default)]
struct SkipCounts {
    too_large: u64,
    binary: u64,
    invalid_utf8: u64,
    unsupported: u64,
    failed: u64,
}

impl SkipCounts {
    fn add(&mut self, reason: FileSkipReason) {
        match reason {
            FileSkipReason::TooLarge => self.too_large += 1,
            FileSkipReason::Binary => self.binary += 1,
            FileSkipReason::InvalidUtf8 => self.invalid_utf8 += 1,
            FileSkipReason::UnsupportedLanguage => self.unsupported += 1,
        }
    }

    fn total(&self) -> u64 {
        self.too_large + self.binary + self.invalid_utf8 + self.unsupported + self.failed
    }
}

fn filter_by_extensions(files: Vec<FileEntry>, extensions: &[String]) -> Vec<FileEntry> {
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

pub fn run() -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    let config = project.config();

    println!("codescope project status");
    println!("========================");
    println!();
    println!("Root:    {}", project.root().display());
    println!("Profile: {}", config.profile);
    println!();
    println!("Index:");
    println!("  Database: {}", project.meta_db_path().display());
    println!("  HNSW:     {}", project.hnsw_index_path().display());
    println!("  Tantivy:  {}", project.tantivy_dir().display());
    println!();

    // TODO: Show actual stats from storage
    let walker_config = WalkerConfig {
        max_file_size: config.indexing.max_file_size,
        follow_symlinks: config.indexing.follow_symlinks,
        exclude_patterns: config.indexing.ignore_patterns.clone(),
        ..Default::default()
    };

    let walker = Walker::with_config(project.root().to_path_buf(), walker_config);
    let mut files = walker.walk().context("Failed to walk project files")?;
    files = filter_by_extensions(files, &config.indexing.include_extensions);
    files.retain(|entry| entry.has_supported_language());

    let total_files = files.len() as u64;
    let reader = FileReader::new(FileReadConfig::from_config(config));
    let parser = FileParser::with_default_parser(FileParseConfig::from_config(config));

    let mut parsed_files = 0u64;
    let mut chunk_count = 0u64;
    let mut skip_counts = SkipCounts::default();

    for outcome in parser.parse_stream(reader.read_files(files)).iter() {
        match outcome {
            FileParseOutcome::Parsed(parsed) => {
                parsed_files += 1;
                chunk_count += parsed.chunks.len() as u64;
            }
            FileParseOutcome::Skipped(skip) => {
                skip_counts.add(skip.reason);
            }
            FileParseOutcome::Failed(_) => {
                skip_counts.failed += 1;
            }
        }
    }

    println!("Statistics:");
    println!("  Files:    {}", total_files);
    println!("  Parsed:   {}", parsed_files);
    if skip_counts.total() == 0 {
        println!("  Skipped:  0");
    } else {
        println!(
            "  Skipped:  {} (large: {}, binary: {}, utf8: {}, unsupported: {}, failed: {})",
            skip_counts.total(),
            skip_counts.too_large,
            skip_counts.binary,
            skip_counts.invalid_utf8,
            skip_counts.unsupported,
            skip_counts.failed
        );
    }
    println!("  Chunks:   {}", chunk_count);
    println!("  Vectors:  {}", chunk_count);

    Ok(())
}
