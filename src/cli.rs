//! Headless intersection exports; shared by the CLI and editor executable.
use crate::{network, parse_road_file, Statement};
use std::{ffi::OsString, path::PathBuf};

pub const HELP: &str = "OpenCity — road and intersection tools

USAGE:
  opencity intersection-svg INPUT.road --name NAME --output OUTPUT.svg [OPTIONS]
  opencity intersection-png INPUT.road --name NAME --output OUTPUT.png [OPTIONS]
  editor [INPUT.road]
  editor mcp

COMMANDS:
  intersection-svg   Export a standalone intersection SVG
  intersection-png   Export a standalone intersection PNG
  mcp                Run the editor automation bridge (editor executable)

OPTIONS:
  --name NAME       Component definition or placed instance to render
                    Optional when the file contains exactly one component
  -o, --output PATH Output file (required)
  --size PIXELS     Image width and height, 64–8192 (default: 1024)
  --no-paths        Hide diagnostic paths and permitted movement overlays
  --appearance-only Hide all path, movement, and port overlays
  -h, --help        Show this help without opening a window

Both export commands also work through the editor executable.
Component types: intersection, interchange, road_segment.
";

/// Return None for GUI/MCP arguments, or a completed headless operation.
pub fn run(args: Vec<OsString>) -> Option<Result<(), String>> {
    let first = args.first()?.to_str()?;
    if matches!(first, "--help" | "-h" | "help") {
        println!("{HELP}");
        return Some(Ok(()));
    }
    if !matches!(first, "intersection-svg" | "intersection-png") {
        return None;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Some(Ok(()));
    }
    Some(export(&args))
}

fn export(args: &[OsString]) -> Result<(), String> {
    let input = args
        .get(1)
        .filter(|arg| !arg.to_string_lossy().starts_with('-'))
        .ok_or("input .road file is required; see --help")?;
    let mut name = None;
    let mut output = None;
    let mut size = 1024;
    let mut show_paths = true;
    let mut show_ports = true;
    let mut index = 2;
    while index < args.len() {
        let flag = args[index].to_str().ok_or("option must be valid text")?;
        if flag == "--appearance-only" {
            show_paths = false;
            show_ports = false;
            index += 1;
            continue;
        }
        if flag == "--no-paths" {
            show_paths = false;
            index += 1;
            continue;
        }
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--name" => {
                name = Some(value.to_str().ok_or("name must be valid text")?.to_owned());
            }
            "--output" | "-o" => output = Some(PathBuf::from(value)),
            "--size" => {
                size = value
                    .to_str()
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|n| (64..=8192).contains(n))
                    .ok_or("--size must be an integer from 64 to 8192")?;
            }
            _ => return Err(format!("unknown option '{flag}'; see --help")),
        }
        index += 2;
    }
    let output = output.ok_or("--output is required")?;
    let program = parse_road_file(input).map_err(|e| e.to_string())?;
    let definitions: Vec<_> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Network(d) => Some(d),
            _ => None,
        })
        .collect();
    let projection = network::project(&program);
    let (title, geometry) = if let Some(name) = &name {
        let instances: Vec<_> = projection
            .instances
            .iter()
            .filter(|instance| &instance.name == name)
            .collect();
        let definitions: Vec<_> = definitions
            .iter()
            .filter(|definition| definition.name.as_ref() == Some(name))
            .collect();
        if instances.len() == 1 {
            (name.clone(), instances[0].geometry.clone())
        } else if instances.is_empty() && definitions.len() == 1 {
            (
                name.clone(),
                network::resolve(definitions[0]).map_err(|e| e.to_string())?,
            )
        } else if instances.is_empty() {
            let map = crate::map::project(&program);
            let intersection = crate::map::intersections(&program, &map)
                .into_iter()
                .find(|intersection| &intersection.point_ref == name);
            if let Some(instance) = intersection.as_ref().and_then(|intersection| {
                network::default_four_way_instance(&program, &map, intersection)
                    .or_else(|| network::default_three_way_instance(&program, &map, intersection))
            }) {
                (name.clone(), instance.geometry)
            } else {
                return Err(format!("unknown component or instance '{name}'"));
            }
        } else {
            return Err(format!("ambiguous instance '{name}'; select a unique name"));
        }
    } else if definitions.len() == 1 {
        let definition = definitions[0];
        (
            definition
                .name
                .clone()
                .unwrap_or_else(|| definition.kind.keyword().into()),
            network::resolve(definition)?,
        )
    } else {
        return Err("--name is required when there is more than one component (or none)".into());
    };
    let svg = network::to_svg_with_options(&geometry, &title, size, show_paths, show_ports);
    // Render before touching the destination, so invalid exports leave it intact.
    let bytes = if args[0] == "intersection-svg" {
        svg.into_bytes()
    } else {
        render_png(&svg)?
    };
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&output, bytes).map_err(|e| e.to_string())?;
    println!("Wrote {}", output.display());
    Ok(())
}

#[cfg(feature = "intersection-export")]
pub fn render_png(svg: &str) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixels = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or("could not allocate image")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    );
    pixels.encode_png().map_err(|e| e.to_string())
}
#[cfg(not(feature = "intersection-export"))]
pub fn render_png(_svg: &str) -> Result<Vec<u8>, String> {
    Err("PNG export requires the intersection-export Cargo feature".into())
}
