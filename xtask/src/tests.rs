use super::{clean, formatting, limits, sources};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Repository(PathBuf);

impl Repository {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "cleanup-xtask-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        sources::git(&path, &["init", "--quiet"], None).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, source: &str) {
        let full = self.0.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, source).unwrap();
    }

    fn stage(&self, files: &[&str]) {
        let mut args = vec!["add", "--"];
        args.extend_from_slice(files);
        sources::git(&self.0, &args, None).unwrap();
    }

    fn index(&self, file: &str) -> String {
        sources::read(&self.0, Path::new(file), true)
            .unwrap()
            .unwrap()
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn markdown_wraps_lists_without_splitting_inline_code_or_losing_hard_breaks() {
    let code = "`a realistic isometric render of a ${vehicle name}`";
    let source = format!(
        "- {} {code} {}  \r\n",
        "before ".repeat(15),
        "after ".repeat(15)
    );
    let result = formatting::format(Path::new("."), Path::new("notes.md"), &source).unwrap();
    assert!(result.contains(code));
    assert!(result.starts_with("- "));
    assert!(result.lines().skip(1).all(|line| line.starts_with("  ")));
    assert!(result.ends_with("  \r\n"));
    assert!(result.lines().all(|line| limits::width(line) <= 100));
    assert_eq!(
        formatting::format(Path::new("."), Path::new("notes.md"), &result).unwrap(),
        result
    );
}

#[test]
fn markdown_preserves_fences_tables_indented_code_and_link_definitions() {
    let long = "unbreakable".repeat(15);
    let source = format!(
        concat!(
            "---\ndescription: {long}\n---\n\n<pre>\n{long}\n</pre>\n\n",
            "> ```rust\n> {long}\n> ```\n\n| {long} |\n\n    {long}\n\n",
            "[reference]: https://example.invalid/{long}\n"
        ),
        long = long
    );
    let result = formatting::format(Path::new("."), Path::new("notes.md"), &source).unwrap();
    assert_eq!(result, source);
    assert!(!limits::inspect(Path::new("notes.md"), &result).is_empty());
}

#[test]
fn read_only_staged_check_never_changes_index_or_worktree() {
    let repo = Repository::new();
    let source = "prose ".repeat(30);
    repo.write("notes.md", &source);
    repo.stage(&["notes.md"]);
    let draft = format!("{source}\nUnstaged draft.\n");
    repo.write("notes.md", &draft);
    assert!(clean(&repo.0, true, true).is_err());
    assert_eq!(repo.index("notes.md"), source);
    assert_eq!(fs::read_to_string(repo.0.join("notes.md")).unwrap(), draft);
}

#[test]
fn staged_format_preserves_partial_edits_file_modes_and_unselected_modules() {
    let repo = Repository::new();
    repo.write("main.rs", "mod sibling;fn main(){println!(\"value\");}\n");
    repo.write("sibling.rs", "Unstaged module content.\n");
    let prose = "Long prose that needs wrapping. ".repeat(8);
    repo.write("notes.md", &prose);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(repo.0.join("main.rs"), fs::Permissions::from_mode(0o755)).unwrap();
        sources::git(&repo.0, &["config", "core.filemode", "true"], None).unwrap();
    }
    repo.stage(&["main.rs", "notes.md"]);
    let before = sources::git(&repo.0, &["ls-files", "--stage", "main.rs"], None).unwrap();
    let draft = format!("{prose}\nUnstaged draft remains exact.\n");
    repo.write("notes.md", &draft);
    clean(&repo.0, true, false).unwrap();
    assert!(repo.index("main.rs").contains("fn main() {"));
    assert_eq!(
        repo.index("main.rs"),
        fs::read_to_string(repo.0.join("main.rs")).unwrap()
    );
    assert!(repo
        .index("notes.md")
        .lines()
        .all(|line| limits::width(line) <= 100));
    assert!(!repo.index("notes.md").contains("Unstaged draft"));
    assert_eq!(fs::read_to_string(repo.0.join("notes.md")).unwrap(), draft);
    assert_eq!(
        fs::read_to_string(repo.0.join("sibling.rs")).unwrap(),
        "Unstaged module content.\n"
    );
    let after = sources::git(&repo.0, &["ls-files", "--stage", "main.rs"], None).unwrap();
    assert_eq!(&before[..6], &after[..6]);
    clean(&repo.0, true, true).unwrap();
}

#[test]
fn working_tree_format_does_not_stage_changes() {
    let repo = Repository::new();
    let source = "A long paragraph. ".repeat(15);
    repo.write("nested/notes.md", &source);
    repo.stage(&["nested/notes.md"]);
    clean(&repo.0, false, false).unwrap();
    assert_eq!(repo.index("nested/notes.md"), source);
    let formatted = fs::read_to_string(repo.0.join("nested/notes.md")).unwrap();
    assert_ne!(formatted, source);
    assert!(formatted.lines().all(|line| limits::width(line) <= 100));
    clean(&repo.0, false, true).unwrap();
}

#[test]
#[cfg(unix)]
fn staged_and_worktree_format_never_follow_symlinks() {
    use std::os::unix::fs::symlink;
    let repo = Repository::new();
    let external = Repository::new();
    let source = "External prose. ".repeat(20);
    external.write("notes.md", &source);
    symlink(external.0.join("notes.md"), repo.0.join("linked.md")).unwrap();
    repo.stage(&["linked.md"]);
    clean(&repo.0, true, false).unwrap();
    clean(&repo.0, false, false).unwrap();
    assert_eq!(
        fs::read_to_string(external.0.join("notes.md")).unwrap(),
        source
    );
    assert!(fs::symlink_metadata(repo.0.join("linked.md"))
        .unwrap()
        .file_type()
        .is_symlink());
}
