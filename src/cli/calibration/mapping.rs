//! Calibration fitting and reversible coordinate mapping.
use serde_json::Value;
use std::collections::HashSet;

pub(super) type Point = [f64; 2];
const RADIUS: f64 = 6_378_137.;

pub(super) fn pair(value: &Value, label: &str) -> Result<Point, String> {
    let values = value
        .as_array()
        .filter(|v| v.len() == 2)
        .ok_or_else(|| format!("{label} must contain exactly two finite numbers"))?;
    let mut out = [0.; 2];
    for i in 0..2 {
        out[i] = values[i]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{label} must contain exactly two finite numbers"))?;
    }
    Ok(out)
}

pub(super) fn mercator(lonlat: Point) -> Result<Point, String> {
    let [lon, lat] = lonlat;
    if !lon.is_finite()
        || !lat.is_finite()
        || !(-180. ..=180.).contains(&lon)
        || !(-85.05112878..=85.05112878).contains(&lat)
    {
        return Err("coordinate is outside the finite WGS84/Web Mercator domain".into());
    }
    Ok([
        RADIUS * lon.to_radians(),
        RADIUS
            * (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.)
                .tan()
                .ln(),
    ])
}

fn geographic(xy: Point) -> Result<Point, String> {
    let point = [
        (xy[0] / RADIUS).to_degrees(),
        (2. * (xy[1] / RADIUS).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees(),
    ];
    mercator(point)?;
    Ok(point)
}

#[derive(Debug)]
pub(super) struct Mapping {
    pub(super) origin: Point,
    pub(super) affine: [[f64; 3]; 2],
    // This preserves an existing authored-map correction; it is not a fit parameter.
    pub(super) north: Option<Point>, // [world Y threshold, added X per unit below threshold]
    pub(super) bounds: [f64; 4],
    pub(super) source: Value,
}

impl Mapping {
    pub(super) fn load(source: Value) -> Result<Self, String> {
        if source["schema"] != "opencity-georeference-v1" {
            return Err("unsupported calibration schema; expected opencity-georeference-v1".into());
        }
        if source["projection"] != "local_web_mercator" {
            return Err("unsupported calibration projection".into());
        }
        let origin = mercator(pair(&source["origin_lonlat"], "origin_lonlat")?)?;
        let raw = source["world_bounds"]
            .as_array()
            .filter(|b| b.len() == 4)
            .ok_or("world_bounds must be [min_x,min_y,max_x,max_y]")?;
        let mut bounds = [0.; 4];
        for i in 0..4 {
            bounds[i] = raw[i]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid world_bounds")?;
        }
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return Err("world_bounds must be finite and ordered".into());
        }
        let north = if source["north_x_adjustment"].is_null() {
            None
        } else {
            Some(pair(&source["north_x_adjustment"], "north_x_adjustment")?)
        };
        let controls = source["fit_controls"]
            .as_array()
            .filter(|a| a.len() >= 3)
            .ok_or("calibration needs at least three non-collinear fit_controls")?;
        let mut points = Vec::new();
        let mut ids = HashSet::new();
        for control in controls {
            let id = control["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("control needs an id")?;
            if !ids.insert(id) {
                return Err(format!("duplicate control id {id}"));
            }
            let projected = mercator(pair(&control["lonlat"], "control lonlat")?)?;
            let mut world = pair(&control["world"], "control world")?;
            if let Some([threshold, factor]) = north {
                world[0] -= factor * (threshold - world[1]).max(0.);
            }
            points.push(([projected[0] - origin[0], projected[1] - origin[1]], world));
        }
        // Centered covariance solves a general affine fit without huge absolute
        // Mercator ordinates or a poorly conditioned homogeneous normal matrix.
        let n = points.len() as f64;
        let mut mean = [0.; 4];
        for (p, q) in &points {
            for (i, v) in [p[0], p[1], q[0], q[1]].iter().enumerate() {
                mean[i] += v / n;
            }
        }
        let (mut xx, mut xy, mut yy) = (0., 0., 0.);
        let mut products = [[0.; 2]; 2];
        for (p, q) in &points {
            let (x, y) = (p[0] - mean[0], p[1] - mean[1]);
            xx += x * x;
            xy += x * y;
            yy += y * y;
            for k in 0..2 {
                products[k][0] += x * (q[k] - mean[k + 2]);
                products[k][1] += y * (q[k] - mean[k + 2]);
            }
        }
        let det = xx * yy - xy * xy;
        if !det.is_finite() || xx <= 0. || yy <= 0. || det <= xx * yy * 1e-12 {
            return Err("calibration controls are collinear or too poorly separated".into());
        }
        let mut affine = [[0.; 3]; 2];
        for k in 0..2 {
            let a = (products[k][0] * yy - products[k][1] * xy) / det;
            let b = (products[k][1] * xx - products[k][0] * xy) / det;
            affine[k] = [a, b, mean[k + 2] - a * mean[0] - b * mean[1]];
        }
        let d = affine[0][0] * affine[1][1] - affine[0][1] * affine[1][0];
        let scale = affine[0][0].hypot(affine[0][1]) * affine[1][0].hypot(affine[1][1]);
        if affine.iter().flatten().any(|v| !v.is_finite())
            || !d.is_finite()
            || !scale.is_finite()
            || d.abs() <= scale * 1e-12
            || scale == 0.
        {
            return Err("calibration produces a singular world mapping".into());
        }
        // Validate optional independent checks too, before touching any output.
        if !source["check_controls"].is_null() {
            for c in source["check_controls"]
                .as_array()
                .ok_or("check_controls must be an array")?
            {
                mercator(pair(&c["lonlat"], "check lonlat")?)?;
                pair(&c["world"], "check world")?;
            }
        }
        Ok(Self {
            origin,
            affine,
            north,
            bounds,
            source,
        })
    }

    pub(super) fn to_world(&self, lonlat: Point) -> Result<Point, String> {
        let p = mercator(lonlat)?;
        let (x, y) = (p[0] - self.origin[0], p[1] - self.origin[1]);
        let mut world = self.affine.map(|r| r[0] * x + r[1] * y + r[2]);
        if let Some([threshold, factor]) = self.north {
            world[0] += factor * (threshold - world[1]).max(0.);
        }
        if world.iter().any(|v| !v.is_finite()) {
            return Err("world mapping overflow".into());
        }
        Ok(world)
    }

    pub(super) fn to_lonlat(&self, mut world: Point) -> Result<Point, String> {
        if world.iter().any(|v| !v.is_finite()) {
            return Err("world coordinates must be finite".into());
        }
        if let Some([threshold, factor]) = self.north {
            world[0] -= factor * (threshold - world[1]).max(0.);
        }
        let [a, b, c] = self.affine[0];
        let [d, e, f] = self.affine[1];
        let det = a * e - b * d;
        let (x, y) = (world[0] - c, world[1] - f);
        geographic([
            (e * x - b * y) / det + self.origin[0],
            (a * y - d * x) / det + self.origin[1],
        ])
    }

    pub(super) fn inside(&self, p: Point) -> bool {
        p[0] >= self.bounds[0]
            && p[0] <= self.bounds[2]
            && p[1] >= self.bounds[1]
            && p[1] <= self.bounds[3]
    }
}
