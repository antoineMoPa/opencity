//! Keep handwritten Rust source small enough to review as coherent modules.
use std::{fs, path::Path};

const MAX_SOURCE_LINES: usize = 1200;

fn inspect(path: &Path, failures: &mut Vec<String>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read source directory") {
            inspect(&entry.expect("read source entry").path(), failures);
        }
    } else if path.extension().is_some_and(|extension| extension == "rs") {
        let source = fs::read_to_string(path).expect("read Rust source");
        let lines = source.lines().count();
        if lines > MAX_SOURCE_LINES {
            failures.push(format!("{}: {lines} lines", path.display()));
        }
    }
}

#[test]
fn handwritten_rust_files_stay_within_reviewable_size() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for source in ["src", "tests", "build.rs"] {
        let path = root.join(source);
        if path.exists() {
            inspect(&path, &mut failures);
        }
    }
    failures.sort();
    assert!(
        failures.is_empty(),
        "Rust source exceeds {MAX_SOURCE_LINES} physical lines:\n{}",
        failures.join("\n")
    );
}
