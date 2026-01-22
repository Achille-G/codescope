//! BM25 search via Tantivy

use crate::{Error, Result};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, INDEXED, STORED, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument};

/// BM25 search index using Tantivy
pub struct BM25Index {
    index: Index,
    reader: IndexReader,
    writer: Option<IndexWriter>,
    chunk_id_field: Field,
    content_field: Field,
    symbol_field: Field,
    kind_field: Field,
    file_field: Field,
    chunk_id_indexed: bool,
}

impl BM25Index {
    /// Create or open an index at the given path
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)?;

        let schema = Self::build_schema();

        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            Index::create_in_dir(path, schema.clone())?
        };

        let schema = index.schema();
        let reader = index.reader()?;

        let chunk_id_field = schema.get_field("chunk_id").unwrap();
        let content_field = schema.get_field("content").unwrap();
        let symbol_field = schema.get_field("symbol").unwrap();
        let kind_field = schema.get_field("kind").unwrap();
        let file_field = schema.get_field("file").unwrap();
        let chunk_id_indexed = schema.get_field_entry(chunk_id_field).is_indexed();

        Ok(Self {
            index,
            reader,
            writer: None,
            chunk_id_field,
            content_field,
            symbol_field,
            kind_field,
            file_field,
            chunk_id_indexed,
        })
    }

    /// Create an in-memory index (for testing)
    pub fn open_memory() -> Result<Self> {
        let schema = Self::build_schema();
        let index = Index::create_in_ram(schema.clone());

        let schema = index.schema();
        let reader = index.reader()?;

        let chunk_id_field = schema.get_field("chunk_id").unwrap();
        let content_field = schema.get_field("content").unwrap();
        let symbol_field = schema.get_field("symbol").unwrap();
        let kind_field = schema.get_field("kind").unwrap();
        let file_field = schema.get_field("file").unwrap();
        let chunk_id_indexed = schema.get_field_entry(chunk_id_field).is_indexed();

        Ok(Self {
            index,
            reader,
            writer: None,
            chunk_id_field,
            content_field,
            symbol_field,
            kind_field,
            file_field,
            chunk_id_indexed,
        })
    }

    fn build_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        // Chunk ID (stored for retrieval)
        schema_builder.add_i64_field("chunk_id", STORED | INDEXED);

        // Content (full-text searchable)
        schema_builder.add_text_field("content", TEXT);

        // Symbol name (searchable with boost)
        schema_builder.add_text_field("symbol", TEXT);

        // Kind (function, class, etc.)
        schema_builder.add_text_field("kind", TEXT | STORED);

        // File path
        schema_builder.add_text_field("file", TEXT | STORED);

        schema_builder.build()
    }

    /// Begin a write session
    pub fn begin_write(&mut self, heap_size: usize) -> Result<()> {
        if !self.chunk_id_indexed {
            return Err(Error::Index(
                "BM25 schema is outdated (chunk_id is not indexed). Run `codescope clean` then `codescope index --all` to rebuild."
                    .to_string(),
            ));
        }
        if self.writer.is_none() {
            self.writer = Some(self.index.writer(heap_size)?);
        }
        Ok(())
    }

    /// Add a document to the index
    pub fn add_document(
        &mut self,
        chunk_id: i64,
        content: &str,
        symbol: Option<&str>,
        kind: &str,
        file: &str,
    ) -> Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| Error::Index("Writer not initialized".to_string()))?;

        let mut doc = doc!(
            self.chunk_id_field => chunk_id,
            self.content_field => content,
            self.kind_field => kind,
            self.file_field => file,
        );

        if let Some(sym) = symbol {
            doc.add_text(self.symbol_field, sym);
        }

        writer.add_document(doc)?;
        Ok(())
    }

    /// Delete documents by chunk IDs
    pub fn delete_by_chunk_ids(&mut self, chunk_ids: &[i64]) -> Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| Error::Index("Writer not initialized".to_string()))?;

        for &chunk_id in chunk_ids {
            let term = tantivy::Term::from_field_i64(self.chunk_id_field, chunk_id);
            writer.delete_term(term);
        }
        Ok(())
    }

    /// Commit changes
    pub fn commit(&mut self) -> Result<()> {
        if let Some(writer) = self.writer.as_mut() {
            writer.commit()?;
            // Reload reader after commit
            self.reader.reload()?;
        }
        Ok(())
    }

    /// End write session
    pub fn end_write(&mut self) -> Result<()> {
        self.commit()?;
        self.writer = None;
        Ok(())
    }

    /// Search the index
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<(i64, f32)>> {
        let searcher = self.reader.searcher();

        // Parse query against content and symbol fields
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.content_field, self.symbol_field]);

        let query = query_parser
            .parse_query(query)
            .map_err(|e| Error::Search(e.to_string()))?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_k))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            if let Some(chunk_id) = doc.get_first(self.chunk_id_field) {
                if let Some(id) = chunk_id.as_i64() {
                    results.push((id, score));
                }
            }
        }

        Ok(results)
    }

    /// Get index statistics
    pub fn stats(&self) -> Result<BM25Stats> {
        let searcher = self.reader.searcher();
        let num_docs = searcher.num_docs() as usize;
        Ok(BM25Stats { num_docs })
    }
}

/// BM25 index statistics
#[derive(Debug, Clone)]
pub struct BM25Stats {
    pub num_docs: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic() {
        let mut index = BM25Index::open_memory().unwrap();

        index.begin_write(50_000_000).unwrap();
        index
            .add_document(
                1,
                "fn hello_world() { println!(\"Hello\"); }",
                Some("hello_world"),
                "function",
                "main.rs",
            )
            .unwrap();
        index
            .add_document(
                2,
                "fn goodbye() { println!(\"Bye\"); }",
                Some("goodbye"),
                "function",
                "main.rs",
            )
            .unwrap();
        index.commit().unwrap();

        let results = index.search("hello", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_bm25_symbol_search() {
        let mut index = BM25Index::open_memory().unwrap();

        index.begin_write(50_000_000).unwrap();
        index
            .add_document(
                1,
                "def calculate_sum(a, b): return a + b",
                Some("calculate_sum"),
                "function",
                "math.py",
            )
            .unwrap();
        index.commit().unwrap();

        let results = index.search("calculate_sum", 10).unwrap();
        assert!(!results.is_empty());
    }
}
