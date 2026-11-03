//! Select only the requested features and retain their source identity.
use super::{clip, Options, Selection};
use clip::Point;
use serde_json::{json, Value};

fn fragment_matches(value: &str, query: &str) -> bool {
    value
        .as_bytes()
        .windows(query.len())
        .any(|part| part.eq_ignore_ascii_case(query.as_bytes()))
}

pub(super) fn matches(tags: &Value, selection: &Selection) -> bool {
    match selection {
        Selection::Name(name) => ["name", "name:fr", "alt_name"].iter().any(|key| {
            tags[*key]
                .as_str()
                .is_some_and(|value| fragment_matches(value, name))
        }),
        Selection::Address { street, number } => {
            tags["addr:housenumber"].as_str() == Some(number)
                && tags["addr:street"]
                    .as_str()
                    .is_some_and(|value| fragment_matches(value, street))
        }
    }
}

fn coordinate(value: &Value) -> Result<[f64; 2], String> {
    let lon = value["lon"].as_f64().ok_or("missing numeric longitude")?;
    let lat = value["lat"].as_f64().ok_or("missing numeric latitude")?;
    if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
        return Err("invalid WGS84 coordinate in source data".into());
    }
    Ok([lon, lat])
}

pub(super) fn convert(source: &Value, options: &Options) -> Result<Value, String> {
    let metadata = &source["opencity_source_metadata"];
    if let Some(remark) = source.get("remark") {
        return Err(format!("Overpass query did not complete cleanly: {remark}"));
    }
    let elements = source["elements"]
        .as_array()
        .ok_or("Overpass response needs an elements array")?;
    let mut features = Vec::new();
    let mut matched_ways = 0;
    let mut input_vertices = 0;
    for element in elements {
        let kind = element["type"]
            .as_str()
            .ok_or("Source element needs a type")?;
        if !matches(&element["tags"], &options.selection) {
            continue;
        }
        let id = element["id"]
            .as_u64()
            .ok_or("Source element needs an unsigned id")?;
        let mut properties = json!({
            "osm_type": kind, "osm_id": id, "tags": element["tags"]
        });
        if let Some(template) = metadata["object_url_template"].as_str() {
            properties["source_url"] = json!(template
                .replace("{type}", kind)
                .replace("{id}", &id.to_string()));
        } else if let Some(url) = element.get("source_url") {
            properties["source_url"] = url.clone();
        }
        if options.street {
            if kind != "way" || !element["tags"]["highway"].is_string() {
                continue;
            }
            let geometry = element["geometry"]
                .as_array()
                .ok_or("matching street lacks geometry; the source query must use 'out geom'")?;
            if geometry.len() < 2 {
                return Err("matching street needs at least two source vertices".into());
            }
            let points = geometry
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    Ok(Point {
                        xy: coordinate(value)?,
                        node: element["nodes"][index].as_u64(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            matched_ways += 1;
            input_vertices += points.len();
            for (index, part) in clip::line(&points, options.bounds).iter().enumerate() {
                properties["part"] = json!(index);
                properties["source_node_ids"] =
                    json!(part.iter().map(|p| p.node).collect::<Vec<_>>());
                features.push(json!({
                    "type": "Feature", "properties": properties,
                    "geometry": { "type": "LineString", "coordinates":
                        part.iter().map(|p| p.coordinates()).collect::<Vec<_>>() }
                }));
            }
        } else {
            if !matches!(kind, "node" | "way" | "relation") {
                continue;
            }
            let xy = coordinate(if kind == "node" {
                element
            } else {
                &element["center"]
            })?;
            if !options.bounds.contains(xy) {
                continue;
            }
            properties["position_kind"] = json!(if kind == "node" {
                "source_node"
            } else {
                "bounding_box_center"
            });
            features.push(json!({
                "type": "Feature", "properties": properties,
                "geometry": { "type": "Point", "coordinates": xy }
            }));
        }
    }
    let vertices: usize = features
        .iter()
        .filter_map(|feature| {
            feature["geometry"]["coordinates"]
                .as_array()
                .filter(|_| options.street)
                .map(Vec::len)
        })
        .sum();
    Ok(json!({
        "type": "FeatureCollection", "bbox": options.bounds.0, "features": features,
        "properties": {
            "attribution": metadata.get("attribution").unwrap_or(&source["osm3s"]["copyright"]),
            "license": metadata["license"],
            "source_metadata": metadata,
            "coordinate_system": "WGS84 longitude,latitude", "simplified": false,
            "source": source["opencity_source"],
            "source_timestamp": source["osm3s"]["timestamp_osm_base"],
            "matched_ways": matched_ways, "source_vertices": input_vertices,
            "output_vertices": vertices,
            "note": "Crop only; selection, simplification and map placement belong to the agent."
        }
    }))
}
