use super::*;

#[test]
fn exact_limits_and_utf8_characters_are_allowed() {
    let source = format!("//{}\n", "é".repeat(98)).repeat(1200);
    assert!(inspect(Path::new("source.rs"), &source).is_empty());
    assert!(inspect(Path::new("guide.md"), &format!("{}\r\n", "x".repeat(100))).is_empty());
    assert!(inspect(Path::new("source.rs"), "").is_empty());
}

#[test]
fn excessive_line_lengths_file_lengths_and_tabs_fail() {
    for file in ["source.rs", "guide.md"] {
        assert_eq!(inspect(Path::new(file), &"x".repeat(101)).len(), 1);
        assert_eq!(inspect(Path::new(file), &"\t".repeat(26)).len(), 1);
    }
    let source = "// comment\n".repeat(1201);
    assert_eq!(inspect(Path::new("source.rs"), &source).len(), 1);
    assert!(inspect(Path::new("guide.md"), &source).is_empty());
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("source-limits-{}", process::id()));
        fs::create_dir(&path).unwrap();
        git(&path, &["init", "--quiet"]).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) {
        fs::write(self.0.join(name), contents).unwrap();
    }

    fn stage(&self) {
        git(&self.0, &["add", "--all"]).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn git_index_and_working_tree_are_checked_independently() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "ignored.md\n");
    fixture.write("ignored.md", &"x".repeat(101));
    fixture.write("guide with spaces.md", "valid\n");
    fixture.write("source.rs", "// valid\n");
    fixture.stage();
    let index = git(&fixture.0, &["diff", "--cached", "--binary"]).unwrap();
    fixture.write("guide with spaces.md", &"x".repeat(101));
    assert!(check_limits(&fixture.0, true).unwrap().is_empty());
    assert_eq!(check_limits(&fixture.0, false).unwrap().len(), 1);
    assert_eq!(
        git(&fixture.0, &["diff", "--cached", "--binary"]).unwrap(),
        index
    );
    fixture.stage();
    fixture.write("guide with spaces.md", "valid\n");
    assert_eq!(check_limits(&fixture.0, true).unwrap().len(), 1);
    assert!(check_limits(&fixture.0, false).unwrap().is_empty());
    fixture.stage();
    fixture.write("new.rs", &"// comment\n".repeat(1201));
    assert_eq!(check_limits(&fixture.0, false).unwrap().len(), 1);
    assert!(check_limits(&fixture.0, true).unwrap().is_empty());
    fs::remove_file(fixture.0.join("new.rs")).unwrap();
    fs::remove_file(fixture.0.join("guide with spaces.md")).unwrap();
    fixture.stage();
    assert!(check_limits(&fixture.0, true).unwrap().is_empty());
}
