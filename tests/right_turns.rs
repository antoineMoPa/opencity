use opencity::{
    map,
    network::{self, BandKind, Material, Point},
    parse,
    road_geometry::RoadGeometry,
    road_profiles, RoadElementKind,
};

fn source(sx: f32, sy: f32, mixed: bool, reverse: bool, placed: bool) -> String {
    let vertical = if mixed {
        "sidewalk bikelane carlane middle carlane bikelane sidewalk"
    } else {
        "sidewalk carlane middle carlane sidewalk"
    };
    let horizontal = if mixed {
        "buildingedge sidewalk parkinglane carlane buslane middle buslane \
            carlane parkinglane sidewalk buildingedge"
    } else {
        vertical
    };
    let mut result = format!(
        "road v: {vertical}; end\nroad h: {horizontal}; \
            end\nc=map_point(0,0);\nv=map_point(0,{sy});\nh=map_point({sx},\
            0);\n"
    );
    let v_ref = if placed { "vp" } else { "c" };
    let h_ref = if placed { "hp" } else { "c" };
    if placed {
        result += &format!("vp=map_point(0,{});hp=map_point({},0);", sy * 0.3, sx * 0.3);
    }
    let v_call = if reverse {
        format!("a=v(v,0,{},0,{},{});", sy * 0.7, sy * 0.5, v_ref)
    } else {
        format!("a=v({v_ref},0,{},0,{},v);", sy * 0.5, sy * 0.7)
    };
    let h_call = if reverse {
        format!("b=h(h,{},0,{},0,{});", sx * 0.7, sx * 0.5, h_ref)
    } else {
        format!("b=h({h_ref},{},0,{},0,h);", sx * 0.5, sx * 0.7)
    };
    result += &v_call;
    result += &h_call;
    let v_side = if sy > 0. { "north" } else { "south" };
    let h_side = if sx > 0. { "east" } else { "west" };
    result += &if placed {
        format!("intersection2(c,v,{v_side},vp,{h_side},hp);")
    } else {
        format!("intersection2(c,v,{h_side},h);")
    };
    result
}
fn inside(p: Point, quad: [Point; 4]) -> bool {
    [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]]
        .into_iter()
        .any(|triangle| {
            let mut positive = false;
            let mut negative = false;
            let mut area = 0.;
            for i in 0..3 {
                let a = triangle[i];
                let b = triangle[(i + 1) % 3];
                let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                positive |= cross > 1e-10;
                negative |= cross < -1e-10;
                area += a.x * b.y - a.y * b.x;
            }
            area.abs() > 1e-12 && !(positive && negative)
        })
}

#[test]
fn full_profiles_match_road_seams_in_all_four_orientations_and_both_directions() {
    for sx in [-1., 1.] {
        for sy in [-1., 1.] {
            for profile in 0..3 {
                for reverse in [false, true] {
                    for placed in [false, true] {
                        let mut source = source(sx, sy, profile != 0, reverse, placed);
                        if profile == 2 {
                            source = source.replace(
                                "road v: sidewalk bikelane carlane middle carlane bikelane \
                                    sidewalk;",
                                "road v: sidewalk bikelane carlane middle carlane;",
                            ).replace(
                                "road h: buildingedge sidewalk parkinglane carlane buslane middle \
                                    buslane carlane parkinglane sidewalk buildingedge;",
                                "road h: buildingedge sidewalk parkinglane carlane buslane middle \
                                    buslane carlane;",
                            );
                        }
                        let program = parse(&source).unwrap();
                        let projection = map::project(&program);
                        let junctions = map::intersections(&program, &projection);
                        assert_eq!(junctions.len(), 1, "{source}");
                        let junction = &junctions[0];
                        let instance =
                            network::default_right_turn_instance(&program, &projection, junction)
                                .unwrap();
                        let geometry = RoadGeometry::compile(&program, &projection);
                        for (index, segment) in projection.segments.iter().enumerate() {
                            let road = road_profiles::road_for_segment(&program, segment).unwrap();
                            let row = if reverse {
                                geometry.cross_sections(index).last().unwrap()
                            } else {
                                &geometry.cross_sections(index)[0]
                            };
                            for (band, element) in road.elements.iter().enumerate() {
                                let a = row.boundaries[band].point;
                                let b = row.boundaries[band + 1].point;
                                let mut point = Point {
                                    x: (a.x + b.x) / 2.,
                                    y: -(a.y + b.y) / 2.,
                                    z: 0.,
                                };
                                // Move just inside the junction along its approach tangent.
                                if segment.start.x == segment.end.x {
                                    point.y += sy * 0.000001;
                                } else {
                                    point.x -= sx * 0.000001;
                                }
                                let surfaces: Vec<_> = instance
                                    .geometry
                                    .bands
                                    .iter()
                                    .filter(|b| {
                                        b.kind == BandKind::Surface
                                            && instance
                                                .geometry
                                                .band_quads(b)
                                                .into_iter()
                                                .any(|q| inside(point, q))
                                    })
                                    .collect();
                                assert!(
                                    surfaces
                                        .iter()
                                        .any(|b| b.material == Material::RoadElement(element.kind)),
                                    "missing {:?} seam at {:?} for {source}",
                                    element.kind,
                                    point
                                );
                            }
                        }
                        for mesh in &instance.geometry.band_meshes {
                            for q in &mesh.quads {
                                for p in q {
                                    assert!(p.x.is_finite() && p.y.is_finite());
                                }
                            }
                        }
                        assert_eq!(instance.geometry.movements.len(), 2);
                        for movement in &instance.geometry.movements {
                            let port = |name: &str| {
                                instance
                                    .geometry
                                    .ports
                                    .iter()
                                    .find(|p| format!("{}.{}", p.name, p.role.keyword()) == name)
                                    .unwrap()
                            };
                            assert_eq!(
                                movement.points.first(),
                                Some(&port(&movement.movement.from).position)
                            );
                            assert_eq!(
                                movement.points.last(),
                                Some(&port(&movement.movement.to).position)
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rounded_bends_leave_inside_and_outside_corners_open_without_filler() {
    for sx in [-1., 1.] {
        for sy in [-1., 1.] {
            let program = parse(&source(sx, sy, false, false, true)).unwrap();
            let projection = map::project(&program);
            let junction = map::intersections(&program, &projection).remove(0);
            let instance =
                network::default_right_turn_instance(&program, &projection, &junction).unwrap();
            assert!(instance
                .geometry
                .bands
                .iter()
                .all(|b| matches!(b.material, Material::RoadElement(_))));
            for sign in [-1., 1.] {
                let point = Point {
                    x: sx * sign * 0.299,
                    y: -sy * sign * 0.299,
                    z: 0.,
                };
                assert!(
                    !instance
                        .geometry
                        .band_meshes
                        .iter()
                        .flat_map(|m| &m.quads)
                        .any(|q| inside(point, *q)),
                    "filled corner {point:?}"
                );
            }
            let sidewalk_count = instance
                .geometry
                .bands
                .iter()
                .filter(|b| b.material == Material::RoadElement(RoadElementKind::Sidewalk))
                .count();
            assert_eq!(sidewalk_count, 2);
        }
    }
}
