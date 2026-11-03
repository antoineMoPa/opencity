#![cfg(all(feature = "geodata", not(target_arch = "wasm32")))]
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "opencity-calibration-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, value: &Value) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn calibration() -> Value {
    json!({"schema":"opencity-georeference-v1","id":"test-map","projection":"local_web_mercator",
        "origin_lonlat":[-73.7,45.5],"world_bounds":[-100.,-100.,100.,100.],
        "fit_controls":[
            {"id":"a","lonlat":[-73.7,45.5],"world":[2.,3.]},
            {"id":"b","lonlat":[-73.69,45.5],"world":[12.,4.]},
            {"id":"c","lonlat":[-73.7,45.51],"world":[1.,-8.]}
        ],"check_controls":[{"id":"check","lonlat":[-73.69,45.51],"world":[11.,-7.]}]})
}
fn command(calibration: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_opencity"));
    command
        .args(["map-coordinates", "--calibration"])
        .arg(calibration);
    command
}

#[test]
fn maps_and_reverses_geography_and_reports_independent_checks() {
    let dir = Fixture::new("roundtrip");
    let path = dir.write("map.json", &calibration());
    let output = command(&path)
        .args(["--lonlat", "-73.69,45.51"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!((value["world"][0].as_f64().unwrap() - 11.).abs() < 1e-8);
    assert!((value["world"][1].as_f64().unwrap() + 7.).abs() < 1e-8);
    let output = command(&path).args(["--world", "11,-7"]).output().unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!((value["lonlat"][0].as_f64().unwrap() + 73.69).abs() < 1e-9);
    assert!((value["lonlat"][1].as_f64().unwrap() - 45.51).abs() < 1e-9);
    let output = command(&path).arg("--report").output().unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["fit"]["count"], 3);
    assert_eq!(value["independent_checks"]["count"], 1);
    assert!(value["independent_checks"]["max_world"].as_f64().unwrap() < 1e-8);
}

#[test]
fn bulk_mapping_retains_original_data_and_never_overwrites_inputs() {
    let dir = Fixture::new("bulk");
    let path = dir.write("map.json", &calibration());
    let original = json!({"type":"FeatureCollection","properties":{"simplified":false},"features":[{
        "type":"Feature","properties":{"osm_id":8,"source_node_ids":[4,5,6]},
        "geometry":{"type":"LineString","coordinates":[[-73.7,45.5],[-73.695,45.5],[-73.69,45.5]]}
    }]});
    let input = dir.write("source.geojson", &original);
    let output = dir.0.join("world.json");
    let result = command(&path)
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mapped: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(mapped["source_geojson"], original);
    assert_eq!(mapped["vertex_count"], 3);
    assert_eq!(mapped["type"], "OpenCityMappedFeatures");
    let before = fs::read(&input).unwrap();
    let result = command(&path)
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&input)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&input).unwrap(), before);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
        calibration()
    );
}

#[test]
fn malformed_calibration_or_source_leaves_existing_output_intact() {
    let dir = Fixture::new("invalid");
    let mut invalid = calibration();
    invalid["fit_controls"][2]["lonlat"] = json!([-73.68, 45.5]);
    let path = dir.write("map.json", &invalid);
    let output = dir.0.join("output.json");
    fs::write(&output, "keep").unwrap();
    let result = command(&path)
        .arg("--report")
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    dir.write("map.json", &calibration());
    let input = dir.write(
        "source.geojson",
        &json!({"type":"FeatureCollection","features":[{
            "type":"Feature","geometry":{"type":"Point","coordinates":[0.,90.]}
        }]}),
    );
    let result = command(&path)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
}
