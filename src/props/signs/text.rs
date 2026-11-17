//! Tessellate the bundled PT Sans outlines once per sign design, without an atlas.
use super::*;
use ttf_parser::{Face, OutlineBuilder};
fn font() -> &'static Face<'static> {
    static FONT: OnceLock<Face<'static>> = OnceLock::new();
    FONT.get_or_init(|| {
        Face::parse(
            crate::game_asset!(
                "../../../assets/fonts/pt-sans-bold.ttf",
                "opencity/assets/fonts/pt-sans-bold.ttf"
            ),
            0,
        )
        .expect("bundled sign font")
    })
}
pub(super) fn has_glyph(c: char) -> bool {
    font().glyph_index(c).is_some()
}
#[derive(Default)]
struct Outline {
    contours: Vec<Vec<[f64; 2]>>,
}
impl Outline {
    fn point(&mut self, x: f32, y: f32) {
        self.contours.last_mut().unwrap().push([x as f64, y as f64]);
    }
    fn last(&self) -> [f32; 2] {
        self.contours
            .last()
            .unwrap()
            .last()
            .unwrap()
            .map(|v| v as f32)
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.contours.push(Vec::new());
        self.point(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let a = self.last();
        for i in 1..=8 {
            let t = i as f32 / 8.;
            let u = 1. - t;
            self.point(
                u * u * a[0] + 2. * u * t * x1 + t * t * x,
                u * u * a[1] + 2. * u * t * y1 + t * t * y,
            );
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let a = self.last();
        for i in 1..=12 {
            let t = i as f32 / 12.;
            let u = 1. - t;
            self.point(
                u * u * u * a[0] + 3. * u * u * t * x1 + 3. * u * t * t * x2 + t * t * t * x,
                u * u * u * a[1] + 3. * u * u * t * y1 + 3. * u * t * t * y2 + t * t * t * y,
            );
        }
    }
    fn close(&mut self) {}
}
fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut yes = false;
    for (a, b) in ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
    {
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes;
        }
    }
    yes
}
pub(super) fn draw(
    mesh: &mut Mesh,
    label: &str,
    center: [f32; 2],
    height: f32,
    width: f32,
    y: f32,
    color: [f32; 3],
) {
    if label.is_empty() {
        return;
    }
    let font = font();
    let glyphs: Vec<_> = label.chars().filter_map(|c| font.glyph_index(c)).collect();
    let advance: f32 = glyphs
        .iter()
        .map(|g| font.glyph_hor_advance(*g).unwrap_or(0) as f32)
        .sum();
    let cap = font.capital_height().unwrap_or(700) as f32;
    let scale = (height / cap).min(width / advance.max(1.));
    let mut cursor = center[0] - advance * scale * 0.5;
    for glyph in glyphs {
        let mut outline = Outline::default();
        font.outline_glyph(glyph, &mut outline);
        outline.contours.retain(|r| r.len() >= 3);
        let depths: Vec<_> = outline
            .contours
            .iter()
            .enumerate()
            .map(|(i, r)| {
                outline
                    .contours
                    .iter()
                    .enumerate()
                    .filter(|(j, other)| i != *j && inside(r[0], other))
                    .count()
            })
            .collect();
        for (i, ring) in outline
            .contours
            .iter()
            .enumerate()
            .filter(|(i, _)| depths[*i] % 2 == 0)
        {
            let mut points = ring.clone();
            let mut holes = Vec::new();
            for (j, hole) in outline.contours.iter().enumerate() {
                if depths[j] == depths[i] + 1 && inside(hole[0], ring) {
                    holes.push(points.len());
                    points.extend(hole);
                }
            }
            let coords: Vec<_> = points.iter().flatten().copied().collect();
            for tri in earcutr::earcut(&coords, &holes, 2)
                .expect("font contours")
                .chunks_exact(3)
            {
                let p = std::array::from_fn(|k| {
                    let v = points[tri[k]];
                    [
                        (cursor + v[0] as f32 * scale) / UNITS,
                        y / UNITS,
                        (center[1] + (v[1] as f32 - cap * 0.5) * scale) / UNITS,
                    ]
                });
                mesh.triangle(p, color, SurfaceKind::Prop, [1., 0., 0.]);
            }
        }
        cursor += font.glyph_hor_advance(glyph).unwrap_or(0) as f32 * scale;
    }
}
