//! Calibration diagnostics and full-detail bulk coordinate output.
use super::mapping::{pair, Mapping};
use serde_json::{json, Value};

impl Mapping {
    pub(super) fn errors(&self, key: &str) -> Result<Value, String> {
        let mut rows = Vec::new();
        let mut sum = 0.;
        let mut max: f64 = 0.;
        for c in self.source[key]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let actual = pair(&c["world"], "control world")?;
            let predicted = self.to_world(pair(&c["lonlat"], "control lonlat")?)?;
            let error = (actual[0] - predicted[0]).hypot(actual[1] - predicted[1]);
            sum += error * error;
            max = max.max(error);
            rows.push(json!({
                "id": c["id"],
                "expected_world": actual,
                "mapped_world": predicted,
                "error_world": error
            }));
        }
        Ok(json!({
            "count":rows.len(),
            "rms_world":if rows.is_empty(){
                None
            }else{
                Some((sum/rows.len() as f64).sqrt())
            },
            "max_world":max,
            "controls":rows
        }))
    }

    pub(super) fn report(&self) -> Result<Value, String> {
        let [left, top, right, bottom] = self.bounds;
        let mut boundary = vec![[left, top], [right, top], [left, bottom], [right, bottom]];
        if let Some([threshold, _]) = self.north {
            if threshold > top && threshold < bottom {
                boundary.extend([[left, threshold], [right, threshold]]);
            }
        }
        let mut geographic_bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for point in boundary {
            let p = self.to_lonlat(point)?;
            geographic_bbox[0] = geographic_bbox[0].min(p[0]);
            geographic_bbox[1] = geographic_bbox[1].min(p[1]);
            geographic_bbox[2] = geographic_bbox[2].max(p[0]);
            geographic_bbox[3] = geographic_bbox[3].max(p[1]);
        }
        Ok(json!({
            "type":"OpenCityCalibrationReport",
            "calibration_id":self.source["id"],
            "projection":"local_web_mercator",
            "origin_mercator_metres":self.origin,
            "mercator_to_world_affine":self.affine,
            "north_x_adjustment":self.north,
            "world_bounds":self.bounds,
            "geographic_bbox":geographic_bbox,
            "fit":self.errors("fit_controls")?,
            "independent_checks":self.errors("check_controls")?,
            "quality":self.source["quality"],
            "note": concat!(
                "World XY is the authored game-map coordinate frame, not physical metres. ",
                "Check errors describe registration, not shoreline accuracy."
            )
        }))
    }

    pub(super) fn map_geojson(&self, source: Value) -> Result<Value, String> {
        if source["type"] != "FeatureCollection" {
            return Err("input must be a GeoJSON FeatureCollection".into());
        }
        let features = source["features"]
            .as_array()
            .ok_or("GeoJSON features must be an array")?;
        let mut mapped = Vec::new();
        let mut vertices = 0;
        let mut outside = 0;
        for (index, feature) in features.iter().enumerate() {
            if feature["type"] != "Feature" {
                return Err("collection contains a non-Feature".into());
            }
            let geometry = &feature["geometry"];
            let mut point = |v: &Value| -> Result<Value, String> {
                let world = self.to_world(pair(v, "GeoJSON position")?)?;
                vertices += 1;
                if !self.inside(world) {
                    outside += 1;
                }
                Ok(json!(world))
            };
            let coordinates = match geometry["type"].as_str() {
                Some("Point") => point(&geometry["coordinates"])?,
                Some("LineString") => {
                    let points = geometry["coordinates"]
                        .as_array()
                        .filter(|p| p.len() >= 2)
                        .ok_or("LineString needs at least two positions")?;
                    Value::Array(
                        points
                            .iter()
                            .map(&mut point)
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                }
                _ => {
                    return Err(
                        "only Point and LineString geometry is supported; input is unchanged"
                            .into(),
                    )
                }
            };
            mapped.push(json!({
                "source_feature_index":index,
                "world_geometry":{
                    "type":geometry["type"],
                    "coordinates":coordinates
                }
            }));
        }
        Ok(json!({
            "type":"OpenCityMappedFeatures",
            "calibration_id":self.source["id"],
            "coordinate_system":"OpenCity authored world XY",
            "simplified":false,
            "joined":false,
            "source_geojson":source,
            "mapped_features":mapped,
            "vertex_count":vertices,
            "vertices_outside_map":outside,
            "quality":self.source["quality"],
            "note": concat!(
                "Source GeoJSON and vertex ordering retained. ",
                "World geometry is not geographic GeoJSON. ",
                "Heights, simplification and placement remain agent decisions."
            )
        }))
    }
}
