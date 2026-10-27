use super::*;

#[test]
fn default_map_selection_reads_reuse_projection_and_follow_edits() {
    let mut editor = sample_editor();
    editor.program = boot_program();
    let start = std::time::Instant::now();
    let projection = editor.projected_map();
    let cold = start.elapsed();
    assert_eq!(projection.segments.len(), 592);
    let segment = projection
        .segments
        .iter()
        .find(|segment| segment.source.editable())
        .unwrap();
    let source = segment.source;
    let midpoint = segment.point_at(0.5);
    editor.selected_segment = Some(source);
    editor.selected_segments = vec![source];
    editor.selected_intersection = None;
    editor.selected_landscape = None;
    let start = std::time::Instant::now();
    for _ in 0..3 {
        editor.focus_orbit_on_selection();
        editor.copy_selection();
        assert_eq!(editor.orbit_target, midpoint);
        assert_eq!(editor.clipboard.len(), 1);
        assert_eq!(editor.clipboard[0].points[0], segment.start);
        assert!(Arc::ptr_eq(&projection, &editor.projected_map()));
    }
    let warm = start.elapsed();
    eprintln!("Default-map projection {cold:?}; three focus/copy pairs {warm:?}");
    // Six read-only operations must cost less than one full projection. This
    // relative budget catches rebuilding the city on selection without relying
    // on a machine-specific frame-time threshold.
    assert!(
        warm < cold,
        "selection reads rebuilt the city: {warm:?} >= {cold:?}"
    );

    let control = segment.controls[0];
    assert!(map::set_control(
        &mut editor.program,
        source,
        false,
        WorldPoint {
            x: control.x + 0.5,
            y: control.y
        },
    ));
    editor.focus_orbit_on_selection();
    editor.copy_selection();
    let changed = editor.projected_map();
    assert!(!Arc::ptr_eq(&projection, &changed));
    let segment = changed
        .segments
        .iter()
        .find(|segment| segment.source == source)
        .unwrap();
    assert_eq!(editor.orbit_target, segment.point_at(0.5));
    assert_eq!(editor.clipboard[0].points[1], segment.controls[0]);
}
