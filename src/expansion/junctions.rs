//! City junction alternatives in the existing road-end proposal flow.
use super::*;

pub(super) fn append(
    program: &mut Program,
    end: &End,
    reference: &str,
    extension: f32,
    kind: Kind,
) -> bool {
    if end.road.role != RoadRole::Ordinary
        || end.road.direction != RoadDirection::Both
        || end
            .road
            .elements
            .iter()
            .filter(|e| e.kind == RoadElementKind::CarLane)
            .count()
            < 2
    {
        return false;
    }
    let half = width(&end.road) * 0.5 * crate::network::default_four_way_footprint_factor();
    // The junction owns its intrinsic footprint. Only the visible road outside
    // that footprint is adjustable; the incoming join needs a one-metre seam.
    let count = map::project(program).segments.len();
    add(program, end, &end.road, reference, half + 0.01, 0., 0.);
    let projection = map::project(program);
    let approach = &projection.segments[count];
    let (center, reference) = if end.at_end {
        (approach.end, approach.end_ref.as_ref())
    } else {
        (approach.start, approach.start_ref.as_ref())
    };
    let Some(reference) = reference else {
        return false;
    };
    let reference = reference.clone();
    for turn in [
        std::f32::consts::FRAC_PI_2,
        -std::f32::consts::FRAC_PI_2,
        0.,
    ] {
        if turn == 0. && kind == Kind::ThreeWay {
            continue;
        }
        let mut arm = end.clone();
        arm.point = center;
        arm.heading += turn;
        add(
            program,
            &arm,
            &end.road,
            &reference,
            half + extension,
            0.,
            0.,
        );
    }
    program.statements.push(Statement::Call(Call {
        assigns: vec![],
        name: match kind {
            Kind::ThreeWay => "intersection3",
            _ => "intersection4",
        }
        .into(),
        args: vec![
            Argument::Identifier(reference.clone()),
            Argument::Identifier(end.road.name.clone()),
        ],
    }));
    set_elevation(program, &reference, end.height);
    true
}

