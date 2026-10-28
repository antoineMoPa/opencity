//! An inspectable, top-down SVG map of the projected city.

use std::collections::HashMap;
use std::fmt::Write;

use super::map;
use opencity::{
    project_landscape_features, LandscapeKind, Program, RoadDefinition, RoadDirection, Statement,
};

fn xml_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn area_style(kind: LandscapeKind) -> (&'static str, &'static str) {
    match kind {
        LandscapeKind::Land => ("#bdcbae", "#91a383"),
        LandscapeKind::Beach => ("#e7d8a4", "#cbbd8b"),
        LandscapeKind::Concrete => ("#aeb8b9", "#829197"),
        LandscapeKind::Lake => ("#75b7c8", "#5b9aad"),
        LandscapeKind::River => ("none", "#75b7c8"),
        LandscapeKind::Vegetation => ("#83a77e", "#668963"),
        LandscapeKind::Terrain => ("#9eae8c", "#75866d"),
        LandscapeKind::Building => ("#a4a8a5", "#707a79"),
    }
}

fn bounds_point(bounds: &mut [f32; 4], x: f32, y: f32) {
    bounds[0] = bounds[0].min(x);
    bounds[1] = bounds[1].min(y);
    bounds[2] = bounds[2].max(x);
    bounds[3] = bounds[3].max(y);
}

/// Map world north (+Y) to the SVG screen's upward direction (-Y).
fn svg_y(world_y: f32) -> f32 {
    if world_y == 0.0 {
        0.0
    } else {
        -world_y
    }
}

