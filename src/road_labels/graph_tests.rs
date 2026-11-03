use crate::{map, parse, road_labels::RoadNameState};

fn project(body: &str) -> map::Projection {
    let source = format!("road local: carlane; end {body}");
    map::project(&parse(&source).unwrap())
}

const CHAIN: &str = "s = map_point(0, 0); p = map_point(1, 0);
    q = map_point(2, 0); t = map_point(3, 0);
    a = local(s, 0.33, 0, 0.67, 0, p); b = local(p, 1.33, 0, 1.67, 0, q);
    c = local(q, 2.33, 0, 2.67, 0, t);";

#[test]
fn sparse_anchor_and_concurrent_routes_propagate_and_rebuild_after_edit() {
    let source = format!(
        "road local: carlane; end {CHAIN}
        road_path(a, \"River Road\"); road_routes(a, \"Route 7\");
        road_routes(c, \"Route 8\");"
    );
    let mut program = parse(&source).unwrap();
    let old = map::project(&program);
    assert_eq!(old.road_names.groups.len(), 1);
    for road in &old.segments {
        assert_eq!(road.label_path, ["River Road"]);
        assert_eq!(road.route_labels, ["Route 7", "Route 8"]);
    }
    super::set_road_path(&mut program, "a", &["Canal Road".into()]).unwrap();
    assert_eq!(
        map::project(&program).segments[2].label_path,
        ["Canal Road"]
    );
    assert_eq!(old.segments[2].label_path, ["River Road"]);
}

#[test]
fn conflicting_names_are_reported_until_an_explicit_boundary_is_added() {
    let labels = "road_path(a, \"West Road\"); road_path(c, \"East Road\");";
    let conflict = project(&format!("{CHAIN} {labels}"));
    assert_eq!(
        conflict.road_names.groups[0].state,
        RoadNameState::ConflictingAnchors
    );
    assert!(conflict.segments[1].label_path.is_empty());
    let bounded = project(&format!(
        "{CHAIN} {labels} road_boundary(q); road_boundary(missing);"
    ));
    assert_eq!(bounded.segments[1].label_path, ["West Road"]);
    assert_eq!(bounded.segments[2].label_path, ["East Road"]);
    assert_eq!(bounded.road_names.unused_boundaries, ["missing"]);
}

#[test]
fn separate_references_and_grade_separation_do_not_share_names() {
    let map = project(
        "p = map_point(1, 0); other = map_point(1, 0);
        s = map_point(0, 0); t = map_point(2, 0);
        a = local(s, 0.33, 0, 0.67, 0, p); b = local(other, 1.33, 0, 1.67, 0, t);
        c = local(p, 1.33, 0, 1.67, 0, t);
        elevation(c, 1); road_path(a, \"River Road\");",
    );
    assert!(map.segments[1].label_path.is_empty());
    assert!(map.segments[2].label_path.is_empty());
    assert_eq!(map.road_names.groups.len(), 3);
}

#[test]
fn equal_forks_are_ambiguous_and_do_not_inherit_the_trunk_name() {
    let map = project(
        "p = map_point(0, 0); s = map_point(-1, 0);
        t = map_point(1, 0.1); u = map_point(1, -0.1);
        a = local(s, -0.67, 0, -0.33, 0, p);
        b = local(p, 0.33, 0.033, 0.67, 0.067, t);
        c = local(p, 0.33, -0.033, 0.67, -0.067, u); road_path(a, \"Trunk\");",
    );
    assert_eq!(map.road_names.ambiguous_nodes, ["p"]);
    assert!(map.segments[1].label_path.is_empty());
    assert!(map.segments[2].label_path.is_empty());
}

#[test]
fn sidewalk_changes_are_compatible_but_ramps_are_a_separate_family() {
    let source = "road main: carlane; role highway; end
        road wider: sidewalk carlane sidewalk; role highway; end
        road ramp: carlane; role ramp; end
        p = map_point(0, 0); s = map_point(-1, 0); t = map_point(1, 0);
        a = main(s, -0.67, 0, -0.33, 0, p);
        b = wider(p, 0.33, 0, 0.67, 0, t); c = ramp(p, 0.33, 0, 0.67, 0, t);
        road_path(a, \"Mainline\");";
    let map = map::project(&parse(source).unwrap());
    assert_eq!(map.segments[1].label_path, ["Mainline"]);
    assert!(map.segments[2].label_path.is_empty());
}

#[test]
fn native_junction_pairs_opposite_arms_without_cross_axis_leakage() {
    let source = format!(
        "{} road_path(west_street, \"Cross Street\");
        road_path(north_street, \"North Street\");",
        include_str!("../../road_library/four_way.road")
    );
    let map = map::project(&parse(&source).unwrap());
    assert_eq!(map.road_names.groups.len(), 2);
    let path = |name| {
        &map.segments
            .iter()
            .find(|r| r.name.as_deref() == Some(name))
            .unwrap()
            .label_path
    };
    assert_eq!(path("east_street"), &["Cross Street"]);
    assert_eq!(path("south_street"), &["North Street"]);
}

#[test]
fn lane_attachment_does_not_turn_a_branch_into_a_continuation() {
    let map = project(
        "a = local(-2, 0, 2, 0);
        p = lane_point(a, 0, 0.5); t = map_point(0, 2);
        b = local(p, 0, 0.7, 0, 1.4, t); road_path(a, \"Main Street\");",
    );
    assert_eq!(map.segments.len(), 2);
    assert!(map.segments[1].label_path.is_empty());
}

#[test]
fn scoped_boundaries_and_aliases_remain_inside_their_instance() {
    let source = format!(
        "road local: carlane; end block corridor: {CHAIN}
        road_boundary(q); road_path(a, \"West\"); road_path(c, \"East\"); end
        one = corridor(0, 0); two = corridor(0, 5);
        road_routes(one, \"Route 7\"); road_routes(two, \"Route 8\");"
    );
    let map = map::project(&parse(&source).unwrap());
    assert!(map.road_names.unused_boundaries.is_empty());
    assert_eq!(map.segments.len(), 6);
    assert_eq!(map.segments[1].label_path.last().unwrap(), "West");
    assert_eq!(map.segments[2].label_path.last().unwrap(), "East");
    assert_eq!(map.segments[1].route_labels, ["Route 7"]);
    assert_eq!(map.segments[4].route_labels, ["Route 8"]);
}

#[test]
fn topology_edits_rebuild_groups_and_preserve_the_old_projection() {
    let mut program = parse(&format!(
        "road local: carlane; end {CHAIN} road_path(a, \"River Road\");"
    ))
    .unwrap();
    let old = map::project(&program);
    assert!(map::remove(&mut program, old.segments[1].source));
    let updated = map::project(&program);
    assert_eq!(updated.road_names.groups.len(), 2);
    assert!(updated.segments[1].label_path.is_empty());
    assert_eq!(old.segments[2].label_path, ["River Road"]);
}

#[test]
fn opposite_one_way_travel_is_not_a_continuation() {
    let source = "road east: carlane; direction forward; end
        road west: carlane; direction reverse; end
        s = map_point(-1, 0); p = map_point(0, 0); t = map_point(1, 0);
        a = east(s, -0.67, 0, -0.33, 0, p);
        b = west(p, 0.33, 0, 0.67, 0, t); road_path(a, \"East Street\");";
    let map = map::project(&parse(source).unwrap());
    assert_eq!(map.road_names.groups.len(), 2);
    assert!(map.segments[1].label_path.is_empty());
}
