use super::*;

#[test]
fn all_cloverleaf_ramp_mouths_align_with_outer_highway_lane() {
    let program = parse(include_str!("../../../../examples/cloverleaf_town.road")).unwrap();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let mut exits = 0;
    let mut merges = 0;
    for (index, segment) in projection.segments.iter().enumerate() {
        if segment.road_name != "ramp_mouth" {
            continue;
        }
        let road = road_for_segment(&program, segment).unwrap();
        for (reference, selected, t) in [
            (
                segment.start_ref.as_deref(),
                profiles[index].start_alignment,
                0.0,
            ),
            (
                segment.end_ref.as_deref(),
                profiles[index].merge_alignment,
                1.0,
            ),
        ] {
            if reference
                .and_then(|name| map::lane_attachment(&program, name))
                .is_none()
            {
                continue;
            }
            assert_eq!(
                selected,
                Some(0),
                "{} at t={t}",
                segment.name.as_deref().unwrap()
            );
            let (_, left_edge) = section_widths(road, 100.0, &profiles[index], t);
            assert!(
                (left_edge + map_element_width(RoadElementKind::CarLane, 100.0) / 2.0).abs()
                    < 0.0001,
                "{} at t={t}: ramp lane misses the receiving lane",
                segment.name.as_deref().unwrap()
            );
            if t == 0.0 {
                exits += 1;
            } else {
                merges += 1;
            }
        }
    }
    assert_eq!((exits, merges), (8, 9));
}

#[test]
fn every_ramp_mouth_opens_at_its_highway_attachment() {
    for (source, expected_mouths) in [
        (include_str!("../../../../examples/starter_city.road"), 16),
        (
            include_str!("../../../../examples/cloverleaf_town.road"),
            17,
        ),
    ] {
        let program = parse(source).unwrap();
        let projection = map::project(&program);
        let profiles = paint_profiles(&program, &projection);
        let mut mouths = 0;
        for (index, segment) in projection.segments.iter().enumerate() {
            if segment.road_name != "ramp_mouth" {
                continue;
            }
            mouths += 1;
            let road = road_for_segment(&program, segment).unwrap();
            let profile = &profiles[index];
            let (t, lane, parent) = if let Some(parent) = profile.start_parent.as_ref() {
                (0.0, profile.start_alignment.unwrap(), parent)
            } else {
                (
                    1.0,
                    profile.merge_alignment.unwrap(),
                    profile.end_parent.as_ref().unwrap(),
                )
            };
            let (widths, left) = section_widths(road, 1.0, profile, t);
            let lane_center = left + widths[..lane].iter().sum::<f32>() + widths[lane] / 2.0;
            assert!(
                lane_center.abs() < 0.00001,
                "{} lane center",
                segment.name.as_deref().unwrap()
            );
            assert!(
                (widths[lane] - map::element_world_width(RoadElementKind::CarLane)).abs() < 0.00001
            );
            for verge in attachment_verges(road, lane) {
                assert!(
                    (widths[verge] - map::element_world_width(RoadElementKind::Verge)).abs()
                        < 0.00001,
                    "{} verge",
                    segment.name.as_deref().unwrap()
                );
            }
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
            let center = screen_of(rect, egui::Vec2::ZERO, 1000.0, segment.point_at(t));
            let normal = screen_normal_at(segment, t);
            for offset in [left, left + widths.iter().sum::<f32>()] {
                assert!(
                    screen_point_in_road(
                        center + normal * (offset * 1000.0),
                        parent,
                        rect,
                        egui::Vec2::ZERO,
                        1000.0
                    ),
                    "{} ramp outline starts outside receiving road",
                    segment.name.as_deref().unwrap()
                );
            }
        }
        assert_eq!(mouths, expected_mouths);
    }
}

