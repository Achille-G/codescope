//! Benchmarks for the parser crate
//!
//! Run with: cargo bench -p codescope-parser

use codescope_parser::{Language, Parser};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

/// Generate synthetic TypeScript code with N functions
fn generate_typescript(num_functions: usize) -> String {
    let mut code = String::new();
    for i in 0..num_functions {
        code.push_str(&format!(
            r#"
/**
 * Function {i} documentation
 * @param x - The input parameter
 * @returns The computed result
 */
export function compute_{i}(x: number): number {{
    const result = x * {i} + Math.sqrt(x);
    if (result > 100) {{
        return result / 2;
    }}
    return result;
}}

"#
        ));
    }
    code
}

/// Generate synthetic Python code with N functions
fn generate_python(num_functions: usize) -> String {
    let mut code = String::new();
    for i in 0..num_functions {
        code.push_str(&format!(
            r#"
def compute_{i}(x: int) -> int:
    """
    Function {i} documentation.

    Args:
        x: The input parameter

    Returns:
        The computed result
    """
    result = x * {i} + (x ** 0.5)
    if result > 100:
        return result // 2
    return result

"#
        ));
    }
    code
}

/// Generate synthetic Rust code with N functions
fn generate_rust(num_functions: usize) -> String {
    let mut code = String::new();
    for i in 0..num_functions {
        code.push_str(&format!(
            r#"
/// Function {i} documentation
///
/// # Arguments
/// * `x` - The input parameter
///
/// # Returns
/// The computed result
pub fn compute_{i}(x: f64) -> f64 {{
    let result = x * {i}.0 + x.sqrt();
    if result > 100.0 {{
        result / 2.0
    }} else {{
        result
    }}
}}

"#
        ));
    }
    code
}

/// Generate a class with methods (TypeScript)
fn generate_typescript_class(num_methods: usize) -> String {
    let mut code = String::from(
        r#"
/**
 * A sample service class for benchmarking
 */
export class BenchmarkService {
    private data: Map<string, number> = new Map();

    constructor() {
        this.initialize();
    }

    private initialize(): void {
        this.data.set("default", 0);
    }
"#,
    );

    for i in 0..num_methods {
        code.push_str(&format!(
            r#"
    /**
     * Method {i} documentation
     */
    public method_{i}(input: string): number {{
        const value = this.data.get(input) ?? 0;
        return value * {i} + input.length;
    }}
"#
        ));
    }

    code.push_str("}\n");
    code
}

fn bench_parse_typescript(c: &mut Criterion) {
    let parser = Parser::new();
    let mut group = c.benchmark_group("parse_typescript");

    for num_functions in [10, 50, 100, 200] {
        let code = generate_typescript(num_functions);
        let bytes = code.len();

        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("functions", num_functions),
            &code,
            |b, code| {
                b.iter(|| parser.parse(black_box(code), Language::TypeScript));
            },
        );
    }

    group.finish();
}

fn bench_parse_python(c: &mut Criterion) {
    let parser = Parser::new();
    let mut group = c.benchmark_group("parse_python");

    for num_functions in [10, 50, 100, 200] {
        let code = generate_python(num_functions);
        let bytes = code.len();

        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("functions", num_functions),
            &code,
            |b, code| {
                b.iter(|| parser.parse(black_box(code), Language::Python));
            },
        );
    }

    group.finish();
}

fn bench_parse_rust(c: &mut Criterion) {
    let parser = Parser::new();
    let mut group = c.benchmark_group("parse_rust");

    for num_functions in [10, 50, 100, 200] {
        let code = generate_rust(num_functions);
        let bytes = code.len();

        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("functions", num_functions),
            &code,
            |b, code| {
                b.iter(|| parser.parse(black_box(code), Language::Rust));
            },
        );
    }

    group.finish();
}

fn bench_parse_class(c: &mut Criterion) {
    let parser = Parser::new();
    let mut group = c.benchmark_group("parse_class");

    for num_methods in [10, 25, 50, 100] {
        let code = generate_typescript_class(num_methods);
        let bytes = code.len();

        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("methods", num_methods),
            &code,
            |b, code| {
                b.iter(|| parser.parse(black_box(code), Language::TypeScript));
            },
        );
    }

    group.finish();
}

fn bench_language_comparison(c: &mut Criterion) {
    let parser = Parser::new();
    let mut group = c.benchmark_group("language_comparison");

    // Use 50 functions for comparison
    let ts_code = generate_typescript(50);
    let py_code = generate_python(50);
    let rs_code = generate_rust(50);

    group.bench_function("typescript_50fn", |b| {
        b.iter(|| parser.parse(black_box(&ts_code), Language::TypeScript));
    });

    group.bench_function("python_50fn", |b| {
        b.iter(|| parser.parse(black_box(&py_code), Language::Python));
    });

    group.bench_function("rust_50fn", |b| {
        b.iter(|| parser.parse(black_box(&rs_code), Language::Rust));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_typescript,
    bench_parse_python,
    bench_parse_rust,
    bench_parse_class,
    bench_language_comparison,
);

criterion_main!(benches);
