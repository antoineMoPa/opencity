use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{self, Command},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let staged = match args.as_slice() {
        [] => false,
        [arg] if arg == "--staged" => true,
        _ => return Err("Usage: cargo xtask [--staged]".into()),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask must be inside the repository")?;
    check_formatting(root)?;
    let failures = check_limits(root, staged)?;
    if !failures.is_empty() {
        return Err(format!(
            "{}\nWrap long lines manually and split oversized Rust modules.",
            failures.join("\n")
        )
        .into());
    }
    println!("Cargo formatting and source limits passed.");
    Ok(())
}

fn check_formatting(root: &Path) -> Result<()> {
    // The helper is a separate package, so explicitly format it as well.
    for manifest in ["Cargo.toml", "xtask/Cargo.toml"] {
        let status = Command::new("cargo")
            .current_dir(root)
            .args(["fmt", "--manifest-path", manifest, "--", "--check"])
            .status()?;
        if !status.success() {
            return Err("Rust formatting differs; run make fmt and stage the result.".into());
        }
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output.stdout)
}

fn check_limits(root: &Path, staged: bool) -> Result<Vec<String>> {
    let args: &[&str] = if staged {
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ]
    } else {
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ]
    };
    let listing = String::from_utf8(git(root, args)?)?;
    let files: BTreeSet<PathBuf> = listing
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(PathBuf::from)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "md" || extension == "rs")
        })
        .collect();
    let mut failures = Vec::new();
    for file in files {
        if !staged && !root.join(&file).is_file() {
            continue;
        }
        let source = if staged {
            String::from_utf8(git(root, &["show", &format!(":{}", file.display())])?)?
        } else {
            fs::read_to_string(root.join(&file))?
        };
        failures.extend(inspect(&file, &source));
    }
    Ok(failures)
}

fn inspect(file: &Path, source: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let mut lines = 0;
    for (index, line) in source.lines().enumerate() {
        lines += 1;
        let width: usize = line.chars().map(|ch| if ch == '\t' { 4 } else { 1 }).sum();
        if width > 100 {
            failures.push(format!(
                "{}:{}: {width} characters; maximum is 100.",
                file.display(),
                index + 1
            ));
        }
    }
    if file.extension().is_some_and(|extension| extension == "rs") && lines > 1200 {
        failures.push(format!(
            "{}: {lines} lines; Rust files must have at most 1200.",
            file.display()
        ));
    }
    failures
}
