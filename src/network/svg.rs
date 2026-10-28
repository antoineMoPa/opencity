use super::*;
use std::fmt::Write;
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn path_data(points: &[Point]) -> String {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{} {} {}", if i == 0 { "M" } else { "L" }, p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ")
}
/// A standalone, inspectable top view. Units and +Y south match the DSL.
pub fn to_svg(network: &Resolved, title: &str, size: u32, show_paths: bool) -> String {
    to_svg_with_options(network, title, size, show_paths, true)
}
pub fn to_svg_with_options(
    network: &Resolved,
    title: &str,
    size: u32,
    show_paths: bool,
    show_ports: bool,
) -> String {
    let bounds = network.bounds();
    let width = (bounds[2] - bounds[0]).max(0.0001);
    let height = (bounds[3] - bounds[1]).max(0.0001);
    let pad = width.max(height) * 0.08;
    let extent = width.max(height) + pad * 2.;
    let view_x = (bounds[0] + bounds[2] - extent) / 2.;
    let view_y = (bounds[1] + bounds[3] - extent) / 2.;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" \
            height=\"{size}\" viewBox=\"{} {} {} {}\"><title>{}</title><rect \
            x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#17202b\"/>",
        view_x,
        view_y,
        extent,
        extent,
        escape(title),
        view_x,
        view_y,
        extent,
        extent
    );
    let mut bands: Vec<_> = network.bands.iter().collect();
    bands.sort_by_key(|b| b.kind == BandKind::Marking);
    for band in bands {
        {
            let rgb = band.material.rgb();
            let color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            let _ = write!(
                out,
                r#"<g data-band="{}" data-path="{}">"#,
                escape(&band.name),
                escape(&band.path)
            );
            let quads = network.band_quads(band);
            if !quads
                .windows(2)
                .all(|pair| pair[0][1] == pair[1][0] && pair[0][2] == pair[1][3])
            {
                // Fan meshes and disconnected patches cannot share a ribbon outline.
                let _ = write!(out, r#"<path fill="{color}" d=""#);
                for quad in &quads {
                    let _ = write!(out, "{} Z ", path_data(quad));
                }
                out.push_str("\"/>");
            } else if let Some(last) = quads.last() {
                // One outline avoids antialias seams between adjacent quads.
                let outline: Vec<_> = quads
                    .iter()
                    .map(|quad| quad[0])
                    .chain([last[1], last[2]])
                    .chain(quads.iter().rev().map(|quad| quad[3]))
                    .collect();
                let points = outline
                    .iter()
                    .map(|p| format!("{},{}", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                let _ = write!(out, r#"<polygon points="{points}" fill="{color}"/>"#);
            }
            out.push_str("</g>");
        }
    }
    if show_paths {
        for path in &network.paths {
            let _ = write!(
                out,
                "<path data-path=\"{}\" d=\"{}\" fill=\"none\" stroke=\"#67b9ed\" \
                    stroke-width=\"{}\" opacity=\"0.6\"/>",
                escape(&path.name),
                path_data(&path.points),
                width.max(height) * 0.003
            );
        }
        for movement in &network.movements {
            let _ = write!(
                out,
                "<path data-from=\"{}\" data-to=\"{}\" data-class=\"{}\" d=\"{}\" \
                    fill=\"none\" stroke=\"#72d7ad\" stroke-width=\"{}\" \
                    opacity=\"0.7\"/>",
                escape(&movement.movement.from),
                escape(&movement.movement.to),
                escape(&movement.movement.class),
                path_data(&movement.points),
                width.max(height) * 0.005
            );
        }
    }
    let radius = width.max(height) * 0.012;
    for port in network.ports.iter().filter(|_| show_ports) {
        let color = if port.role == LaneRole::In {
            "#62d9ad"
        } else {
            "#f3b66a"
        };
        let p = port.position;
        let heading = port.heading.vector();
        let _ = write!(
            out,
            "<g data-port=\"{}.{}\"><circle cx=\"{}\" cy=\"{}\" r=\"{radius}\" \
                fill=\"{color}\"/><path d=\"M {} {} l {} {}\" stroke=\"{color}\" \
                stroke-width=\"{}\"/></g>",
            escape(&port.name),
            port.role.keyword(),
            p.x,
            p.y,
            p.x,
            p.y,
            heading[0] * radius * 3.,
            heading[1] * radius * 3.,
            radius / 3.
        );
    }
    out.push_str("</svg>");
    out
}
