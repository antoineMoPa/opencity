//! Geographic clipping preserves vertices; it never simplifies a street.
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds(pub [f64; 4]);

impl Bounds {
    pub fn parse(text: &str) -> Result<Self, String> {
        let values: Vec<_> = text
            .split(',')
            .map(str::parse::<f64>)
            .collect::<Result<_, _>>()
            .map_err(|_| "--bbox expects west,south,east,north in decimal degrees")?;
        let [west, south, east, north] = values.as_slice() else {
            return Err("--bbox needs exactly four coordinates: west,south,east,north".into());
        };
        if !values.iter().all(|v| v.is_finite())
            || *west < -180.0
            || *east > 180.0
            || *south < -90.0
            || *north > 90.0
            || west >= east
            || south >= north
        {
            return Err("--bbox must be a finite, ordered WGS84 rectangle".into());
        }
        if (east - west) * (north - south) > 1.0 {
            return Err(
                "--bbox is too large; use a local crop of at most one square degree".into(),
            );
        }
        Ok(Self([*west, *south, *east, *north]))
    }

    pub fn contains(self, p: [f64; 2]) -> bool {
        let [w, s, e, n] = self.0;
        p[0] >= w && p[0] <= e && p[1] >= s && p[1] <= n
    }

    pub fn overpass(self) -> String {
        let [w, s, e, n] = self.0;
        format!("({s},{w},{n},{e})")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Point {
    pub xy: [f64; 2],
    pub node: Option<u64>,
}

impl Point {
    fn between(self, end: Self, t: f64) -> Self {
        if t == 0.0 {
            self
        } else if t == 1.0 {
            end
        } else {
            Self {
                xy: std::array::from_fn(|i| self.xy[i] + (end.xy[i] - self.xy[i]) * t),
                node: None,
            }
        }
    }

    pub fn coordinates(self) -> Value {
        json!(self.xy)
    }
}

fn segment(a: Point, b: Point, bounds: Bounds) -> Option<[Point; 2]> {
    let [w, s, e, n] = bounds.0;
    let dx = b.xy[0] - a.xy[0];
    let dy = b.xy[1] - a.xy[1];
    let mut start: f64 = 0.0;
    let mut end: f64 = 1.0;
    for (p, q) in [
        (-dx, a.xy[0] - w),
        (dx, e - a.xy[0]),
        (-dy, a.xy[1] - s),
        (dy, n - a.xy[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            start = start.max(q / p);
        } else {
            end = end.min(q / p);
        }
    }
    (start <= end).then(|| {
        let mut points = [a.between(b, start), a.between(b, end)];
        for point in &mut points {
            if point.node.is_none() {
                // Keep computed intersections inside the crop despite floating-point rounding.
                point.xy = [point.xy[0].clamp(w, e), point.xy[1].clamp(s, n)];
            }
        }
        points
    })
}

pub(super) fn line(points: &[Point], bounds: Bounds) -> Vec<Vec<Point>> {
    let mut parts = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    for pair in points.windows(2) {
        if let Some([a, b]) = segment(pair[0], pair[1], bounds) {
            if current.last().is_some_and(|last| last.xy != a.xy) {
                parts.push(std::mem::take(&mut current));
            }
            if current.is_empty() {
                current.push(a);
            }
            current.push(b);
            if b.xy != pair[1].xy {
                parts.push(std::mem::take(&mut current));
            }
        } else if !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}
