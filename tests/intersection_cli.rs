use std::process::Command;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn help_and_headless_svg_exports_work_and_errors_are_nonzero() {
    let help = cli(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("intersection-svg"));
    let dir =
        std::env::temp_dir().join(format!("opencity-intersection-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cross.svg");
    let output = cli(&[
        "intersection-svg",
        "examples/intersections.road",
        "--name",
        "crossroads",
        "--output",
        path.to_str().unwrap(),
        "--size",
        "256",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = std::fs::read_to_string(&path).unwrap();
    assert!(svg.contains("data-from=\"N.in\""));
    assert!(svg.contains("width=\"256\""));
    let path = dir.join("must-stay.txt");
    std::fs::write(&path, "keep").unwrap();
    for args in [
        vec![
            "intersection-svg",
            "examples/intersections.road",
            "--name",
            "missing",
            "--output",
            path.to_str().unwrap(),
        ],
        vec![
            "intersection-svg",
            "examples/intersections.road",
            "--output",
            path.to_str().unwrap(),
            "--size",
            "0",
        ],
        vec!["intersection-svg", "examples/intersections.road"],
    ] {
        let output = cli(&args);
        assert!(!output.status.success());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(feature = "editor")]
#[test]
fn editor_help_lists_exports_without_launching_a_window() {
    let output = Command::new(env!("CARGO_BIN_EXE_editor"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("intersection-svg"));
    assert!(help.contains("editor mcp"));
}
