//! This bench test simulates updating the human-readable IDs (HRIDs) in parent
//! links in a large directory of requirements.

#![allow(missing_docs)]

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use requiem::{Directory, Hrid};
use tempfile::TempDir;

/// Generates a large number of interlinked documents
fn preseed_directory(path: PathBuf) {
    let mut directory = Directory::new(path).load_all();
    for i in 1..=99 {
        directory.add_requirement("USR".to_string());
        directory.add_requirement("SYS".to_string());
        let mut requirement =
            directory.link_requirement(format!("SYS-{i:<03}"), format!("USR-{i:<03}"));
        requirement.parents_mut().next().unwrap().1.hrid = Hrid::try_from("WRONG-001").unwrap();
    }
}

use criterion::BatchSize;

fn update_hrids(c: &mut Criterion) {
    c.bench_function("update hrids", |b| {
        b.iter_batched(
            || {
                // Setup: create directory with broken HRIDs
                let tmp_dir = TempDir::new().unwrap();
                preseed_directory(tmp_dir.path().to_path_buf());
                tmp_dir
            },
            |tmp_dir| {
                Directory::new(tmp_dir.path().to_path_buf())
                    .load_all()
                    .update_hrids();
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, update_hrids);
criterion_main!(benches);
