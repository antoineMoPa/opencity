//! Bounded, read-only source lookup. Game-map decisions stay outside this CLI.
use serde_json::Value;
use std::{ffi::OsString, fs, io::Write, path::PathBuf, process::Command};

mod clip;
mod features;
mod pbf;
#[cfg(test)]
mod tests;

const MAX_BYTES: usize = 16 * 1024 * 1024;
const HELP: &str = "OpenCity selective geographic lookup

  opencity fetch-street --name NAME --bbox WEST,SOUTH,EAST,NORTH [OPTIONS]
  opencity lookup-place --name NAME --bbox WEST,SOUTH,EAST,NORTH [OPTIONS]
  opencity lookup-place --street STREET --number NUMBER --bbox W,S,E,N [OPTIONS]

  --output PATH       Write GeoJSON; otherwise print it to stdout
  --input PATH        Read a saved Overpass JSON response instead of fetching
  --pbf PATH          Read a local PBF extract; no network requests
  --raw-output PATH   Save source JSON for replay (PBF: resolved matching objects)
  --endpoint URL      Overpass endpoint (or OPENCITY_OVERPASS_ENDPOINT)
  --source-metadata PATH  Optional JSON attribution, license and object_url_template
  --name NAME         Literal fragment, ASCII case-insensitive; keep accents/hyphens
  --bbox W,S,E,N      Required local WGS84 crop, at most one square degree

Street output keeps all in-crop vertices and source way/node IDs. Only boundary
intersections are inserted. Ways, branches and carriageways are not merged.
No simplification, invented connections, game coordinates or .road edits occur.
Place lookup returns candidates, not an automatic choice. A way/relation's
reported position is its bounding-box center, not a surveyed entrance.
The agent chooses what to keep and simplifies it separately after calibration.
Network fetches require curl; --input and --pbf work offline. Commands are read-only.
";

#[derive(Clone, Debug)]
enum Selection {
    Name(String),
    Address { street: String, number: String },
}

#[derive(Debug)]
struct Options {
    street: bool,
    selection: Selection,
    bounds: clip::Bounds,
    output: Option<PathBuf>,
    input: Option<PathBuf>,
    pbf: Option<PathBuf>,
    raw_output: Option<PathBuf>,
    endpoint: String,
    source_metadata: Option<Value>,
}

fn text(value: &std::ffi::OsStr, flag: &str) -> Result<String, String> {
    let value = value
        .to_str()
        .ok_or_else(|| format!("{flag} must be UTF-8"))?;
    if value.is_empty() || value.chars().all(char::is_whitespace) {
        return Err(format!("{flag} cannot be empty"));
    }
    Ok(value.into())
}

fn options(args: &[OsString]) -> Result<Options, String> {
    let street = args[0] == "fetch-street";
    let (mut name, mut address_street, mut number, mut bounds) = (None, None, None, None);
    let (mut output, mut input, mut raw_output, mut pbf) = (None, None, None, None);
    let mut endpoint = std::env::var("OPENCITY_OVERPASS_ENDPOINT").unwrap_or_default();
    let mut metadata_path = None;
    let mut seen = std::collections::HashSet::new();
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].to_str().ok_or("option must be UTF-8")?;
        let key = if flag == "-o" { "--output" } else { flag };
        if !seen.insert(key) {
            return Err(format!("duplicate option {key}"));
        }
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--name" => name = Some(text(value, flag)?),
            "--street" => address_street = Some(text(value, flag)?),
            "--number" => number = Some(text(value, flag)?),
            "--bbox" => bounds = Some(clip::Bounds::parse(&text(value, flag)?)?),
            "--output" | "-o" => output = Some(PathBuf::from(value)),
            "--input" => input = Some(PathBuf::from(value)),
            "--pbf" => pbf = Some(PathBuf::from(value)),
            "--raw-output" => raw_output = Some(PathBuf::from(value)),
            "--endpoint" => endpoint = text(value, flag)?,
            "--source-metadata" => metadata_path = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown option {flag}; see --help")),
        }
        index += 2;
    }
    let selection = match (name, address_street, number) {
        (Some(name), None, None) => Selection::Name(name),
        (None, Some(street), Some(number)) if !street.is_empty() => {
            Selection::Address { street, number }
        }
        _ => return Err("use --name, or --street with --number, but not both".into()),
    };
    if street && !matches!(selection, Selection::Name(_)) {
        return Err("fetch-street requires --name".into());
    }
    if input.is_some() && pbf.is_some() {
        return Err("choose --input JSON or --pbf, not both".into());
    }
    if (input.is_some() || pbf.is_some()) && seen.contains("--endpoint") {
        return Err("--endpoint cannot be combined with a local input".into());
    }
    let paths = [&input, &pbf, &output, &raw_output, &metadata_path];
    for i in 0..paths.len() {
        for j in i + 1..paths.len() {
            if paths[i].is_some() && paths[i] == paths[j] {
                return Err("input, pbf, output and raw-output must be different files".into());
            }
            if let (Some(a), Some(b)) = (paths[i], paths[j]) {
                if let (Ok(a), Ok(b)) = (fs::canonicalize(a), fs::canonicalize(b)) {
                    if a == b {
                        return Err("Geographic input/output paths alias the same file".into());
                    }
                }
            }
        }
    }
    if input.is_none()
        && pbf.is_none()
        && !endpoint.is_empty()
        && !(endpoint.starts_with("https://") || endpoint.starts_with("http://"))
    {
        return Err("--endpoint must be an HTTP or HTTPS URL".into());
    }
    let source_metadata = metadata_path
        .map(|path| {
            if fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_BYTES as u64 {
                return Err("source metadata exceeds the 16 MiB limit".into());
            }
            let value: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("invalid source metadata JSON: {e}"))?;
            if !value.is_object() {
                return Err("source metadata must be a JSON object".into());
            }
            for key in ["attribution", "license", "object_url_template"] {
                if value.get(key).is_some_and(|v| !v.is_string()) {
                    return Err(format!("source metadata {key} must be a string"));
                }
            }
            Ok(value)
        })
        .transpose()?;
    Ok(Options {
        street,
        selection,
        bounds: bounds.ok_or("--bbox is required")?,
        output,
        input,
        pbf,
        raw_output,
        endpoint,
        source_metadata,
    })
}

