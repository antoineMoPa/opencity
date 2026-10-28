//! Convert native painter-style junction bands into disjoint physical surfaces.
//! Approach strips belong to the editor's template preview; ordinary roads own
//! pavement outside the junction footprint in a game. Within that footprint,
//! later surface bands replace earlier fills instead of leaving coplanar faces.
use crate::network::{BandKind, Material, Point, Resolved};

pub struct Patch {
    pub points: Vec<Point>,
    pub material: Material,
    pub kind: BandKind,
}
fn area(poly: &[Point]) -> f64 {
    poly.iter()
        .zip(poly.iter().cycle().skip(1))
        .take(poly.len())
        .map(|(a, b)| a.x as f64 * b.y as f64 - b.x as f64 * a.y as f64)
        .sum::<f64>()
        * 0.5
}
fn bounds(poly: &[Point]) -> [f32; 4] {
    poly.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, p| [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)],
    )
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
/// Partition a convex polygon along an oriented line; both pieces retain Z.
fn split(poly: &[Point], a: Point, b: Point, sign: f64) -> (Vec<Point>, Vec<Point>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    let side = |p: Point| {
        sign * ((b.x as f64 - a.x as f64) * (p.y as f64 - a.y as f64)
            - (b.y as f64 - a.y as f64) * (p.x as f64 - a.x as f64))
    };
    for (&p, &q) in poly
        .iter()
        .zip(poly.iter().cycle().skip(1))
        .take(poly.len())
    {
        let d = side(p);
        let e = side(q);
        if d >= 0. {
            inside.push(p);
        }
        if d <= 0. {
            outside.push(p);
        }
        if (d < 0. && e > 0.) || (d > 0. && e < 0.) {
            let t = d / (d - e);
            let point = Point {
                x: (p.x as f64 + (q.x as f64 - p.x as f64) * t) as f32,
                y: (p.y as f64 + (q.y as f64 - p.y as f64) * t) as f32,
                z: (p.z as f64 + (q.z as f64 - p.z as f64) * t) as f32,
            };
            inside.push(point);
            outside.push(point);
        }
    }
    (inside, outside)
}
fn intersection(mut poly: Vec<Point>, clip: &[Point]) -> Vec<Point> {
    let sign = area(clip).signum();
    for (&a, &b) in clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .take(clip.len())
    {
        if a.x == b.x && a.y == b.y {
            continue;
        }
        poly = split(&poly, a, b, sign).0;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}
fn difference(poly: Vec<Point>, clip: &[Point]) -> Vec<Vec<Point>> {
    if !overlaps(bounds(&poly), bounds(clip)) {
        return vec![poly];
    }
    // Disjoint polygons can share bounding boxes. Keep the original face
    // instead of fragmenting it along every unrelated neighboring band.
    if area(&intersection(poly.clone(), clip)).abs() < 1e-14 {
        return vec![poly];
    }
    let sign = area(clip).signum();
    let mut remaining = poly;
    let mut pieces = Vec::new();
    for (&a, &b) in clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .take(clip.len())
    {
        if a.x == b.x && a.y == b.y {
            continue;
        }
        let (inside, outside) = split(&remaining, a, b, sign);
        if area(&outside).abs() > 1e-14 {
            pieces.push(outside);
        }
        remaining = inside;
        if area(&remaining).abs() < 1e-14 {
            break;
        }
    }
    pieces
}
/// `half` selects native-junction composition. Explicit authored components
/// keep their original geometry, including intentional surfaces at other decks.
pub fn patches(geometry: &Resolved, half: Option<f32>) -> Vec<Patch> {
    let boundary = half.map(|h| {
        [
            Point {
                x: -h,
                y: -h,
                z: 0.,
            },
            Point { x: h, y: -h, z: 0. },
            Point { x: h, y: h, z: 0. },
            Point { x: -h, y: h, z: 0. },
        ]
    });
    let mut patches = Vec::new();
    for band in &geometry.bands {
        for quad in geometry.band_quads(band) {
            let points = if let Some(boundary) = &boundary {
                intersection(quad.to_vec(), boundary)
            } else {
                quad.to_vec()
            };
            if half.is_none() || area(&points).abs() > 1e-14 {
                patches.push(Patch {
                    points,
                    material: band.material,
                    kind: band.kind,
                });
            }
        }
    }
    if half.is_none() {
        return patches;
    }
    let patch_bounds: Vec<_> = patches.iter().map(|p| bounds(&p.points)).collect();
    let mut result = Vec::new();
    for (i, patch) in patches.iter().enumerate() {
        let mut pieces = vec![patch.points.clone()];
        // Markings remain shallow decals over the physical supporting surface.
        if patch.kind == BandKind::Surface {
            for next in patches[i + 1..]
                .iter()
                .enumerate()
                .filter(|(relative, p)| {
                    p.kind == BandKind::Surface
                        && overlaps(patch_bounds[i], patch_bounds[i + 1 + relative])
                })
                .map(|(_, p)| p)
            {
                pieces = pieces
                    .into_iter()
                    .flat_map(|piece| difference(piece, &next.points))
                    .collect();
                if pieces.is_empty() {
                    break;
                }
            }
        }
        result.extend(pieces.into_iter().map(|points| Patch {
            points,
            material: patch.material,
            kind: patch.kind,
        }));
    }
    result
}

/// Exterior of the physical junction footprint. Internal band seams and paint
/// overlays are excluded; callers remove the open road mouths.
pub fn boundary(geometry: &Resolved, half: f32) -> Vec<[Point; 2]> {
    let clip = [
        Point {
            x: -half,
            y: -half,
            z: 0.,
        },
        Point {
            x: half,
            y: -half,
            z: 0.,
        },
        Point {
            x: half,
            y: half,
            z: 0.,
        },
        Point {
            x: -half,
            y: half,
            z: 0.,
        },
    ];
    let polygons: Vec<_> = geometry
        .bands
        .iter()
        .filter(|b| b.kind == BandKind::Surface)
        .flat_map(|b| geometry.band_quads(b))
        .map(|q| intersection(q.to_vec(), &clip))
        .filter(|p| area(p).abs() > 1e-14)
        .collect();
    let polygon_bounds: Vec<_> = polygons.iter().map(|p| bounds(p)).collect();
    let covered = |point: Point| {
        polygons.iter().zip(&polygon_bounds).any(|(poly, b)| {
            if point.x < b[0] || point.y < b[1] || point.x > b[2] || point.y > b[3] {
                return false;
            }
            let mut inside = false;
            for (a, b) in poly
                .iter()
                .zip(poly.iter().cycle().skip(1))
                .take(poly.len())
            {
                if (a.y > point.y) != (b.y > point.y)
                    && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
                {
                    inside = !inside;
                }
            }
            inside
        })
    };
    let mut result: Vec<[Point; 2]> = Vec::new();
    for poly in &polygons {
        for (&a, &b) in poly
            .iter()
            .zip(poly.iter().cycle().skip(1))
            .take(poly.len())
        {
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length_squared = dx * dx + dy * dy;
            if length_squared < 1e-14 {
                continue;
            }
            let mut cuts = vec![0., 1.];
            for p in polygons.iter().flatten() {
                let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length_squared;
                if t > 0. && t < 1. && (p.x - a.x - dx * t).hypot(p.y - a.y - dy * t) < 1e-6 {
                    cuts.push(t);
                }
            }
            cuts.sort_by(f32::total_cmp);
            cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
            let point = |t: f32| Point {
                x: a.x + dx * t,
                y: a.y + dy * t,
                z: a.z + (b.z - a.z) * t,
            };
            let length = length_squared.sqrt();
            for pair in cuts.windows(2) {
                let middle = point((pair[0] + pair[1]) * 0.5);
                let probe = |sign: f32| Point {
                    x: middle.x - dy / length * 1e-6 * sign,
                    y: middle.y + dx / length * 1e-6 * sign,
                    z: middle.z,
                };
                if covered(probe(-1.)) == covered(probe(1.)) {
                    continue;
                }
                let edge = [point(pair[0]), point(pair[1])];
                let close = |a: Point, b: Point| (a.x - b.x).hypot(a.y - b.y) < 1e-6;
                if !result.iter().any(|p| {
                    (close(p[0], edge[0]) && close(p[1], edge[1]))
                        || (close(p[0], edge[1]) && close(p[1], edge[0]))
                }) {
                    result.push(edge);
                }
            }
        }
    }
    result
}
