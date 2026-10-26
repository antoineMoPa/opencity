//! Convert native painter-style junction bands into disjoint physical surfaces.
//! Approach strips belong to the editor's template preview; ordinary roads own
//! pavement outside the junction footprint in a game. Within that footprint,
//! later surface bands replace earlier fills instead of leaving coplanar faces.
use crate::network::{BandKind, Material, Point, Resolved};

pub(super) struct Patch {
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
pub(super) fn patches(geometry: &Resolved, half: Option<f32>) -> Vec<Patch> {
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