pub(super) fn clear(program: &Program, end: &End, obstacles: &obstacles::Obstacles) -> bool {
    let projection = map::project(program);
    let Some(junction) = map::intersections(program, &projection).into_iter().last() else {
        return false;
    };
    let half = map::intersection_footprint_half(&junction, width(&end.road) * 0.5);
    // A circumscribed footprint protects rounded corners as well as the road arms.
    !obstacles.point_clashes(
        junction.position,
        end.height,
        half * std::f32::consts::SQRT_2,
        end,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = include_str!("../../tests/fixtures/expansion_junctions.road");

    #[test]
    #[cfg(feature = "game-runtime")]
    fn raised_rotated_zero_exits_keep_pavement_and_open_junction_mouths() {
        use crate::{road_geometry::RoadGeometry, runtime::City};
        let mut program = parse(include_str!(
            "../../tests/fixtures/zero_junction_raised_seed.road"
        ))
        .unwrap();
        let end = ends(&program).into_iter().find(|e| !e.at_end).unwrap();
        propose(
            &program,
            &end,
            Options {
                junction_extension: 0.,
                ..Default::default()
            },
        )
        .into_iter()
        .find(|p| p.kind == Kind::FourWay)
        .unwrap()
        .apply(&mut program)
        .unwrap();
        let projection = map::project(&program);
        let junction = map::intersections(&program, &projection).pop().unwrap();
        let geometry = RoadGeometry::compile(&program, &projection);
        assert_eq!(
            geometry.junction_seams(&projection, &junction).len(),
            4,
            "zero-stub mouths still supply their boundary and deck elevation"
        );
        let city = City::from_program(&program, None).unwrap();
        let angle = map::intersection_rotation(&projection, &junction).to_radians();
        let world = |x: f32, y: f32| {
            [
                junction.position.x + x * angle.cos() - y * angle.sin(),
                junction.position.y + x * angle.sin() + y * angle.cos(),
            ]
        };
        for axis in [false, true] {
            for i in -8..=8 {
                let t = junction.footprint_half * i as f32 / 8.;
                let p = if axis { world(t, 0.) } else { world(0., t) };
                let surface = city
                    .surface_at(p, 0.16)
                    .expect("junction pavement must be continuous");
                assert!((surface.height - 0.142).abs() < 0.001);
            }
            for sign in [-1., 1.] {
                let t = junction.footprint_half * sign;
                let p = if axis { world(t, 0.) } else { world(0., t) };
                assert!(
                    city.road_barrier_contact(p, 0.005, 0.142, 0.17).is_none(),
                    "open exit must not get a retaining wall across its mouth"
                );
            }
        }
    }

    #[test]
    fn junction_roads_end_at_the_face_plus_the_requested_extension() {
        for kind in [Kind::ThreeWay, Kind::FourWay] {
            for extension in [0., 0.005, 0.01, 0.05] {
                let program = parse(SOURCE).unwrap();
                let end = ends(&program).into_iter().find(|e| e.at_end).unwrap();
                let proposal = propose(
                    &program,
                    &end,
                    Options {
                        length: 0.05,
                        junction_extension: extension,
                        ..Default::default()
                    },
                )
                .into_iter()
                .find(|p| p.kind == kind)
                .expect("short junction must stay available");
                let mut next = program.clone();
                proposal.apply(&mut next).unwrap();
                let projection = map::project(&next);
                let junction = map::intersections(&next, &projection).pop().unwrap();
                for arm in proposal.ghosts.iter().skip(1) {
                    let extra = distance(junction.position, arm.end) - junction.footprint_half;
                    assert!((extra - extension).abs() < 0.00011, "{kind:?}: {extra}");
                }
                #[cfg(feature = "game-runtime")]
                crate::runtime::City::from_program(&next, None).unwrap();
                for exit in ends(&next).into_iter().filter(|e| e.point.x > -0.5) {
                    assert!(
                        possibilities(&next, &exit, 1.)
                            .iter()
                            .any(|p| p.kind == Kind::Continue),
                        "{kind:?} extension={extension}: {:?}",
                        exit.point
                    );
                }
            }
        }
    }

    #[test]
    fn both_icons_author_connected_city_junctions_from_either_end() {
        for at_end in [false, true] {
            for kind in [Kind::ThreeWay, Kind::FourWay] {
                let before = parse(SOURCE).unwrap();
                let end = ends(&before)
                    .into_iter()
                    .find(|e| e.at_end == at_end)
                    .unwrap();
                let proposal = possibilities(&before, &end, 1.)
                    .into_iter()
                    .find(|p| p.kind == kind)
                    .expect("city junction option");
                assert_eq!(
                    proposal.ghosts.len(),
                    if kind == Kind::ThreeWay { 3 } else { 4 }
                );
                let mut after = before.clone();
                proposal.apply(&mut after).unwrap();
                let source = edited_source(SOURCE, &before, &after).unwrap();
                assert!(source.starts_with("# Keep this authored street"));
                assert_eq!(parse(&source).unwrap(), after);
                let projection = map::project(&after);
                assert_eq!(projection.unresolved, 0);
                let junction = map::intersections(&after, &projection).pop().unwrap();
                assert_eq!(
                    junction.kind,
                    if kind == Kind::ThreeWay {
                        map::IntersectionKind::ThreeWay
                    } else {
                        map::IntersectionKind::FourWay
                    }
                );
                assert_eq!(
                    map::intersection_arms(&projection, &junction.point_ref).len(),
                    junction.kind.arm_count()
                );
                #[cfg(feature = "game-runtime")]
                crate::runtime::City::from_program(&after, None).unwrap();
            }
        }
    }

    #[test]
    fn occupied_junction_corners_are_rejected_without_changing_the_seed() {
        let mut program = parse(SOURCE).unwrap();
        program.statements.extend(
            parse(include_str!(
                "../../tests/fixtures/expansion_junction_obstacle.road"
            ))
            .unwrap()
            .statements,
        );
        let end = ends(&program).into_iter().find(|e| e.at_end).unwrap();
        let before = program.clone();
        let choices = possibilities(&program, &end, 1.);
        assert!(!choices
            .iter()
            .any(|p| matches!(p.kind, Kind::ThreeWay | Kind::FourWay)));
        assert_eq!(program, before);
    }
}