#[test]
fn loop_mouth_lane_enters_parent_verge_without_a_seam_fin() {
    let program = starter_program();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
    let pan = egui::Vec2::ZERO;
    let zoom = 1000.0;
    for (name, seam_t) in [("nw_loop_c_merge", 0.0), ("ne_loop_a_mouth", 1.0)] {
        let index = projection
            .segments
            .iter()
            .position(|segment| segment.name.as_deref() == Some(name))
            .unwrap();
        let segment = &projection.segments[index];
        let road = road_for_segment(&program, segment).unwrap();
        let profile = &profiles[index];
        let (parent, lane) = if seam_t == 0.0 {
            (
                profile.end_parent.as_ref().unwrap(),
                profile.merge_alignment.unwrap(),
            )
        } else {
            (
                profile.start_parent.as_ref().unwrap(),
                profile.start_alignment.unwrap(),
            )
        };
        let row_at = |t| {
            let center = screen_of(rect, pan, zoom, segment.point_at(t));
            let normal = screen_normal_at(segment, t);
            let (widths, mut offset) = section_widths(road, zoom, profile, t);
            let mut row = vec![center + normal * offset];
            for width in widths {
                offset += width;
                row.push(center + normal * offset);
            }
            row
        };
        let mut row = row_at(seam_t);
        let before = row.clone();
        let left_inside = screen_point_in_road(row[lane], parent, rect, pan, zoom);
        let right_inside = screen_point_in_road(row[lane + 1], parent, rect, pan, zoom);
        assert_ne!(
            left_inside, right_inside,
            "{name} must cross the parent edge"
        );
        let (inside, outside) = if left_inside {
            (lane, lane + 1)
        } else {
            (lane + 1, lane)
        };
        tuck_mouth_lane_at_seam(&mut row, seam_t, profile, rect, pan, zoom);
        assert!(
            !screen_point_in_road(row[inside], parent, rect, pan, zoom),
            "{name} fin remains"
        );
        assert_eq!(
            row[outside], before[outside],
            "{name} outside lane edge moved"
        );
        assert!(
            row[inside].distance(row[outside]) > 2.0,
            "{name} lane collapsed"
        );
        let far_t = if seam_t == 0.0 { 0.35 } else { 0.65 };
        let mut far_row = row_at(far_t);
        let original_far_row = far_row.clone();
        tuck_mouth_lane_at_seam(&mut far_row, far_t, profile, rect, pan, zoom);
        assert_eq!(
            far_row, original_far_row,
            "{name} mouth beyond seam changed"
        );
    }
}

#[test]
fn adjoining_loop_ramp_outlines_clip_at_the_mouths_parent() {
    let program = starter_program();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    for (ramp_name, mouth_name, merge) in [
        ("nw_loop_c", "nw_loop_c_merge", true),
        ("ne_loop_a", "ne_loop_a_mouth", false),
    ] {
        let find = |name: &str| {
            projection
                .segments
                .iter()
                .position(|segment| segment.name.as_deref() == Some(name))
                .unwrap()
        };
        let ramp = &profiles[find(ramp_name)];
        let mouth = &profiles[find(mouth_name)];
        let (ramp_parent, mouth_parent) = if merge {
            (ramp.end_parent.as_ref(), mouth.end_parent.as_ref())
        } else {
            (ramp.start_parent.as_ref(), mouth.start_parent.as_ref())
        };
        let ramp_names: Vec<_> = ramp_parent
            .unwrap()
            .segments
            .iter()
            .map(|segment| segment.name.as_deref())
            .collect();
        let mouth_names: Vec<_> = mouth_parent
            .unwrap()
            .segments
            .iter()
            .map(|segment| segment.name.as_deref())
            .collect();
        assert_eq!(ramp_names, mouth_names, "{ramp_name} clips at {mouth_name}");
        assert!(ramp.start_alignment.is_none());
        assert!(ramp.merge_alignment.is_none());
    }
}

