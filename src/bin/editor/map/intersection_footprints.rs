//! Reserve turn room without expanding adjacent junctions into each other.
use super::*;
pub(super) fn assign(program: &Program, projection: &Projection, junctions: &mut [Intersection]) {
    let widths: HashMap<_, _> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Road(road) => Some((
                road.name.as_str(),
                road.elements
                    .iter()
                    .map(|e| element_world_width(e.kind))
                    .sum::<f32>()
                    / 2.,
            )),
            _ => None,
        })
        .collect();
    let mut owners = HashMap::new();
    for (index, junction) in junctions.iter().enumerate() {
        owners.insert(junction.point_ref.clone(), index);
        for port in &junction.ports {
            owners.insert(port.point_ref.clone(), index);
        }
    }
    let mut base: Vec<f32> = junctions
        .iter()
        .map(|j| {
            std::iter::once(j.road_name.as_str())
                .chain(j.arm_roads.iter().map(|(_, name)| name.as_str()))
                .filter_map(|name| widths.get(name))
                .copied()
                .fold(0., f32::max)
        })
        .collect();
    for segment in &projection.segments {
        for reference in [&segment.start_ref, &segment.end_ref] {
            if let Some(index) = reference.as_ref().and_then(|r| owners.get(r)) {
                base[*index] = base[*index].max(
                    widths
                        .get(segment.road_name.as_str())
                        .copied()
                        .unwrap_or(0.),
                );
            }
        }
    }
    let factor = opencity::network::default_four_way_footprint_factor();
    let desired: Vec<_> = junctions
        .iter()
        .zip(&base)
        .map(|(j, half)| {
            if !j.ports.is_empty() {
                j.ports
                    .iter()
                    .map(|p| {
                        (p.position.x - j.position.x)
                            .abs()
                            .max((p.position.y - j.position.y).abs())
                    })
                    .fold(*half, f32::max)
            } else if j.kind == IntersectionKind::FourWay {
                half * factor
            } else {
                *half
            }
        })
        .collect();
    let mut extents = desired.clone();
    for segment in &projection.segments {
        let Some(a) = segment
            .start_ref
            .as_ref()
            .and_then(|name| owners.get(name))
            .copied()
        else {
            continue;
        };
        let Some(b) = segment
            .end_ref
            .as_ref()
            .and_then(|name| owners.get(name))
            .copied()
        else {
            continue;
        };
        if a == b {
            continue;
        }
        let pa = junctions[a].position;
        let pb = junctions[b].position;
        let distance = (pa.x - pb.x).abs().max((pa.y - pb.y).abs());
        let extra_a = desired[a] - base[a];
        let extra_b = desired[b] - base[b];
        let extra = extra_a + extra_b;
        if extra <= 0. {
            continue;
        }
        let room = (distance - base[a] - base[b]).max(0.) * 0.9;
        let share = (room / extra).clamp(0., 1.);
        for (index, addition) in [(a, extra_a), (b, extra_b)] {
            if junctions[index].ports.is_empty() {
                extents[index] = extents[index].min(base[index] + addition * share);
            }
        }
    }
    for (junction, extent) in junctions.iter_mut().zip(extents) {
        junction.footprint_half = extent;
    }
}
