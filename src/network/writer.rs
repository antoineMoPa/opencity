use super::*;
use std::fmt::Write;
fn point(point: Point) -> String {
    if point.z == 0. {
        format!("({}, {})", point.x, point.y)
    } else {
        format!("({}, {}, {})", point.x, point.y, point.z)
    }
}
fn endpoint(value: &Endpoint) -> String {
    match value {
        Endpoint::Point(p) => point(*p),
        Endpoint::Reference(name) => name.clone(),
    }
}
fn bound(value: &Bound) -> String {
    match value {
        Bound::Start => "start".into(),
        Bound::End => "end".into(),
        Bound::Distance(n) => n.to_string(),
        Bound::Anchor(name) => name.clone(),
    }
}
pub(crate) fn write_definition(out: &mut String, definition: &Definition) {
    out.push_str(definition.kind.keyword());
    if let Some(name) = &definition.name {
        let _ = write!(out, " {name}");
    }
    if let Some(origin) = definition.origin {
        let _ = write!(out, " {}", point(origin));
    }
    out.push_str(" {\n");
    for port in &definition.ports {
        let _ = write!(
            out,
            "    port {}.{} at {} heading {}",
            port.name,
            port.role.keyword(),
            point(port.position),
            port.heading.keyword()
        );
        if let Some(width) = port.width {
            let _ = write!(out, " width {width}");
        }
        out.push_str(";\n");
    }
    for path in &definition.paths {
        let _ = writeln!(out, "    path {} {{", path.name);
        let _ = writeln!(out, "        from {};", endpoint(&path.start));
        for segment in &path.segments {
            match segment {
                PathSegment::Line(end) => {
                    let _ = writeln!(out, "        line to {};", endpoint(end));
                }
                PathSegment::Cubic { first, second, end } => {
                    let _ = writeln!(
                        out,
                        "        cubic {}, {} to {};",
                        point(*first),
                        point(*second),
                        endpoint(end)
                    );
                }
                PathSegment::Arc {
                    end,
                    radius,
                    clockwise,
                } => {
                    let _ = writeln!(
                        out,
                        "        arc to {} radius {radius} {};",
                        endpoint(end),
                        if *clockwise {
                            "clockwise"
                        } else {
                            "counterclockwise"
                        }
                    );
                }
            }
        }
        out.push_str("    }\n");
    }
    for anchor in &definition.anchors {
        let _ = writeln!(
            out,
            "    anchor {} at {} distance {};",
            anchor.name, anchor.path, anchor.distance
        );
    }
    for movement in &definition.movements {
        let _ = write!(
            out,
            "    allow {} {} -> {} via ",
            movement.class, movement.from, movement.to
        );
        for (i, step) in movement.via.iter().enumerate() {
            if i > 0 {
                out.push_str(" then ");
            }
            out.push_str(&step.path);
            if step.start != Bound::Start || step.end != Bound::End {
                let _ = write!(out, "[{}..{}]", bound(&step.start), bound(&step.end));
            }
        }
        out.push_str(";\n");
    }
    for band in &definition.bands {
        let _ = writeln!(
            out,
            "    {} {} along {} width {} side {} {} {};",
            if band.kind == BandKind::Surface {
                "surface"
            } else {
                "marking"
            },
            band.name,
            band.path,
            band.width,
            band.side.keyword(),
            if band.kind == BandKind::Surface {
                "material"
            } else {
                "color"
            },
            band.material.keyword()
        );
    }
    out.push_str("}\n");
}
