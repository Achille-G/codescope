//! Concurrent file reading and parsing pipeline

use crate::{Config, FileEntry, Profile};
use codescope_parser::{Chunk, Language, Parser};
use crossbeam_channel::{bounded, Receiver};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tracing::{debug, trace};

const DEFAULT_MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Configuration for concurrent file reading
#[derive(Debug, Clone)]
pub struct FileReadConfig {
    /// Maximum file size in bytes
    pub max_file_size: u64,
    /// Bounded channel size for backpressure
    pub buffer_size: usize,
    /// Number of reader threads
    pub num_threads: usize,
    /// Whether to allow lossy UTF-8 conversion
    pub lossy_utf8: bool,
}

impl FileReadConfig {
    /// Build a reader config from a profile and max file size.
    pub fn from_profile(profile: Profile, max_file_size: u64) -> Self {
        Self {
            max_file_size,
            buffer_size: profile.walker_buffer_size(),
            num_threads: profile.thread_count(),
            lossy_utf8: true,
        }
    }

    /// Build a reader config from workspace config.
    pub fn from_config(config: &Config) -> Self {
        Self::from_profile(config.profile, config.indexing.max_file_size)
    }
}

impl Default for FileReadConfig {
    fn default() -> Self {
        Self::from_profile(Profile::Default, DEFAULT_MAX_FILE_SIZE)
    }
}

/// Configuration for parsing file contents
#[derive(Debug, Clone)]
pub struct FileParseConfig {
    /// Bounded channel size for backpressure
    pub buffer_size: usize,
    /// Number of parser threads
    pub num_threads: usize,
}

impl FileParseConfig {
    /// Build a parser config from a profile.
    pub fn from_profile(profile: Profile) -> Self {
        Self {
            buffer_size: profile.walker_buffer_size(),
            num_threads: profile.thread_count(),
        }
    }

    /// Build a parser config from workspace config.
    pub fn from_config(config: &Config) -> Self {
        Self::from_profile(config.profile)
    }
}

impl Default for FileParseConfig {
    fn default() -> Self {
        Self::from_profile(Profile::Default)
    }
}

/// A UTF-8 decoded file payload
#[derive(Debug, Clone)]
pub struct FileContent {
    pub path: PathBuf,
    pub language: Option<Language>,
    pub size: u64,
    pub content: String,
    pub was_lossy: bool,
}

/// Parsed chunks for a file
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub path: PathBuf,
    pub language: Language,
    pub size: u64,
    pub was_lossy: bool,
    pub chunks: Vec<Chunk>,
}

/// Reasons a file can be skipped
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSkipReason {
    TooLarge,
    Binary,
    InvalidUtf8,
    UnsupportedLanguage,
}

/// Skip metadata
#[derive(Debug, Clone)]
pub struct FileSkip {
    pub path: PathBuf,
    pub reason: FileSkipReason,
}

/// File read error metadata
#[derive(Debug, Clone)]
pub struct FileReadError {
    pub path: PathBuf,
    pub message: String,
}

/// File parse error metadata
#[derive(Debug, Clone)]
pub struct FileParseError {
    pub path: PathBuf,
    pub message: String,
}

/// Outcome from reading a file
#[derive(Debug, Clone)]
pub enum FileReadOutcome {
    Read(FileContent),
    Skipped(FileSkip),
    Failed(FileReadError),
}

/// Outcome from parsing a file
#[derive(Debug, Clone)]
pub enum FileParseOutcome {
    Parsed(ParsedFile),
    Skipped(FileSkip),
    Failed(FileParseError),
}

/// Concurrent reader for file contents
pub struct FileReader {
    config: FileReadConfig,
}

impl FileReader {
    pub fn new(config: FileReadConfig) -> Self {
        Self { config }
    }

    /// Read files concurrently and return a bounded stream of results.
    pub fn read_files(&self, files: Vec<FileEntry>) -> Receiver<FileReadOutcome> {
        let buffer_size = self.config.buffer_size.max(1);
        let num_threads = self.config.num_threads.max(1);

        let (job_tx, job_rx) = bounded::<FileEntry>(buffer_size);
        let (result_tx, result_rx) = bounded::<FileReadOutcome>(buffer_size);

        for _ in 0..num_threads {
            let job_rx = job_rx.clone();
            let result_tx = result_tx.clone();
            let config = self.config.clone();
            thread::spawn(move || {
                for entry in job_rx.iter() {
                    let outcome = read_entry(&entry, &config);
                    if result_tx.send(outcome).is_err() {
                        break;
                    }
                }
            });
        }

        drop(result_tx);

        thread::spawn(move || {
            for entry in files {
                if job_tx.send(entry).is_err() {
                    return;
                }
            }
        });

        result_rx
    }
}

