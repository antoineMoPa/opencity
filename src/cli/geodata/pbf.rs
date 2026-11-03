//! Local, selective PBF input. The existing converter owns crop semantics.
use super::{features, Options, Selection, MAX_BYTES};
use osmpbfreader::{OsmId, OsmObj, OsmPbfReader, Tags};
use protobuf::Message;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

fn tags_json(tags: &Tags) -> Value {
    Value::Object(
        tags.iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect(),
    )
}

fn selected(object: &OsmObj, options: &Options) -> bool {
    if options.street && (!object.is_way() || !object.tags().contains_key("highway")) {
        return false;
    }
    let tags = object.tags();
    let keys: &[&str] = match options.selection {
        Selection::Name(_) => &["name", "name:fr", "alt_name"],
        Selection::Address { .. } => &["addr:street"],
    };
    if !keys.iter().any(|key| tags.contains_key(*key)) {
        return false;
    }
    features::matches(&tags_json(tags), &options.selection)
}

// The upstream reader assumes complete blob frames. Reject truncated/oversized
// framing before decoding, and reject unsupported PBF payloads rather than returning no matches.
fn validate_container(file: &mut File) -> Result<(), String> {
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let mut at = 0;
    while at < length {
        let mut size = [0; 4];
        file.read_exact(&mut size)
            .map_err(|_| "truncated PBF blob header length")?;
        let size = u32::from_be_bytes(size) as usize;
        if size == 0 || size > 64 * 1024 {
            return Err("invalid PBF blob header size".into());
        }
        let mut bytes = vec![0; size];
        file.read_exact(&mut bytes)
            .map_err(|_| "truncated PBF blob header")?;
        let header = osmpbfreader::fileformat::BlobHeader::parse_from_bytes(&bytes)
            .map_err(|e| format!("invalid PBF header: {e}"))?;
        let expected = if at == 0 { "OSMHeader" } else { "OSMData" };
        if header.type_() != expected {
            return Err(format!("expected PBF {expected}, found {}", header.type_()));
        }
        let payload = header.datasize();
        if payload <= 0 || payload > 32 * 1024 * 1024 {
            return Err("invalid PBF blob payload size".into());
        }
        at += 4 + size as u64 + payload as u64;
        if at > length {
            return Err("truncated PBF blob payload".into());
        }
        file.seek(SeekFrom::Current(payload as i64))
            .map_err(|e| e.to_string())?;
    }
    if at == 0 {
        return Err("empty PBF file".into());
    }
    file.rewind().map_err(|e| e.to_string())?;
    Ok(())
}

fn coordinates(
    id: OsmId,
    objects: &BTreeMap<OsmId, OsmObj>,
    seen: &mut BTreeSet<OsmId>,
    out: &mut Vec<[f64; 2]>,
) -> Result<(), String> {
    if !seen.insert(id) {
        return Ok(());
    }
    let object = objects.get(&id).ok_or_else(|| {
        format!("local extract is missing dependency {id:?}; use a complete extract")
    })?;
    match object {
        OsmObj::Node(node) => out.push([node.lon(), node.lat()]),
        OsmObj::Way(way) => {
            for node in &way.nodes {
                coordinates((*node).into(), objects, seen, out)?;
            }
        }
        OsmObj::Relation(relation) => {
            for member in &relation.refs {
                coordinates(member.member, objects, seen, out)?;
            }
        }
    }
    Ok(())
}

fn element(
    object: &OsmObj,
    objects: &BTreeMap<OsmId, OsmObj>,
    street: bool,
) -> Result<Value, String> {
    let tags = tags_json(object.tags());
    if let OsmObj::Node(node) = object {
        return Ok(
            json!({"type":"node","id":node.id.0,"tags":tags,"lon":node.lon(),"lat":node.lat()}),
        );
    }
    if street {
        let way = object.way().ok_or("street must be a way")?;
        let mut points = Vec::new();
        for id in &way.nodes {
            let node = objects
                .get(&(*id).into())
                .and_then(OsmObj::node)
                .ok_or_else(|| {
                    format!(
                        "way {} is missing node {}; local extract is incomplete",
                        way.id.0, id.0
                    )
                })?;
            points.push(json!({"lon":node.lon(),"lat":node.lat()}));
        }
        return Ok(json!({
            "type":"way",
            "id":way.id.0,
            "tags":tags,
            "nodes":way.nodes.iter().map(|id|id.0).collect::<Vec<_>>(),
            "geometry":points
        }));
    }
    let mut points = Vec::new();
    coordinates(object.id(), objects, &mut BTreeSet::new(), &mut points)?;
    if points.is_empty() {
        return Err(format!(
            "PBF object {:?} has no position in the local extract",
            object.id()
        ));
    }
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        bounds[0] = bounds[0].min(p[0]);
        bounds[1] = bounds[1].min(p[1]);
        bounds[2] = bounds[2].max(p[0]);
        bounds[3] = bounds[3].max(p[1]);
    }
    let (kind, id) = match object {
        OsmObj::Way(w) => ("way", w.id.0),
        OsmObj::Relation(r) => ("relation", r.id.0),
        _ => unreachable!(),
    };
    Ok(json!({
        "type":kind,
        "id":id,
        "tags":tags,
        "center":{
            "lon":(bounds[0]+bounds[2])/2.,
            "lat":(bounds[1]+bounds[3])/2.
        }
    }))
}

pub(super) fn read(path: &Path, options: &Options) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = file.metadata().map_err(|e| e.to_string())?.len();
    validate_container(&mut file)?;
    let mut reader = OsmPbfReader::new(file);
    // Keep only named matches and the nodes/member objects needed to resolve them.
    // No city-wide object index or automatic road import is constructed.
    let objects = reader
        .get_objs_and_deps(|object| selected(object, options))
        .map_err(|e| format!("could not decode local PBF: {e}"))?;
    let mut elements = Vec::new();
    for object in objects.values().filter(|object| selected(object, options)) {
        let value = element(object, &objects, options.street)?;
        // Nodes can be excluded immediately. The existing converter clips full
        // way geometry and tests place centers using the same rules as JSON input.
        if object.is_node()
            && !options.bounds.contains([
                value["lon"].as_f64().unwrap(),
                value["lat"].as_f64().unwrap(),
            ])
        {
            continue;
        }
        if options.street {
            let points: Vec<_> = value["geometry"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, p)| super::clip::Point {
                    xy: [p["lon"].as_f64().unwrap(), p["lat"].as_f64().unwrap()],
                    node: value["nodes"][i].as_u64(),
                })
                .collect();
            if super::clip::line(&points, options.bounds).is_empty() {
                continue;
            }
        } else if !object.is_node()
            && !options.bounds.contains([
                value["center"]["lon"].as_f64().unwrap(),
                value["center"]["lat"].as_f64().unwrap(),
            ])
        {
            continue;
        }
        elements.push(value);
    }
    let source = json!({
        "version":0.6,
        "generator":"OpenCity local PBF reader",
        "elements":elements,
        "opencity_source":{
            "kind":"local_pbf",
            "path":path.display().to_string(),
            "bytes":bytes,
            "snapshot_timestamp":null,
            "note": concat!(
                "Selective local extract read; no network. ",
                "Extract file modification time is not a source snapshot timestamp."
            )
        }
    });
    let raw = serde_json::to_vec(&source).map_err(|e| e.to_string())?;
    if raw.len() > MAX_BYTES {
        return Err("selected PBF response exceeds 16 MiB; narrow the name/crop".into());
    }
    Ok(raw)
}
