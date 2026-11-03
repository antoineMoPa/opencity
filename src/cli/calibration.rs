//! Explicit WGS84-to-authored-map mapping. This never edits or simplifies roads.
use serde_json::{json, Value};
use std::{collections::HashSet, ffi::OsString, fs, path::PathBuf};

mod mapping;
mod output;
#[cfg(test)]
mod tests;

#[cfg(test)]
use mapping::mercator;
use mapping::{pair, Mapping, Point};

const HELP: &str = "OpenCity calibrated coordinate mapping

  opencity map-coordinates --calibration MAP.json --lonlat LONGITUDE,LATITUDE
  opencity map-coordinates --calibration MAP.json --world X,Y
  opencity map-coordinates --calibration MAP.json --input SOURCE.geojson [--output WORLD.json]
  opencity map-coordinates --calibration MAP.json --report

Outputs JSON to stdout unless --output is supplied. Longitude comes first.
The report fits the saved control points and reports separate check-point errors.
Bulk input supports Point and LineString features from the geographic lookup commands.
Original GeoJSON is preserved alongside explicitly labeled world geometry;
world coordinates are game units, not geographic GeoJSON or physical metres.
No simplification, route joining, terrain changes or .road edits occur.
";

fn argument_pair(value: &str) -> Result<Point, String> {
    let numbers: Result<Vec<_>, _> = value.split(',').map(str::parse::<f64>).collect();
    pair(
        &json!(numbers.map_err(|_| "expected two comma-separated numbers")?),
        "coordinate",
    )
}

fn read_json(path: &PathBuf) -> Result<Value, String> {
    if fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len()
        > 16 * 1024 * 1024
    {
        return Err("JSON input exceeds 16 MiB".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub(super) fn run(args: &[OsString]) -> Result<(), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let (mut calibration, mut input, mut output) = (None, None, None);
    let (mut lonlat, mut world, mut report) = (None, None, false);
    let mut seen = HashSet::new();
    let mut i = 1;
    while i < args.len() {
        let flag = args[i].to_str().ok_or("option must be UTF-8")?;
        let flag = if flag == "-o" { "--output" } else { flag };
        if !seen.insert(flag) {
            return Err(format!("duplicate option {flag}"));
        }
        if flag == "--report" {
            report = true;
            i += 1;
            continue;
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--calibration" => calibration = Some(PathBuf::from(value)),
            "--input" => input = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--lonlat" => {
                lonlat = Some(argument_pair(
                    value.to_str().ok_or("coordinate must be UTF-8")?,
                )?)
            }
            "--world" => {
                world = Some(argument_pair(
                    value.to_str().ok_or("coordinate must be UTF-8")?,
                )?)
            }
            _ => return Err(format!("unknown option {flag}; see --help")),
        }
        i += 2;
    }
    if usize::from(report)
        + usize::from(input.is_some())
        + usize::from(lonlat.is_some())
        + usize::from(world.is_some())
        != 1
    {
        return Err("choose exactly one of --lonlat, --world, --input or --report".into());
    }
    let calibration = calibration.ok_or("--calibration is required")?;
    if output.as_ref() == Some(&calibration) || output.is_some() && output == input {
        return Err("output must differ from calibration and input paths".into());
    }
    // Resolve existing aliases too, so symlinks cannot overwrite either input.
    if let Some(out) = &output {
        if let Ok(out) = fs::canonicalize(out) {
            for source in [&calibration, input.as_ref().unwrap_or(&calibration)] {
                if fs::canonicalize(source).ok().as_ref() == Some(&out) {
                    return Err("output aliases an input file".into());
                }
            }
        }
    }
    let map = Mapping::load(read_json(&calibration)?)?;
    let result = if report {
        map.report()?
    } else if let Some(p) = lonlat {
        let w = map.to_world(p)?;
        json!({
            "lonlat":p,
            "world":w,
            "inside_map":map.inside(w),
            "calibration_id":map.source["id"],
            "quality":map.source["quality"]
        })
    } else if let Some(w) = world {
        json!({
            "world":w,
            "lonlat":map.to_lonlat(w)?,
            "inside_map":map.inside(w),
            "calibration_id":map.source["id"],
            "quality":map.source["quality"]
        })
    } else {
        map.map_geojson(read_json(&input.unwrap())?)?
    };
    let bytes = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
    if let Some(path) = output {
        fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    } else {
        println!("{}", String::from_utf8(bytes).map_err(|e| e.to_string())?);
    }
    Ok(())
}
