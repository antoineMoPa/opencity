use super::*;

pub fn point_at_distance(points: &[Point], distance: f32) -> Option<Point> {
    if points.is_empty() || !distance.is_finite() || distance < 0. {
        return None;
    }
    let mut traversed = 0.;
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if distance == traversed + length {
            return Some(pair[1]);
        }
        if distance <= traversed + length && length > 0. {
            return Some(pair[0].lerp(pair[1], ((distance - traversed) / length).clamp(0.0, 1.0)));
        }
        traversed += length;
    }
    if (distance - traversed).abs() <= 0.001 {
        points.last().copied()
    } else {
        None
    }
}

pub fn slice_path(points: &[Point], start: f32, end: f32) -> Option<Vec<Point>> {
    if end <= start {
        return None;
    }
    let first = point_at_distance(points, start)?;
    let last = point_at_distance(points, end)?;
    let mut result = vec![first];
    let mut distance = 0.;
    for pair in points.windows(2) {
        distance += pair[0].distance(pair[1]);
        if distance > start && distance < end {
            result.push(pair[1]);
        }
    }
    result.push(last);
    Some(result)
}

pub(super) fn append_segment(
    points: &mut Vec<Point>,
    segment: &PathSegment,
    end: Point,
) -> Result<(), String> {
    let start = *points.last().ok_or("missing path start")?;
    match segment {
        PathSegment::Line(_) => points.push(end),
        PathSegment::Cubic { first, second, .. } => {
            for index in 1..=64 {
                let t = index as f32 / 64.;
                let a = 1. - t;
                let coordinate = |s: f32, c: f32, d: f32, e: f32| {
                    a * a * a * s + 3. * a * a * t * c + 3. * a * t * t * d + t * t * t * e
                };
                points.push(Point {
                    x: coordinate(start.x, first.x, second.x, end.x),
                    y: coordinate(start.y, first.y, second.y, end.y),
                    z: coordinate(start.z, first.z, second.z, end.z),
                });
            }
        }
        PathSegment::Arc {
            radius, clockwise, ..
        } => {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let chord = dx.hypot(dy);
            if !radius.is_finite()
                || *radius <= 0.
                || chord <= 0.000001
                || chord > 2. * radius + 0.00001
            {
                return Err("arc radius must be positive and span its chord".into());
            }
            let h = (radius * radius - chord * chord / 4.).max(0.).sqrt();
            let sign = if *clockwise { 1. } else { -1. };
            let cx = (start.x + end.x) / 2. - sign * dy / chord * h;
            let cy = (start.y + end.y) / 2. + sign * dx / chord * h;
            let angle = (start.y - cy).atan2(start.x - cx);
            let mut sweep = (end.y - cy).atan2(end.x - cx) - angle;
            if *clockwise && sweep < 0. {
                sweep += std::f32::consts::TAU;
            }
            if !clockwise && sweep > 0. {
                sweep -= std::f32::consts::TAU;
            }
            for index in 1..=64 {
                let t = index as f32 / 64.;
                let a = angle + sweep * t;
                points.push(Point {
                    x: cx + radius * a.cos(),
                    y: cy + radius * a.sin(),
                    z: start.z + (end.z - start.z) * t,
                });
            }
            *points.last_mut().unwrap() = end;
        }
    }
    if points
        .iter()
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
    {
        return Err("path geometry must remain finite".into());
    }
    Ok(())
}

/// Quads along a sampled path. Local +Y is south, so left is (dy, -dx).
pub fn band_quads(path: &[Point], width: f32, side: Side) -> Vec<[Point; 4]> {
    if !width.is_finite() || width <= 0. {
        return vec![];
    }
    let (low, high) = match side {
        Side::Center => (-width / 2., width / 2.),
        Side::Left => (0., width),
        Side::Right => (-width, 0.),
    };
    let edges: Vec<_> = path
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let before = path[index.saturating_sub(1)];
            let after = path[(index + 1).min(path.len() - 1)];
            let (dx, dy) = if path.len() >= 3 && index == 0 {
                (
                    -3. * point.x + 4. * path[1].x - path[2].x,
                    -3. * point.y + 4. * path[1].y - path[2].y,
                )
            } else if path.len() >= 3 && index + 1 == path.len() {
                (
                    3. * point.x - 4. * path[index - 1].x + path[index - 2].x,
                    3. * point.y - 4. * path[index - 1].y + path[index - 2].y,
                )
            } else {
                (after.x - before.x, after.y - before.y)
            };
            let angle = dy.atan2(dx);
            let offset = |d: f32| Point {
                x: point.x + angle.sin() * d,
                y: point.y - angle.cos() * d,
                z: point.z,
            };
            (offset(low), offset(high))
        })
        .collect();
    edges
        .windows(2)
        .map(|p| [p[0].0, p[1].0, p[1].1, p[0].1])
        .collect()
}
