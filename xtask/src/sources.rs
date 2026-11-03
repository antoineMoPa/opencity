//! Git source selection and formatting writes that preserve unstaged contents.
use super::Result;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or("missing git stdin")?
            .write_all(input)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output.stdout)
}

pub(super) fn list(root: &Path, staged: bool) -> Result<BTreeSet<PathBuf>> {
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
    let listing = String::from_utf8(git(root, args, None)?)?;
    Ok(listing
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(PathBuf::from)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "md" || extension == "rs")
        })
        .collect())
}

fn staged_mode(root: &Path, file: &Path) -> Result<String> {
    let entry = git(
        root,
        &["ls-files", "--stage", "-z", "--", path(file)?],
        None,
    )?;
    let entry = String::from_utf8(entry)?;
    Ok(entry
        .split_whitespace()
        .next()
        .ok_or("missing staged file mode")?
        .to_owned())
}

fn path(file: &Path) -> Result<&str> {
    file.to_str()
        .ok_or_else(|| "source path must be UTF-8".into())
}

pub(super) fn read(root: &Path, file: &Path, staged: bool) -> Result<Option<String>> {
    if staged {
        if !matches!(staged_mode(root, file)?.as_str(), "100644" | "100755") {
            return Ok(None);
        }
        Ok(Some(String::from_utf8(git(
            root,
            &["show", &format!(":{}", path(file)?)],
            None,
        )?)?))
    } else {
        let full = root.join(file);
        match fs::symlink_metadata(&full) {
            Ok(metadata) if metadata.file_type().is_file() => Ok(Some(fs::read_to_string(full)?)),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

pub(super) fn write(
    root: &Path,
    file: &Path,
    original: &str,
    formatted: &str,
    staged: bool,
) -> Result<()> {
    if !staged {
        fs::write(root.join(file), formatted)?;
        println!("Formatted {}", file.display());
        return Ok(());
    }
    // Format the index snapshot, never stage unrelated worktree edits.
    if read(root, file, true)?.as_deref() != Some(original) {
        return Err(format!(
            "{}: staged contents changed during formatting",
            file.display()
        )
        .into());
    }
    let mode = staged_mode(root, file)?;
    let hash = git(
        root,
        &["hash-object", "-w", "--stdin"],
        Some(formatted.as_bytes()),
    )?;
    let hash = String::from_utf8(hash)?;
    git(
        root,
        &[
            "update-index",
            "--cacheinfo",
            &mode,
            hash.trim(),
            path(file)?,
        ],
        None,
    )?;
    let full = root.join(file);
    let same_worktree = fs::symlink_metadata(&full).is_ok_and(|m| m.file_type().is_file())
        && fs::read(&full).ok().as_deref() == Some(original.as_bytes());
    if same_worktree {
        fs::write(full, formatted)?;
        println!("Formatted and staged {}", file.display());
    } else {
        println!(
            "Formatted staged {}; unstaged contents preserved",
            file.display()
        );
    }
    Ok(())
}