/// Concurrent parser for file content streams
pub struct FileParser {
    config: FileParseConfig,
    parser: Arc<Parser>,
}

impl FileParser {
    pub fn new(parser: Arc<Parser>, config: FileParseConfig) -> Self {
        Self { config, parser }
    }

    /// Build a parser with a default tree-sitter pool.
    pub fn with_default_parser(config: FileParseConfig) -> Self {
        Self::new(Arc::new(Parser::new()), config)
    }

    /// Parse a stream of file reads and return parsed outcomes.
    pub fn parse_stream(&self, input: Receiver<FileReadOutcome>) -> Receiver<FileParseOutcome> {
        let buffer_size = self.config.buffer_size.max(1);
        let num_threads = self.config.num_threads.max(1);

        let (result_tx, result_rx) = bounded::<FileParseOutcome>(buffer_size);

        for _ in 0..num_threads {
            let input_rx = input.clone();
            let result_tx = result_tx.clone();
            let parser = Arc::clone(&self.parser);
            thread::spawn(move || {
                for outcome in input_rx.iter() {
                    let parsed = match outcome {
                        FileReadOutcome::Read(content) => parse_entry(&parser, content),
                        FileReadOutcome::Skipped(skip) => FileParseOutcome::Skipped(skip),
                        FileReadOutcome::Failed(err) => FileParseOutcome::Failed(FileParseError {
                            path: err.path,
                            message: err.message,
                        }),
                    };

                    if result_tx.send(parsed).is_err() {
                        break;
                    }
                }
            });
        }

        drop(result_tx);
        result_rx
    }
}

fn read_entry(entry: &FileEntry, config: &FileReadConfig) -> FileReadOutcome {
    if entry.size > config.max_file_size {
        trace!(
            "Skipping large file {:?} ({} bytes)",
            entry.path,
            entry.size
        );
        return FileReadOutcome::Skipped(FileSkip {
            path: entry.path.clone(),
            reason: FileSkipReason::TooLarge,
        });
    }

    let bytes = match std::fs::read(&entry.path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return FileReadOutcome::Failed(FileReadError {
                path: entry.path.clone(),
                message: err.to_string(),
            })
        }
    };

    if bytes.len() as u64 > config.max_file_size {
        trace!(
            "Skipping large file {:?} ({} bytes)",
            entry.path,
            bytes.len()
        );
        return FileReadOutcome::Skipped(FileSkip {
            path: entry.path.clone(),
            reason: FileSkipReason::TooLarge,
        });
    }

    if bytes.iter().any(|b| *b == 0) {
        debug!("Skipping binary file {:?}", entry.path);
        return FileReadOutcome::Skipped(FileSkip {
            path: entry.path.clone(),
            reason: FileSkipReason::Binary,
        });
    }

    match String::from_utf8(bytes) {
        Ok(content) => FileReadOutcome::Read(FileContent {
            path: entry.path.clone(),
            language: entry.language,
            size: entry.size,
            content,
            was_lossy: false,
        }),
        Err(err) => {
            if !config.lossy_utf8 {
                return FileReadOutcome::Skipped(FileSkip {
                    path: entry.path.clone(),
                    reason: FileSkipReason::InvalidUtf8,
                });
            }

            let bytes = err.into_bytes();
            let content = String::from_utf8_lossy(&bytes).into_owned();
            FileReadOutcome::Read(FileContent {
                path: entry.path.clone(),
                language: entry.language,
                size: entry.size,
                content,
                was_lossy: true,
            })
        }
    }
}

