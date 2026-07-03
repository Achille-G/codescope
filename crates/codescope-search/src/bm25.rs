//! BM25 search via Tantivy

use crate::{Error, Result};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, INDEXED, STORED, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument};

/// Resolved handles to the schema fields, built once per index.
#[derive(Debug, Clone, Copy)]
struct Bm25Fields {
    chunk_id: Field,
    content: Field,
    symbol: Field,
    kind: Field,
    file: Field,
}

impl Bm25Fields {
    fn from_schema(schema: &Schema) -> Result<Self> {
        Ok(Self {
            chunk_id: schema.get_field("chunk_id")?,
            content: schema.get_field("content")?,
            symbol: schema.get_field("symbol")?,
            kind: schema.get_field("kind")?,
            file: schema.get_field("file")?,
        })
    }
}

/// BM25 search index using Tantivy
pub struct BM25Index {
    index: Index,
    reader: IndexReader,
    writer: Option<IndexWriter>,
    fields: Bm25Fields,
}

impl BM25Index {
    /// Create or open an index at the given path
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)?;

        let schema = Self::build_schema();

        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            Index::create_in_dir(path, schema)?
        };

        let reader = index.reader()?;
        let fields = Bm25Fields::from_schema(&index.schema())?;

        Ok(Self {
            index,
            reader,
            writer: None,
            fields,
        })
    }

    /// Create an in-memory index (for testing)
    pub fn open_memory() -> Result<Self> {
        let schema = Self::build_schema();
        let index = Index::create_in_ram(schema);

        let reader = index.reader()?;
        let fields = Bm25Fields::from_schema(&index.schema())?;

        Ok(Self {
            index,
            reader,
            writer: None,
            fields,
        })
    }

    fn build_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        // Chunk ID (stored for retrieval, indexed so delete_term can match it —
        // deleting by chunk_id is a silent no-op on a non-indexed field)
        schema_builder.add_i64_field("chunk_id", INDEXED | STORED);

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
            self.fields.chunk_id => chunk_id,
            self.fields.content => content,
            self.fields.kind => kind,
            self.fields.file => file,
        );

        if let Some(sym) = symbol {
            doc.add_text(self.fields.symbol, sym);
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
            let term = tantivy::Term::from_field_i64(self.fields.chunk_id, chunk_id);
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
            QueryParser::for_index(&self.index, vec![self.fields.content, self.fields.symbol]);

        let query = query_parser
            .parse_query(query)
            .map_err(|e| Error::Search(e.to_string()))?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_k))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            if let Some(chunk_id) = doc.get_first(self.fields.chunk_id) {
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
    fn test_bm25_delete_by_chunk_ids() {
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
                "fn hello_again() { println!(\"Hello again\"); }",
                Some("hello_again"),
                "function",
                "main.rs",
            )
            .unwrap();
        index.commit().unwrap();

        index.delete_by_chunk_ids(&[1]).unwrap();
        index.commit().unwrap();

        // Deletion must actually remove the document (regression: with a
        // non-INDEXED chunk_id field, delete_term was a silent no-op).
        let results = index.search("hello", 10).unwrap();
        assert!(results.iter().all(|(id, _)| *id != 1));
        assert!(results.iter().any(|(id, _)| *id == 2));
        assert_eq!(index.stats().unwrap().num_docs, 1);
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
