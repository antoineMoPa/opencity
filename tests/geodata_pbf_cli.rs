#![cfg(all(feature = "geodata", not(target_arch = "wasm32")))]
use osmpbfreader::{fileformat, osmformat};
use protobuf::Message;
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("opencity-pbf-{}-{name}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn input(&self, missing_node: bool) -> PathBuf {
        let p = self.0.join("extract.osm.pbf");
        fs::write(&p, pbf(missing_node)).unwrap();
        p
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn frame(kind: &str, raw: Vec<u8>) -> Vec<u8> {
    let mut blob = fileformat::Blob::new();
    blob.set_raw(raw);
    let payload = blob.write_to_bytes().unwrap();
    let mut header = fileformat::BlobHeader::new();
    header.set_type(kind.into());
    header.set_datasize(payload.len() as i32);
    let header = header.write_to_bytes().unwrap();
    let mut out = (header.len() as u32).to_be_bytes().to_vec();
    out.extend(header);
    out.extend(payload);
    out
}
fn pbf(missing_node: bool) -> Vec<u8> {
    let mut header = osmformat::HeaderBlock::new();
    header.required_features.push("OsmSchema-V0.6".into());
    let mut out = frame("OSMHeader", header.write_to_bytes().unwrap());
    let mut table = osmformat::StringTable::new();
    table.s = [
        "",
        "name",
        "Canal Road",
        "highway",
        "secondary",
        "Sample Hall",
        "addr:street",
        "addr:housenumber",
        "42",
        "building",
        "yes",
        "",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    let mut group = osmformat::PrimitiveGroup::new();
    let points = [
        [-0.1, 0.5],
        [0.2, 0.5],
        [0.3, 0.5],
        [0.7, 0.5],
        [1.1, 0.5],
        [0.5, 0.5],
        [0.4, 0.4],
        [0.6, 0.4],
        [0.6, 0.6],
        [0.4, 0.6],
    ];
    for (i, p) in points.iter().enumerate() {
        if missing_node && i == 2 {
            continue;
        }
        let mut n = osmformat::Node::new();
        n.set_id(i as i64 + 1);
        n.set_lon((p[0] * 1e7_f64).round() as i64);
        n.set_lat((p[1] * 1e7_f64).round() as i64);
        if i == 5 {
            n.keys = vec![1, 6, 7];
            n.vals = vec![5, 2, 8];
        }
        group.nodes.push(n);
    }
    let mut way = osmformat::Way::new();
    way.set_id(8);
    way.keys = vec![1, 3];
    way.vals = vec![2, 4];
    way.refs = vec![1, 1, 1, 1, 1];
    group.ways.push(way);
    let mut building = osmformat::Way::new();
    building.set_id(9);
    building.keys = vec![1, 6, 7, 9];
    building.vals = vec![5, 2, 8, 10];
    building.refs = vec![7, 1, 1, 1, -3];
    group.ways.push(building);
    let mut relation = osmformat::Relation::new();
    relation.set_id(11);
    relation.keys = vec![1];
    relation.vals = vec![5];
    relation.memids = vec![9];
    relation
        .types
        .push(osmformat::relation::MemberType::WAY.into());
    relation.roles_sid = vec![11];
    group.relations.push(relation);
    let mut block = osmformat::PrimitiveBlock::new();
    block.stringtable = protobuf::MessageField::some(table);
    block.primitivegroup.push(group);
    out.extend(frame("OSMData", block.write_to_bytes().unwrap()));
    out
}
fn command(input: &std::path::Path, command: &str) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_opencity"));
    c.arg(command)
        .arg("--pbf")
        .arg(input)
        .args(["--bbox", "0,0,1,1"]);
    c.env("OPENCITY_OVERPASS_ENDPOINT", "not-a-network-endpoint");
    c.env("PATH", ""); // Successful PBF reads need neither curl nor a network fallback.
    c
}
#[test]
fn local_street_keeps_nodes_and_collinear_vertices_and_replays_as_json() {
    let f = Fixture::new("street");
    let input = f.input(false);
    let raw = f.0.join("replay.json");
    let result = command(&input, "fetch-street")
        .args(["--name", "Canal"])
        .arg("--raw-output")
        .arg(&raw)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let v: Value = serde_json::from_slice(&result.stdout).unwrap();
    let feature = &v["features"][0];
    assert_eq!(v["features"].as_array().unwrap().len(), 1);
    assert_eq!(feature["properties"]["osm_id"], 8);
    assert_eq!(
        feature["geometry"]["coordinates"].as_array().unwrap().len(),
        5
    );
    assert_eq!(
        feature["properties"]["source_node_ids"],
        serde_json::json!([null, 2, 3, 4, null])
    );
    assert_eq!(v["properties"]["source"]["kind"], "local_pbf");
    assert_eq!(v["properties"]["simplified"], false);
    let replay = Command::new(env!("CARGO_BIN_EXE_opencity"))
        .args([
            "fetch-street",
            "--name",
            "Canal",
            "--bbox",
            "0,0,1,1",
            "--input",
        ])
        .arg(raw)
        .output()
        .unwrap();
    assert!(replay.status.success());
    assert_eq!(serde_json::from_slice::<Value>(&replay.stdout).unwrap(), v);
}
#[test]
fn local_places_keep_node_way_relation_candidates_and_explicit_addresses() {
    let f = Fixture::new("place");
    let input = f.input(false);
    let result = command(&input, "lookup-place")
        .args(["--name", "Sample Hall"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let v: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(v["features"].as_array().unwrap().len(), 3);
    assert!(v["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["properties"]["osm_type"] == "relation"
            && f["properties"]["position_kind"] == "bounding_box_center"));
    let result = command(&input, "lookup-place")
        .args(["--street", "Canal Road", "--number", "42"])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["features"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn incomplete_corrupt_or_conflicting_local_input_preserves_outputs() {
    let f = Fixture::new("invalid");
    let input = f.input(true);
    let output = f.0.join("keep.json");
    fs::write(&output, "keep").unwrap();
    let result = command(&input, "fetch-street")
        .args(["--name", "Canal"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("missing node"));
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    fs::write(&input, [0, 0, 0, 15, 0]).unwrap();
    let result = command(&input, "fetch-street")
        .args(["--name", "Canal"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("truncated PBF"));
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    let original = fs::read(&input).unwrap();
    let result = command(&input, "fetch-street")
        .args(["--name", "Canal"])
        .arg("--raw-output")
        .arg(&input)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&input).unwrap(), original);
    let result = command(&input, "fetch-street")
        .args(["--name", "Canal", "--input"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
}