fn parse_entry(parser: &Parser, content: FileContent) -> FileParseOutcome {
    let language = match content.language {
        Some(language) => language,
        None => {
            return FileParseOutcome::Skipped(FileSkip {
                path: content.path,
                reason: FileSkipReason::UnsupportedLanguage,
            })
        }
    };

    match parser.parse(&content.content, language) {
        Ok(chunks) => FileParseOutcome::Parsed(ParsedFile {
            path: content.path,
            language,
            size: content.size,
            was_lossy: content.was_lossy,
            chunks,
        }),
        Err(err) => FileParseOutcome::Failed(FileParseError {
            path: content.path,
            message: err.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_reader_lossy_utf8() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.rs");
        fs::write(&path, [0xF0, 0x28, 0x8C, 0x28]).unwrap();

        let entry = FileEntry::new(path, 4);
        let config = FileReadConfig {
            num_threads: 1,
            buffer_size: 2,
            ..Default::default()
        };
        let reader = FileReader::new(config);
        let outcomes: Vec<_> = reader.read_files(vec![entry]).iter().collect();

        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            FileReadOutcome::Read(content) => assert!(content.was_lossy),
            _ => panic!("expected lossy read"),
        }
    }

    #[test]
    fn test_reader_skips_binary() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bin.rs");
        fs::write(&path, [0x00, 0x01, 0x02]).unwrap();

        let entry = FileEntry::new(path.clone(), 3);
        let config = FileReadConfig {
            num_threads: 1,
            buffer_size: 2,
            ..Default::default()
        };
        let reader = FileReader::new(config);
        let outcomes: Vec<_> = reader.read_files(vec![entry]).iter().collect();

        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            FileReadOutcome::Skipped(skip) => {
                assert_eq!(skip.reason, FileSkipReason::Binary);
                assert_eq!(skip.path, path);
            }
            _ => panic!("expected binary skip"),
        }
    }

    #[test]
    fn test_reader_skips_large_files() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("large.rs");
        fs::write(&path, "0123456789ABCDEF").unwrap();

        let entry = FileEntry::new(path.clone(), 16);
        let config = FileReadConfig {
            max_file_size: 8,
            num_threads: 1,
            buffer_size: 2,
            ..Default::default()
        };
        let reader = FileReader::new(config);
        let outcomes: Vec<_> = reader.read_files(vec![entry]).iter().collect();

        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            FileReadOutcome::Skipped(skip) => {
                assert_eq!(skip.reason, FileSkipReason::TooLarge);
                assert_eq!(skip.path, path);
            }
            _ => panic!("expected large file skip"),
        }
    }

    #[test]
    fn test_parse_stream_skips_unsupported_language() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, "hello").unwrap();

        let entry = FileEntry::new(path.clone(), 5);
        let reader = FileReader::new(FileReadConfig {
            num_threads: 1,
            buffer_size: 2,
            ..Default::default()
        });
        let parser = FileParser::new(
            Arc::new(Parser::new()),
            FileParseConfig {
                num_threads: 1,
                buffer_size: 2,
            },
        );

        let parse_outcomes: Vec<_> = parser
            .parse_stream(reader.read_files(vec![entry]))
            .iter()
            .collect();

        assert_eq!(parse_outcomes.len(), 1);
        match &parse_outcomes[0] {
            FileParseOutcome::Skipped(skip) => {
                assert_eq!(skip.reason, FileSkipReason::UnsupportedLanguage);
                assert_eq!(skip.path, path);
            }
            _ => panic!("expected unsupported language skip"),
        }
    }

    #[test]
    fn test_parse_stream_parses_supported_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("main.rs");
        fs::write(&path, "fn main() {}").unwrap();

        let entry = FileEntry::new(path.clone(), 12);
        let reader = FileReader::new(FileReadConfig {
            num_threads: 1,
            buffer_size: 2,
            ..Default::default()
        });
        let parser = FileParser::new(
            Arc::new(Parser::new()),
            FileParseConfig {
                num_threads: 1,
                buffer_size: 2,
            },
        );

        let parse_outcomes: Vec<_> = parser
            .parse_stream(reader.read_files(vec![entry]))
            .iter()
            .collect();

        assert_eq!(parse_outcomes.len(), 1);
        match &parse_outcomes[0] {
            FileParseOutcome::Parsed(parsed) => {
                assert_eq!(parsed.path, path);
                assert_eq!(parsed.language, Language::Rust);
                assert!(!parsed.chunks.is_empty());
            }
            _ => panic!("expected parsed file"),
        }
    }
}
