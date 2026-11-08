//! Guard the exposed side of roads beside an authored excavation.
use super::*;

pub(super) fn cut_drop(
    wall: &Wall,
    segment: &Segment,
    map: &crate::heightmap::ElevationMap,
) -> bool {
    let middle = mix(wall.start, wall.end, 0.5).point;
    let center = (0..=32)
        .map(|i| segment.point_at(i as f32 / 32.))
        .min_by(|a, b| {
            let distance = |p: WorldPoint| (p.x - middle.x).powi(2) + (p.y - middle.y).powi(2);
            distance(*a).total_cmp(&distance(*b))
        })
        .unwrap();
    let d = [
        wall.end.point.x - wall.start.point.x,
        wall.end.point.y - wall.start.point.y,
    ];
    let length = d[0].hypot(d[1]);
    if length < 1e-8 {
        return false;
    }
    let mut outward = [-d[1] / length, d[0] / length];
    if outward[0] * (middle.x - center.x) + outward[1] * (middle.y - center.y) < 0. {
        outward = outward.map(|v| -v);
    }
    // A shoulder strip can separate the pavement from the excavation rim.
    [wall.start, wall.end].iter().all(|v| {
        [0.04, 0.1, 0.2].iter().any(|reach| {
            let p = [
                v.point.x + outward[0] * reach,
                v.point.y + outward[1] * reach,
            ];
            map.cut_at(p).is_some()
                && map
                    .height_at(p)
                    .is_some_and(|z| v.height - z > SAFETY_CLEARANCE)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excavation_guards_only_the_exposed_roadside() {
        let program = crate::parse(
            "elevation_map(\"a\",-2,-2,4,4,0,0.1);
            terrain_cut(0,-1.5,0.1,1.5,0.1,1.5,0.5,-1.5,0.5);
            road local: carlane carlane; end r=local(-1,0,1,0);grade(r,0.102,0.102);",
        )
        .unwrap();
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1024, 1024);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![255; 1024 * 1024])
                .unwrap();
        }
        let map = crate::heightmap::ElevationMap::from_png(
            crate::heightmap::ElevationMapSpec::from_program(&program)
                .unwrap()
                .unwrap(),
            &bytes,
        )
        .unwrap();
        let projection = map::project(&program);
        let mut geometry = RoadGeometry::compile(&program, &projection);
        geometry.configure_safety(&program, &projection, Some(&map));
        assert!(!geometry.safety_walls.is_empty());
        assert!(geometry
            .safety_walls
            .iter()
            .all(|w| w.start.point.y > 0. && w.end.point.y > 0.));
    }
}