#[test]
fn adjoining_loop_ramp_fill_stops_at_the_parent_edge() {
    let program = starter_program();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
    let pan = egui::Vec2::ZERO;
    let zoom = 1000.0;
    for (name, t, merge) in [("nw_loop_c", 1.0, true), ("ne_loop_a", 0.0, false)] {
        let index = projection
            .segments
            .iter()
            .position(|segment| segment.name.as_deref() == Some(name))
            .unwrap();
        let segment = &projection.segments[index];
        let profile = &profiles[index];
        let parent = if merge {
            profile.end_parent.as_ref().unwrap()
        } else {
            profile.start_parent.as_ref().unwrap()
        };
        let road = road_for_segment(&program, segment).unwrap();
        let center = screen_of(rect, pan, zoom, segment.point_at(t));
        let normal = screen_normal_at(segment, t);
        let (widths, mut offset) = section_widths(road, zoom, profile, t);
        let mut row = vec![center + normal * offset];
        for width in widths {
            offset += width;
            row.push(center + normal * offset);
        }
        let original = row.clone();
        let inside: Vec<_> = row
            .iter()
            .map(|point| screen_point_in_road(*point, parent, rect, pan, zoom))
            .collect();
        assert!(inside.iter().any(|inside| *inside), "{name} misses parent");
        assert!(inside.iter().any(|inside| !*inside), "{name} wholly buried");
        trim_adjoining_ramp_to_parent(&mut row, profile, rect, pan, zoom);
        for (index, point) in row.iter().enumerate() {
            assert!(
                !screen_point_in_road(*point, parent, rect, pan, zoom),
                "{name} cap remains at boundary {index}"
            );
            if !inside[index] {
                assert_eq!(*point, original[index], "{name} outside surface moved");
            }
        }
    }
}

#[test]
fn ramp_attachment_footprints_cross_connected_highway_seams() {
    for (source, mouth_name, near_name, far_name) in [
        (
            include_str!("../../../../examples/starter_city.road"),
            "nw_loop_c_merge",
            "interstate_north_south_04",
            "interstate_north_south_05",
        ),
        (
            include_str!("../../../../examples/cloverleaf_town.road"),
            "nw_loop_c_merge",
            "i_ns_04",
            "i_ns_05",
        ),
    ] {
        let program = parse(source).unwrap();
        let projection = map::project(&program);
        let profiles = paint_profiles(&program, &projection);
        let (index, _) = projection
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| segment.name.as_deref() == Some(mouth_name))
            .unwrap();
        let footprint = profiles[index].end_parent.as_ref().unwrap();
        for name in [near_name, far_name] {
            assert!(
                footprint
                    .segments
                    .iter()
                    .any(|segment| segment.name.as_deref() == Some(name)),
                "{mouth_name} must clip its outline across {name}"
            );
        }
    }
}

#[test]
fn graded_starter_merges_paint_after_their_receiving_highways() {
    let program = starter_program();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let order = road_paint_order(&program, &projection, &profiles);
    for (ramp_name, highway_name) in [
        ("nw_loop_c_merge", "interstate_north_south_04"),
        ("se_loop_c_merge", "interstate_north_south_03"),
    ] {
        let ramp = projection
            .segments
            .iter()
            .position(|segment| segment.name.as_deref() == Some(ramp_name))
            .unwrap();
        let highway = projection
            .segments
            .iter()
            .position(|segment| segment.name.as_deref() == Some(highway_name))
            .unwrap();
        assert!(projection.segments[ramp].elevation < projection.segments[highway].elevation);
        assert_eq!(
            projection.segments[ramp].end_z,
            projection.segments[highway].elevation
        );
        assert!(
            order.iter().position(|index| *index == ramp).unwrap()
                > order.iter().position(|index| *index == highway).unwrap(),
            "{ramp_name} must paint over its receiving lane"
        );
    }
}

#[test]
fn cloverleaf_road_joins_taper_without_edge_mismatches() {
    let program = parse(include_str!("../../../../examples/cloverleaf_town.road")).unwrap();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let tapered = profiles
        .iter()
        .map(|profile| {
            usize::from(!profile.start_taper.is_empty())
                + usize::from(!profile.end_taper.is_empty())
        })
        .sum::<usize>();
    assert!(
        tapered >= 17,
        "expected every cloverleaf ramp seam to taper"
    );
    let issues = design_rule_issues(&program, &projection, &profiles);
    assert!(
        existing_design_checks_are_clear(&issues),
        "{}",
        issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    );
}