/// Return an SVG with world-coordinate geometry and machine-readable attributes.
/// The road path follows placement point order; `data-direction` states permitted travel.
pub fn export(program: &Program) -> String {
    let projection = map::project(program);
    let landscape = project_landscape_features(program);
    if projection.segments.is_empty() && landscape.features.is_empty() {
        return String::new();
    }

    let roads: HashMap<&str, &RoadDefinition> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some((road.name.as_str(), road)),
            _ => None,
        })
        .collect();
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for feature in &landscape.features {
        for point in &feature.points {
            bounds_point(&mut bounds, point[0], svg_y(point[1]));
        }
    }
    for segment in &projection.segments {
        for point in [
            segment.start,
            segment.controls[0],
            segment.controls[1],
            segment.end,
        ] {
            bounds_point(&mut bounds, point.x, svg_y(point.y));
        }
    }
    let width = (bounds[2] - bounds[0]).max(1.0);
    let height = (bounds[3] - bounds[1]).max(1.0);
    let widest_road = roads
        .values()
        .map(|road| {
            road.elements
                .iter()
                .map(|element| map::element_world_width(element.kind))
                .sum::<f32>()
        })
        .fold(0.16_f32, f32::max);
    let pad = width.max(height) * 0.025 + widest_road * 0.5 + 0.2;
    let x = bounds[0] - pad;
    let y = bounds[1] - pad;
    let view_width = width + pad * 2.0;
    let view_height = height + pad * 2.0;
    let pixel_width = (view_width * 90.0).clamp(640.0, 4096.0).round();
    let pixel_height = (pixel_width * view_height / view_width).round();
    let mut svg = String::new();
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
            width=\"{pixel_width:.0}\" height=\"{pixel_height:.0}\" \
            viewBox=\"{x:.4} {y:.4} {view_width:.4} {view_height:.4}\" \
            role=\"img\" aria-label=\"Annotated OpenCity map\">"
    )
    .unwrap();
    writeln!(svg, "<title>OpenCity annotated map</title>").unwrap();
    writeln!(
        svg,
        "<desc>World coordinates use positive X east and positive Y north. \
            SVG geometry maps world Y to negative SVG Y so north appears \
            upward. Road paths run from their start point to end point; \
            direction attributes indicate permitted travel.</desc>"
    )
    .unwrap();
    writeln!(
        svg,
        "<rect x=\"{x:.4}\" y=\"{y:.4}\" width=\"{view_width:.4}\" \
            height=\"{view_height:.4}\" fill=\"#75b7c8\" \
            data-kind=\"water-background\"/>"
    )
    .unwrap();

    writeln!(svg, "<g id=\"landscape\">").unwrap();
    for (index, feature) in landscape.features.iter().enumerate() {
        let (fill, stroke) = area_style(feature.kind);
        let kind = feature.kind.call_name();
        let name = feature.name.as_deref().unwrap_or("");
        write!(
            svg,
            "<g id=\"feature-{index}\" data-kind=\"{kind}\" data-name=\"{}\" \
                data-elevation=\"{:.4}\"",
            xml_text(name),
            feature.elevation
        )
        .unwrap();
        if let Some(height) = feature.height {
            write!(svg, " data-height=\"{height:.4}\"").unwrap();
        }
        if let Some(model) = &feature.model {
            write!(svg, " data-model=\"{}\"", xml_text(model)).unwrap();
        }
        if let Some(vegetation) = &feature.vegetation_type {
            write!(svg, " data-vegetation-type=\"{}\"", xml_text(vegetation)).unwrap();
        }
        writeln!(svg, ">").unwrap();
        writeln!(
            svg,
            "<title>{}: {}</title>",
            kind,
            xml_text(if name.is_empty() { "unnamed" } else { name })
        )
        .unwrap();
        let points: Vec<String> = feature
            .points
            .iter()
            .map(|point| format!("{:.4},{:.4}", point[0], svg_y(point[1])))
            .collect();
        if feature.kind == LandscapeKind::River {
            writeln!(
                svg,
                "<polyline points=\"{}\" fill=\"none\" stroke=\"{stroke}\" \
                    stroke-width=\"0.14\" stroke-linecap=\"round\" \
                    stroke-linejoin=\"round\"/>",
                points.join(" ")
            )
            .unwrap();
        } else {
            writeln!(
                svg,
                "<polygon points=\"{}\" fill=\"{fill}\" stroke=\"{stroke}\" \
                    stroke-width=\"0.012\" stroke-linejoin=\"round\"/>",
                points.join(" ")
            )
            .unwrap();
        }
        if !name.is_empty() && feature.kind != LandscapeKind::Building {
            let center = feature.points.iter().fold([0.0, 0.0], |sum, point| {
                [sum[0] + point[0], sum[1] + point[1]]
            });
            let count = feature.points.len() as f32;
            writeln!(
                svg,
                "<text x=\"{:.4}\" y=\"{:.4}\" text-anchor=\"middle\" \
                    font-family=\"sans-serif\" font-size=\"0.11\" \
                    fill=\"#273b3b\">{}</text>",
                center[0] / count,
                svg_y(center[1] / count),
                xml_text(name)
            )
            .unwrap();
        }
        writeln!(svg, "</g>").unwrap();
    }
    writeln!(svg, "</g>").unwrap();

    writeln!(svg, "<g id=\"roads\">").unwrap();
    for (index, segment) in projection.segments.iter().enumerate() {
        let road = roads.get(segment.road_name.as_str());
        let road_width = road
            .map(|road| {
                road.elements
                    .iter()
                    .map(|element| map::element_world_width(element.kind))
                    .sum::<f32>()
            })
            .unwrap_or(0.16);
        let name = segment.name.as_deref().unwrap_or("");
        let path = format!(
            "M {:.4} {:.4} C {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
            segment.start.x,
            svg_y(segment.start.y),
            segment.controls[0].x,
            svg_y(segment.controls[0].y),
            segment.controls[1].x,
            svg_y(segment.controls[1].y),
            segment.end.x,
            svg_y(segment.end.y)
        );
        writeln!(
            svg,
            "<g id=\"road-{index}\" data-kind=\"road\" data-name=\"{}\" \
                data-road-type=\"{}\" data-direction=\"{}\" data-start-ref=\"{}\" \
                data-end-ref=\"{}\" data-start-z=\"{:.4}\" data-end-z=\"{:.4}\" \
                data-width=\"{road_width:.4}\">",
            xml_text(name),
            xml_text(&segment.road_name),
            segment.direction.keyword(),
            xml_text(segment.start_ref.as_deref().unwrap_or("")),
            xml_text(segment.end_ref.as_deref().unwrap_or("")),
            segment.start_z,
            segment.end_z
        )
        .unwrap();
        writeln!(
            svg,
            "<title>{}: {} ({})</title>",
            xml_text(&segment.road_name),
            xml_text(if name.is_empty() { "unnamed" } else { name }),
            segment.direction.keyword()
        )
        .unwrap();
        writeln!(
            svg,
            "<path d=\"{path}\" fill=\"none\" stroke=\"#45535a\" \
                stroke-width=\"{road_width:.4}\" stroke-linecap=\"round\"/>"
        )
        .unwrap();
        writeln!(
            svg,
            "<path d=\"{path}\" fill=\"none\" stroke=\"#d8d4bd\" \
                stroke-width=\"0.012\" stroke-dasharray=\"0.08 0.06\"/>"
        )
        .unwrap();
        let midpoint = segment.point_at(0.5);
        let tangent = segment.tangent_at(0.5);
        let tangent_length = (tangent.x * tangent.x + tangent.y * tangent.y).sqrt();
        if tangent_length > 0.0001 {
            let sign = if segment.direction == RoadDirection::Reverse {
                -1.0
            } else {
                1.0
            };
            let dx = sign * tangent.x / tangent_length * 0.095;
            let dy = svg_y(sign * tangent.y / tangent_length * 0.095);
            let arrows = if segment.direction == RoadDirection::Both {
                2
            } else {
                1
            };
            for arrow in 0..arrows {
                let direction = if arrow == 1 { -1.0 } else { 1.0 };
                let offset = if arrows == 2 {
                    (arrow as f32 - 0.5) * road_width * 0.38
                } else {
                    0.0
                };
                let px = midpoint.x - dy * offset / 0.095;
                let py = svg_y(midpoint.y) + dx * offset / 0.095;
                writeln!(
                    svg,
                    "<path d=\"M {:.4} {:.4} L {:.4} {:.4} M {:.4} {:.4} L {:.4} {:.4} \
                        L {:.4} {:.4}\" fill=\"none\" stroke=\"#f6e9b5\" \
                        stroke-width=\"0.018\" stroke-linecap=\"round\"/>",
                    px - dx * direction,
                    py - dy * direction,
                    px + dx * direction,
                    py + dy * direction,
                    px + dx * direction,
                    py + dy * direction,
                    px - dx * direction * 0.1 - dy * 0.38,
                    py - dy * direction * 0.1 + dx * 0.38,
                    px - dx * direction * 0.1 + dy * 0.38,
                    py - dy * direction * 0.1 - dx * 0.38
                )
                .unwrap();
            }
        }
        if !name.is_empty() {
            writeln!(
                svg,
                "<text x=\"{:.4}\" y=\"{:.4}\" text-anchor=\"middle\" \
                    font-family=\"sans-serif\" font-size=\"0.10\" fill=\"#152d35\" \
                    paint-order=\"stroke\" stroke=\"#eae9db\" \
                    stroke-width=\"0.025\">{}</text>",
                midpoint.x,
                svg_y(midpoint.y) - road_width * 0.7,
                xml_text(name)
            )
            .unwrap();
        }
        writeln!(svg, "</g>").unwrap();
    }
    writeln!(svg, "</g></svg>").unwrap();
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencity::parse;

    #[test]
    fn export_contains_projected_geometry_and_inspectable_annotations() {
        let program = parse(
            "road one: carlane; direction reverse; end; a = map_point(0, 0); b \
                = map_point(2, 0); route = one(a, 0.6, 0, 1.4, 0, b); shore = \
                beach(-1, -1, 3, -1, 3, 1, -1, 1); house = building(0.4, \
                \"models/a&b.glb\", 0, 0, 1, 0, 1, 1);",
        )
        .unwrap();
        let svg = export(&program);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("data-kind=\"beach\" data-name=\"shore\""));
        assert!(svg.contains(
            "data-kind=\"road\" data-name=\"route\" data-road-type=\"one\" \
                data-direction=\"reverse\""
        ));
        assert!(svg.contains("data-start-ref=\"a\" data-end-ref=\"b\""));
        assert!(svg.contains("data-model=\"models/a&amp;b.glb\""));
        assert!(svg.contains("<path d=\"M 0.0000 0.0000 C"));
    }

    #[test]
    fn empty_program_has_no_export() {
        assert!(export(&Program::default()).is_empty());
    }

    #[test]
    fn positive_world_y_is_drawn_above_negative_world_y_with_upright_labels() {
        let program = parse(
            "road street: carlane; direction forward; end; south = \
                map_point(0, -1); north = map_point(0, 1); climb = street(south, \
                0, -0.5, 0, 0.5, north); upper = land(-1, 2, 1, 2, 0, 3);",
        )
        .unwrap();
        let svg = export(&program);
        assert!(svg.contains("points=\"-1.0000,-2.0000 1.0000,-2.0000 0.0000,-3.0000\""));
        assert!(svg.contains(
            "<path d=\"M 0.0000 1.0000 C 0.0000 0.5000 0.0000 -0.5000 0.0000 \
                -1.0000\""
        ));
        assert!(svg.contains("<text x=\"0.0000\" y=\"-0.0315\""));
        assert!(!svg.contains("transform=\"scale(1 -1)\""));
    }
}
