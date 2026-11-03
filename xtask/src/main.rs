use std::{error::Error, path::Path, process};

mod formatting;
mod limits;
mod sources;
#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask must be inside the repository")?;
    let (staged, check) = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => (false, false),
        ["--staged"] => (true, false),
        ["--check"] => (false, true),
        ["--staged", "--check"] | ["--check", "--staged"] => (true, true),
        _ => return Err("Usage: cargo xtask [--staged] [--check]".into()),
    };
    clean(root, staged, check)
}

fn clean(root: &Path, staged: bool, check: bool) -> Result<()> {
    let mut failures = Vec::new();
    for file in sources::list(root, staged)? {
        let Some(source) = sources::read(root, &file, staged)? else {
            continue;
        };
        let formatted = formatting::format(root, &file, &source)?;
        if formatted != source {
            if check {
                failures.push(format!(
                    "{}: formatting differs; run cargo xtask.",
                    file.display()
                ));
            } else {
                sources::write(root, &file, &source, &formatted, staged)?;
            }
        }
        failures.extend(limits::inspect(
            &file,
            if check { &source } else { &formatted },
        ));
    }
    if !failures.is_empty() {
        return Err(format!(
            "{}\nReflow remaining code/table lines manually and split oversized Rust modules.",
            failures.join("\n")
        )
        .into());
    }
    println!("Formatting and source limits passed.");
    Ok(())
}