fn regex_literal(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        if "\\.^$|?*+()[]{}".contains(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn query(options: &Options) -> String {
    let kind = if options.street { "way" } else { "nwr" };
    let highway = if options.street { "[highway]" } else { "" };
    let bbox = options.bounds.overpass();
    let clauses = match &options.selection {
        Selection::Name(name) => {
            let pattern = serde_json::to_string(&regex_literal(name)).unwrap();
            ["name", "name:fr", "alt_name"]
                .iter()
                .map(|key| format!("{kind}{highway}[\"{key}\"~{pattern},i]{bbox};"))
                .collect::<String>()
        }
        Selection::Address { street, number } => {
            let street = serde_json::to_string(&regex_literal(street)).unwrap();
            let number = serde_json::to_string(number).unwrap();
            format!("nwr[\"addr:street\"~{street},i][\"addr:housenumber\"={number}]{bbox};")
        }
    };
    let output = if options.street {
        "geom"
    } else {
        "body center"
    };
    format!("[out:json][timeout:25];({clauses});out {output};")
}

fn fetch(options: &Options) -> Result<Vec<u8>, String> {
    if let Some(path) = &options.pbf {
        return pbf::read(path, options);
    }
    if let Some(path) = &options.input {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_BYTES as u64 {
            return Err("source response exceeds the 16 MiB limit".into());
        }
        return fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    }
    if options.endpoint.is_empty() {
        return Err(concat!(
            "network lookup requires --endpoint URL or OPENCITY_OVERPASS_ENDPOINT; ",
            "use --input or --pbf for offline data"
        )
        .into());
    }
    let data = format!("data={}", query(options));
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--location",
            "--get",
        ])
        .args(["--connect-timeout", "10", "--max-time", "35"])
        .args(["--max-filesize", &MAX_BYTES.to_string()])
        .args([
            "--user-agent",
            "OpenCityCLI/0.1 (bounded geographic lookup)",
        ])
        .args(["--data-urlencode", &data, "--url", &options.endpoint])
        .output()
        .map_err(|e| format!("could not run curl: {e}; use --input for offline data"))?;
    if !output.status.success() {
        return Err(format!(
            "Overpass fetch failed: {}. Try --endpoint or --input.",
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    if output.stdout.len() > MAX_BYTES {
        return Err("source response exceeds the 16 MiB limit".into());
    }
    Ok(output.stdout)
}

fn write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub(super) fn run(args: &[OsString]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let options = options(args)?;
    let mut raw = fetch(&options)?;
    let mut source: Value =
        serde_json::from_slice(&raw).map_err(|e| format!("invalid Overpass JSON: {e}"))?;
    if let Some(metadata) = &options.source_metadata {
        let object = source
            .as_object_mut()
            .ok_or("source response must be a JSON object")?;
        object.insert("opencity_source_metadata".into(), metadata.clone());
        raw = serde_json::to_vec(&source).map_err(|e| e.to_string())?;
    }
    let collection = features::convert(&source, &options)?;
    let mut bytes = serde_json::to_vec_pretty(&collection).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    // Validate everything before touching either destination.
    if let Some(path) = &options.raw_output {
        write(path, &raw)?;
    }
    if let Some(path) = &options.output {
        write(path, &bytes)?;
        eprintln!(
            "Wrote {} ({} candidates)",
            path.display(),
            collection["features"].as_array().unwrap().len()
        );
    } else {
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
