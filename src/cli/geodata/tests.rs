use super::*;
use serde_json::json;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn street_options() -> Options {
    options(&args(&[
        "fetch-street",
        "--name",
        "Canal",
        "--bbox",
        "0,0,1,1",
    ]))
    .unwrap()
}

fn point(x: f64, y: f64, node: u64) -> clip::Point {
    clip::Point {
        xy: [x, y],
        node: Some(node),
    }
}

#[test]
fn crop_keeps_all_source_vertices_and_marks_only_inserted_boundary_points() {
    let bounds = clip::Bounds::parse("0,0,1,1").unwrap();
    let points = [
        point(-0.5, 0.5, 1),
        point(0.2, 0.5, 2),
        point(0.4, 0.6, 3),
        point(0.8, 0.5, 4),
        point(1.5, 0.5, 5),
    ];
    let parts = clip::line(&points, bounds);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].len(), 5);
    assert_eq!(parts[0][1..4], points[1..4]);
    assert_eq!(
        parts[0][0],
        clip::Point {
            xy: [0.0, 0.5],
            node: None
        }
    );
    assert_eq!(
        parts[0][4],
        clip::Point {
            xy: [1.0, 0.5],
            node: None
        }
    );
}

#[test]
fn crop_never_connects_an_outside_excursion_or_disconnected_ways() {
    let bounds = clip::Bounds::parse("0,0,1,1").unwrap();
    let points = [
        point(0.2, 0.5, 1),
        point(1.5, 0.5, 2),
        point(1.5, 0.8, 3),
        point(0.2, 0.8, 4),
    ];
    let parts = clip::line(&points, bounds);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].iter().all(|p| p.xy[1] == 0.5));
    assert!(parts[1].iter().all(|p| p.xy[1] == 0.8));
    assert!(clip::line(&[point(2.0, 2.0, 1), point(3.0, 2.0, 2)], bounds).is_empty());
}

#[test]
fn street_selection_retains_even_collinear_points_and_named_minor_paths() {
    let source = json!({"elements": [
        {"type":"way", "id":10, "tags":{"name":"Canal Road", "highway":"primary"},
         "nodes":[1,2,3,4], "geometry":[{"lon":0.1,"lat":0.2},{"lon":0.2,"lat":0.2},
             {"lon":0.3,"lat":0.2},{"lon":0.4,"lat":0.2}]},
        {"type":"way", "id":11, "tags":{"name":"Other Road", "highway":"primary"}},
        {"type":"way", "id":12, "tags":{"name":"CANAL Path", "highway":"footway"},
         "nodes":[5,6], "geometry":[{"lon":0.5,"lat":0.3},{"lon":0.6,"lat":0.3}]}
    ]});
    let result = features::convert(&source, &street_options()).unwrap();
    let features = result["features"].as_array().unwrap();
    assert_eq!(features.len(), 2);
    assert_eq!(
        features[0]["geometry"]["coordinates"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        features[0]["properties"]["source_node_ids"],
        json!([1, 2, 3, 4])
    );
    assert_eq!(features[1]["properties"]["osm_id"], 12);
    assert_eq!(result["properties"]["simplified"], false);
}

#[test]
fn place_lookup_preserves_ambiguity_and_labels_approximate_centers() {
    let options = options(&args(&[
        "lookup-place",
        "--name",
        "Canal",
        "--bbox",
        "0,0,1,1",
    ]))
    .unwrap();
    let source = json!({"elements":[
        {"type":"node","id":1,"lon":0.2,"lat":0.3,"tags":{"name":"Canal Hall"}},
        {"type":"way","id":2,"center":{"lon":0.4,"lat":0.5},
         "tags":{"name":"Canal Hall"}},
        {"type":"node","id":3,"lon":1.2,"lat":0.3,"tags":{"name":"Canal Hall"}}
    ]});
    let result = features::convert(&source, &options).unwrap();
    assert_eq!(result["features"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["features"][0]["properties"]["position_kind"],
        "source_node"
    );
    assert_eq!(
        result["features"][1]["properties"]["position_kind"],
        "bounding_box_center"
    );
}

#[test]
fn addresses_use_explicit_house_number_and_street_fields() {
    let options = options(&args(&[
        "lookup-place",
        "--street",
        "Canal",
        "--number",
        "12A",
        "--bbox",
        "0,0,1,1",
    ]))
    .unwrap();
    let source = json!({"elements":[
        {"type":"node","id":1,"lon":0.2,"lat":0.3,
         "tags":{"addr:housenumber":"12A","addr:street":"Canal Road"}},
        {"type":"node","id":2,"lon":0.2,"lat":0.3,
         "tags":{"addr:housenumber":"12","addr:street":"Canal Road"}}
    ]});
    let result = features::convert(&source, &options).unwrap();
    assert_eq!(result["features"].as_array().unwrap().len(), 1);
    assert_eq!(result["features"][0]["properties"]["osm_id"], 1);
}

#[test]
fn bounded_queries_escape_regex_and_string_metacharacters() {
    let options = options(&args(&[
        "fetch-street",
        "--name",
        "Canal [West].* \"Road\"",
        "--bbox",
        "-1,0,0,1",
    ]))
    .unwrap();
    let query = query(&options);
    assert!(query.contains("[highway]"));
    assert_eq!(query.matches("(0,-1,1,0)").count(), 3);
    assert!(query.contains(r"\\[West\\]\\.\\*"));
    assert!(query.contains(r#"\"Road\""#));
    assert!(query.ends_with("out geom;"));
}

#[test]
fn invalid_crops_incomplete_sources_and_ambiguous_arguments_fail() {
    for crop in ["0,0,0,1", "0,0,1", "0,0,NaN,1", "0,0,3,3", "181,0,182,1"] {
        assert!(clip::Bounds::parse(crop).is_err(), "{crop}");
    }
    for values in [
        vec!["fetch-street", "--name", "Canal"],
        vec!["lookup-place", "--street", "Canal", "--bbox", "0,0,1,1"],
        vec![
            "lookup-place",
            "--name",
            "Canal",
            "--street",
            "Canal",
            "--number",
            "1",
            "--bbox",
            "0,0,1,1",
        ],
    ] {
        assert!(options(&args(&values)).is_err());
    }
    assert!(features::convert(
        &json!({"remark":"runtime error: timed out", "elements":[]}),
        &street_options()
    )
    .is_err());
    assert!(features::convert(
        &json!({"elements":[{"type":"way","id":1,
        "tags":{"name":"Canal Road","highway":"primary"}}]}),
        &street_options()
    )
    .is_err());
}

#[test]
fn source_response_copyright_is_retained_without_assuming_a_license_or_provider() {
    let source = json!({"osm3s":{"copyright":"Fixture rights holder"},"elements":[]});
    let output = features::convert(&source, &street_options()).unwrap();
    assert_eq!(output["properties"]["attribution"], "Fixture rights holder");
    assert!(output["properties"]["license"].is_null());
}
