//! Benchmarks adding many requirements to a directory.

#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;
use req::Directory;

fn preseed_directory(path: &std::path::Path, n: usize) {
    let mut dir = Directory::new(path.to_path_buf()).load_all();

    for _ in 0..n {
        dir.add_requirement("R".to_string());
    }
}

fn load_all(c: &mut Criterion) {
    c.bench_function("load directory pre-seeded with requirements", |b| {
        let tmp_dir = TempDir::new().unwrap();
        preseed_directory(tmp_dir.path(), 1000);

        b.iter(|| {
            let _loaded = Directory::new(tmp_dir.path().to_path_buf()).load_all();
        });
    });
}

fn add_single_requirement_to_populated_dir(c: &mut Criterion) {
    c.bench_function("add one requirement to a pre-seeded directory", |b| {
        let tmp_dir = TempDir::new().expect("Failed to create temp dir");

        // Pre-populate with requirements
        preseed_directory(tmp_dir.path(), 1000);

        b.iter(|| {
            // Benchmark one additional insert
            Directory::new(tmp_dir.path().to_path_buf()).load_all().add_requirement(black_box("R".to_string()));
        });
    });
}

criterion_group!(benches, load_all, add_single_requirement_to_populated_dir);
criterion_main!(benches);
