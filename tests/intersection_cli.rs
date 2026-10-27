#![cfg(feature = "intersection-export")]
use std::process::Command;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn help_and_headless_svg_png_exports_work_and_errors_are_nonzero() {
    let help = cli(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("intersection-svg"));
    assert!(text.contains("intersection-png"));
    let dir =
        std::env::temp_dir().join(format!("opencity-intersection-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (command, extension) in [("intersection-svg", "svg"), ("intersection-png", "png")] {
        let path = dir.join(format!("cross.{extension}"));
        let output = cli(&[
            command,
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
        let bytes = std::fs::read(&path).unwrap();
        if extension == "svg" {
            assert!(String::from_utf8(bytes)
                .unwrap()
                .contains("data-from=\"N.in\""));
        } else {
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 256);
            assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 256);
        }
    }
    let path = dir.join("must-stay.txt");
    std::fs::write(&path, "keep").unwrap();
    for args in [
        vec![
            "intersection-png",
            "examples/intersections.road",
            "--name",
            "missing",
            "--output",
            path.to_str().unwrap(),
        ],
        vec![
            "intersection-png",
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
    assert!(help.contains("intersection-png"));
}

#[test]
fn default_appearance_export_has_stop_paint_sidewalks_and_an_opaque_background() {
    let svg = opencity::network::to_svg_with_options(
        opencity::network::default_four_way(),
        "default",
        512,
        false,
        false,
    );
    let bytes = opencity::cli::render_png(&svg).unwrap();
    let image = resvg::tiny_skia::Pixmap::decode_png(&bytes).unwrap();
    let mut white = 0;
    let mut sidewalk = 0;
    for pixel in image.pixels() {
        assert_eq!(pixel.alpha(), 255);
        if pixel.red() == 245 && pixel.green() == 246 && pixel.blue() == 239 {
            white += 1;
        }
        if [pixel.red(), pixel.green(), pixel.blue()]
            == opencity::network::road_color(opencity::RoadElementKind::Sidewalk, false)
        {
            sidewalk += 1;
        }
    }
    assert!(white > 100, "missing vehicle stop paint");
    assert!(sidewalk > 1000, "missing curved sidewalks");
}

#[test]
fn three_way_export_fills_pavement_between_the_two_curved_corners() {
    let source = include_str!("../examples/default_intersection.road")
        .lines()
        .filter(|line| !line.starts_with("south_approach"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(
            "intersection4(center, street);",
            "intersection3(center, street);",
        );
    let program = opencity::parse(&source).unwrap();
    let projection = opencity::map::project(&program);
    let junction = opencity::map::intersections(&program, &projection).remove(0);
    let instance =
        opencity::network::default_three_way_instance(&program, &projection, &junction).unwrap();
    let bounds = instance.geometry.bounds();
    let extent = (bounds[2] - bounds[0]).max(bounds[3] - bounds[1]) * 1.16;
    let svg = opencity::network::to_svg_with_options(&instance.geometry, "tee", 512, false, false);
    let png = opencity::cli::render_png(&svg).unwrap();
    let image = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
    for sign in [-1., 1.] {
        let x = sign * junction.footprint_half * 0.7;
        let y = -junction.footprint_half * 0.1;
        let px = ((x - (bounds[0] + bounds[2] - extent) / 2.) / extent * 512.) as u32;
        let py = ((y - (bounds[1] + bounds[3] - extent) / 2.) / extent * 512.) as u32;
        let pixel = image.pixel(px, py).unwrap();
        assert_eq!(
            [pixel.red(), pixel.green(), pixel.blue()],
            opencity::network::road_color(opencity::RoadElementKind::CarLane, false)
        );
    }
}
