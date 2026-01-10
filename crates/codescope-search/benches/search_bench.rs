//! Benchmarks for the search crate
//!
//! Run with: cargo bench -p codescope-search

use codescope_search::{BM25Index, HNSWIndex, StoragePool};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::Rng;
use xxhash_rust::xxh3::xxh3_64;

/// Generate random f32 vector of given dimension
fn random_vector(dim: usize) -> Vec<f32> {
    let mut rng = rand::thread_rng();
    let mut vec: Vec<f32> = (0..dim).map(|_| rng.gen::<f32>()).collect();
    // Normalize for cosine similarity
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut vec {
            *x /= norm;
        }
    }
    vec
}

/// Generate synthetic code chunk content
fn generate_chunk_content(id: usize) -> String {
    format!(
        r#"/**
 * Function documentation for chunk {id}
 * This is a sample function that demonstrates the search functionality.
 */
export function process_{id}(input: string): ProcessResult {{
    const result = new ProcessResult();
    result.id = {id};
    result.input = input;
    result.timestamp = Date.now();

    if (input.length > 100) {{
        result.status = "truncated";
        result.output = input.substring(0, 100);
    }} else {{
        result.status = "success";
        result.output = input.toUpperCase();
    }}

    return result;
}}"#
    )
}

/// Setup BM25 index with N documents
fn setup_bm25(num_docs: usize) -> BM25Index {
    let mut index = BM25Index::open_memory().expect("Failed to create BM25 index");
    index
        .begin_write(100_000_000)
        .expect("Failed to begin write");

    for i in 0..num_docs {
        let content = generate_chunk_content(i);
        let symbol = format!("process_{i}");
        let file = format!("src/module_{}.ts", i % 100);
        index
            .add_document(i as i64, &content, Some(&symbol), "function", &file)
            .expect("Failed to add document");
    }

    index.commit().expect("Failed to commit");
    index
}

/// Setup HNSW index with N vectors
fn setup_hnsw(num_vectors: usize, dim: usize) -> HNSWIndex {
    let mut index = HNSWIndex::with_defaults(dim).expect("Failed to create HNSW index");

    for i in 0..num_vectors {
        let vector = random_vector(dim);
        index.add(i as i64, vector).expect("Failed to add vector");
    }

    index
}

/// Setup storage with N chunks
fn setup_storage(num_chunks: usize) -> StoragePool {
    let pool = StoragePool::open_memory(4).expect("Failed to create storage pool");

    {
        let storage = pool.get().expect("Failed to get storage");

        for i in 0..num_chunks {
            let file_path = format!("src/module_{}.ts", i % 100);
            let file_hash = xxh3_64(file_path.as_bytes()).to_le_bytes();
            let file_id = storage
                .upsert_file(&file_path, Some("typescript"), &file_hash, 1000)
                .expect("Failed to upsert file");

            let content = generate_chunk_content(i);
            let content_hash = xxh3_64(content.as_bytes()).to_le_bytes();
            let symbol = format!("process_{i}");

            storage
                .insert_chunk(
                    file_id,
                    Some(&symbol),
                    "function",
                    (i % 500) as u32 + 1,
                    (i % 500) as u32 + 20,
                    &content_hash,
                    &content,
                )
                .expect("Failed to insert chunk");
        }
    }

    pool
}

fn bench_bm25_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_search");

    for num_docs in [100, 1000, 5000, 10000] {
        let index = setup_bm25(num_docs);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("docs", num_docs), &index, |b, index| {
            b.iter(|| index.search(black_box("process function input"), black_box(10)));
        });
    }

    group.finish();
}

fn bench_bm25_search_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_search_queries");
    let index = setup_bm25(5000);

    let queries = [
        "function",
        "process input",
        "ProcessResult status",
        "export function process",
        "substring toUpperCase",
    ];

    for query in queries {
        group.bench_with_input(BenchmarkId::new("query", query), &index, |b, index| {
            b.iter(|| index.search(black_box(query), black_box(10)));
        });
    }

    group.finish();
}

fn bench_bm25_top_k(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_top_k");
    let index = setup_bm25(5000);

    for top_k in [5, 10, 25, 50, 100] {
        group.bench_with_input(BenchmarkId::new("k", top_k), &index, |b, index| {
            b.iter(|| index.search(black_box("process function"), black_box(top_k)));
        });
    }

    group.finish();
}

fn bench_hnsw_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_search");
    let dim = 384; // Common embedding dimension

    for num_vectors in [100, 1000, 5000, 10000] {
        let index = setup_hnsw(num_vectors, dim);
        let query = random_vector(dim);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("vectors", num_vectors),
            &(index, query),
            |b, (index, query)| {
                b.iter(|| index.search(black_box(query), black_box(10)));
            },
        );
    }

    group.finish();
}

fn bench_hnsw_dimensions(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_dimensions");
    let num_vectors = 5000;

    for dim in [128, 256, 384, 512, 768] {
        let index = setup_hnsw(num_vectors, dim);
        let query = random_vector(dim);

        group.bench_with_input(
            BenchmarkId::new("dim", dim),
            &(index, query),
            |b, (index, query)| {
                b.iter(|| index.search(black_box(query), black_box(10)));
            },
        );
    }

    group.finish();
}

fn bench_hnsw_top_k(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_top_k");
    let dim = 384;
    let index = setup_hnsw(5000, dim);
    let query = random_vector(dim);

    for top_k in [5, 10, 25, 50, 100] {
        group.bench_with_input(BenchmarkId::new("k", top_k), &top_k, |b, &top_k| {
            b.iter(|| index.search(black_box(&query), black_box(top_k)));
        });
    }

    group.finish();
}

fn bench_storage_get_chunk(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_get_chunk");

    for num_chunks in [100, 1000, 5000] {
        let pool = setup_storage(num_chunks);
        let storage = pool.get().expect("Failed to get storage");

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("chunks", num_chunks),
            &storage,
            |b, storage| {
                let mut rng = rand::thread_rng();
                b.iter(|| {
                    let chunk_id = rng.gen_range(1..=num_chunks as i64);
                    storage.get_chunk(black_box(chunk_id))
                });
            },
        );
    }

    group.finish();
}

fn bench_hnsw_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_add");
    let dim = 384;

    group.bench_function("add_single", |b| {
        let mut index = HNSWIndex::with_defaults(dim).unwrap();
        let mut id = 0i64;
        b.iter(|| {
            let vector = random_vector(dim);
            index.add(id, vector).unwrap();
            id += 1;
        });
    });

    group.finish();
}

fn bench_bm25_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_add");

    group.bench_function("add_single", |b| {
        let mut index = BM25Index::open_memory().unwrap();
        index.begin_write(100_000_000).unwrap();
        let mut id = 0i64;
        b.iter(|| {
            let content = generate_chunk_content(id as usize);
            let symbol = format!("process_{id}");
            index
                .add_document(id, &content, Some(&symbol), "function", "src/test.ts")
                .unwrap();
            id += 1;
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_bm25_search,
    bench_bm25_search_queries,
    bench_bm25_top_k,
    bench_hnsw_search,
    bench_hnsw_dimensions,
    bench_hnsw_top_k,
    bench_storage_get_chunk,
    bench_hnsw_add,
    bench_bm25_add,
);

criterion_main!(benches);
