//! Explicit component geometry shares the map's world-space depth buffer.
use super::*;
pub(super) fn append_networks(scene: &mut Scene, program: &Program, theme: egui::Theme) {
    use opencity::network::{self, BandKind, Side};
    for instance in network::project(program).instances {
        let mut bands = instance.geometry.bands.clone();
        // A component with only route geometry remains visible as narrow ribbons.
        if bands.is_empty() {
            for path in &instance.geometry.paths {
                bands.push(network::Band {
                    name: path.name.clone(),
                    kind: BandKind::Surface,
                    path: path.name.clone(),
                    width: 0.1,
                    side: Side::Center,
                    material: network::Material::Asphalt,
                });
            }
        }
        bands.sort_by_key(|b| b.kind == BandKind::Marking);
        for band in &bands {
            let [r, g, b] = band.material.rgb_for_theme(theme == egui::Theme::Light);
            let color = Color32::from_rgb(r, g, b);
            for quad in instance.geometry.band_quads(band) {
                let vertices = quad.map(|point| {
                    let p = instance.world_point(point);
                    WorldVertex {
                        point: WorldPoint { x: p.x, y: p.y },
                        height: p.z
                            + if band.kind == BandKind::Marking {
                                0.008
                            } else {
                                0.004
                            },
                        depth_bias: if band.kind == BandKind::Marking {
                            -0.002
                        } else {
                            0.
                        },
                        color,
                        surface: [0.0; 4],
                    }
                });
                for index in [0, 1, 2, 0, 2, 3] {
                    scene.vertices.push(vertices[index]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE:&str="intersection sample { path p { move (0,0,2); line (1,1,3); } surface pavement along p width 0.2; marking stripe along p width 0.01 color yellow; } placed=sample(10,20,3);";
    #[test]
    fn surfaces_and_markings_keep_world_positions_and_independent_heights() {
        let program = parse(SOURCE).unwrap();
        let before = program.clone();
        let mut scene = Scene::default();
        append_networks(&mut scene, &program, egui::Theme::Dark);
        assert_eq!(scene.vertices.len(), 12);
        assert!(scene
            .vertices
            .iter()
            .all(|v| v.point.x > 9. && v.point.y < 21.));
        assert!((scene.vertices[0].height - 5.004).abs() < 0.0001);
        assert!((scene.vertices[6].height - 5.008).abs() < 0.0001);
        assert_eq!(program, before);
    }
    #[test]
    fn incremental_and_native_scene_builds_include_the_same_components() {
        let program = parse(SOURCE).unwrap();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let full = build_scene(&program, &projection, palette, &[], None);
        let mut incremental = WebSceneBuild::new(program, projection, palette, None);
        while !incremental.advance(std::time::Duration::from_secs(1)) {}
        assert_eq!(incremental.finish().vertices, full.vertices);
    }
}

#[cfg(test)]
mod default_tests {
    use super::*;
    #[test]
    fn default_map_junction_uses_authored_sidewalk_and_stop_line_materials() {
        let program = parse("place(four_way, cross, 0, 0, 0);").unwrap();
        let projection = map::project(&program);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let intersection = map::intersections(&program, &projection).remove(0);
        let mut scene = Scene::default();
        super::super::geometry::append_intersection(
            &mut scene,
            &egui::Context::default(),
            &program,
            &projection,
            &geometry,
            &intersection,
            editor_palette(egui::Theme::Dark),
        );
        let white = Color32::from_rgb(245, 246, 239);
        let sidewalk = kind_color(RoadElementKind::Sidewalk, egui::Theme::Dark);
        assert!(scene.vertices.iter().filter(|v| v.color == white).count() >= 24);
        assert!(scene.vertices.iter().any(|v| v.color == sidewalk));
    }
}
