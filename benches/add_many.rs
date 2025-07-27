//! Benchmarks adding many requirements to a directory.

#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use requiem::Directory;
use tempfile::TempDir;

fn preseed_directory(path: &std::path::Path, n: usize) {
    let mut dir = Directory::new(path.to_path_buf()).load_all().unwrap();

    for _ in 0..n {
        dir.add_requirement("R".to_string()).unwrap();
    }
}

fn load_all(c: &mut Criterion) {
    c.bench_function("load directory pre-seeded with requirements", |b| {
        b.iter_batched(
            || {
                // Setup: create and pre-seed directory
                let tmp_dir = TempDir::new().unwrap();
                preseed_directory(tmp_dir.path(), 1000);
                tmp_dir
            },
            |tmp_dir| {
                let _loaded = Directory::new(tmp_dir.path().to_path_buf()).load_all();
            },
            BatchSize::SmallInput,
        );
    });
}

fn add_single_requirement_to_populated_dir(c: &mut Criterion) {
    c.bench_function("add one requirement to a pre-seeded directory", |b| {
        b.iter_batched(
            || {
                // Setup: create fresh directory for each iteration
                let tmp_dir = TempDir::new().expect("Failed to create temp dir");
                preseed_directory(tmp_dir.path(), 1000);
                tmp_dir
            },
            |tmp_dir| {
                // Note this routine deliberately includes the step to load the requirements
                // from disk, since this represents the true end-to-end user
                // workflow.
                Directory::new(tmp_dir.path().to_path_buf())
                    .load_all()
                    .unwrap()
                    .add_requirement(black_box("R".to_string()))
                    .unwrap();
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, load_all, add_single_requirement_to_populated_dir);
criterion_main!(benches);
