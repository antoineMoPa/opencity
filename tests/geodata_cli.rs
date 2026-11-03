#![cfg(all(feature = "geodata", not(target_arch = "wasm32")))]
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "opencity-geodata-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, value: Value) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn cli_fetches_only_requested_geometry_without_simplifying_or_touching_maps() {
    let dir = Fixture::new();
    let input = dir.write(
        "source.json",
        json!({"elements":[{
            "type":"way","id":8,"nodes":[1,2,3,4,5],
            "tags":{"name":"Canal Road West","highway":"secondary"},
            "geometry":[{"lon":-0.1,"lat":0.5},{"lon":0.2,"lat":0.5},
                {"lon":0.3,"lat":0.5},{"lon":0.7,"lat":0.5},{"lon":1.1,"lat":0.5}]
        }]}),
    );
    let output = dir.0.join("street.geojson");
    let raw = dir.0.join("saved.json");
    let map = dir.0.join("map.road");
    fs::write(&map, "# keep this authored map\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args([
            "fetch-street",
            "--name",
            "Canal",
            "--bbox",
            "0,0,1,1",
            "--input",
        ])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--raw-output")
        .arg(&raw)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(data["features"].as_array().unwrap().len(), 1);
    assert_eq!(
        data["features"][0]["geometry"]["coordinates"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        data["features"][0]["properties"]["source_node_ids"],
        json!([null, 2, 3, 4, null])
    );
    assert_eq!(fs::read(&raw).unwrap(), fs::read(&input).unwrap());
    assert_eq!(
        fs::read_to_string(map).unwrap(),
        "# keep this authored map\n"
    );
}

#[test]
fn cli_reports_address_candidates_to_stdout_and_preserves_output_on_bad_sources() {
    let dir = Fixture::new();
    let input = dir.write(
        "places.json",
        json!({"elements":[{
            "type":"node","id":9,"lon":0.4,"lat":0.5,
            "tags":{"addr:street":"Canal Road","addr:housenumber":"42"}
        }]}),
    );
    let result = Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args([
            "lookup-place",
            "--street",
            "Canal Road",
            "--number",
            "42",
            "--bbox",
            "0,0,1,1",
            "--input",
        ])
        .arg(input)
        .output()
        .unwrap();
    assert!(result.status.success());
    let data: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        data["features"][0]["geometry"]["coordinates"],
        json!([0.4, 0.5])
    );
    let bad = dir.write(
        "bad.json",
        json!({"remark":"runtime error: timeout","elements":[]}),
    );
    let output = dir.0.join("existing.geojson");
    fs::write(&output, "preserve").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args([
            "fetch-street",
            "--name",
            "Canal",
            "--bbox",
            "0,0,1,1",
            "--input",
        ])
        .arg(&bad)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(output).unwrap(), "preserve");
    let help = Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args(["fetch-street", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("No simplification"));
}

#[test]
fn source_metadata_survives_offline_conversion_and_raw_replay() {
    let dir = Fixture::new();
    let input = dir.write(
        "source.json",
        json!({"elements":[{"type":"node","id":7,
        "lat":0.5,"lon":0.5,"tags":{"name":"Sample Building"}}]}),
    );
    let metadata = json!({
        "attribution": "Fixture contributors",
        "license": "https://data.example/license",
        "object_url_template": "https://data.example/{type}/{id}",
        "dataset": "fixture"
    });
    let sidecar = dir.write("metadata.json", metadata.clone());
    let replay = dir.0.join("replay.json");
    let invoke = |source: &std::path::Path| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_opencity"));
        command
            .args([
                "lookup-place",
                "--name",
                "Sample",
                "--bbox",
                "0,0,1,1",
                "--input",
            ])
            .arg(source);
        command
    };
    let result = invoke(&input)
        .arg("--source-metadata")
        .arg(&sidecar)
        .arg("--raw-output")
        .arg(&replay)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(data["properties"]["source_metadata"], metadata);
    assert_eq!(data["properties"]["attribution"], "Fixture contributors");
    assert_eq!(
        data["properties"]["license"],
        "https://data.example/license"
    );
    assert_eq!(
        data["features"][0]["properties"]["source_url"],
        "https://data.example/node/7"
    );
    let result = invoke(&replay).output().unwrap();
    assert!(result.status.success());
    let replayed: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(data, replayed);
    let result = invoke(&input).output().unwrap();
    let unspecified: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(unspecified["properties"]["license"].is_null());
    assert!(unspecified["properties"]["attribution"].is_null());
    assert!(unspecified["features"][0]["properties"]
        .get("source_url")
        .is_none());
    fs::write(&sidecar, "[]").unwrap();
    fs::write(dir.0.join("output.json"), "preserve").unwrap();
    let result = invoke(&input)
        .arg("--source-metadata")
        .arg(&sidecar)
        .arg("--output")
        .arg(dir.0.join("output.json"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(
        fs::read_to_string(dir.0.join("output.json")).unwrap(),
        "preserve"
    );
}

#[test]
fn network_lookup_requires_an_explicit_endpoint_before_running_curl() {
    let result = Command::new(env!("CARGO_BIN_EXE_opencity"))
        .env_remove("OPENCITY_OVERPASS_ENDPOINT")
        .env("PATH", "")
        .args(["fetch-street", "--name", "Canal", "--bbox", "0,0,1,1"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("network lookup requires --endpoint"));
}
