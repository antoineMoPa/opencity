use super::*;

#[test]
fn accepted_shapes_select_the_new_tip_and_emit_one_camera_target() {
    for kind in [
        expansion::Kind::Continue,
        expansion::Kind::CurveLeft,
        expansion::Kind::CurveRight,
        expansion::Kind::Rise,
        expansion::Kind::Fall,
        expansion::Kind::AddElement,
        expansion::Kind::RemoveElement,
        expansion::Kind::BranchLeft,
        expansion::Kind::BranchRight,
        expansion::Kind::SplitLeft,
        expansion::Kind::SplitRight,
        expansion::Kind::ThreeWay,
        expansion::Kind::FourWay,
    ] {
        for at_end in [false, true] {
            if !at_end
                && matches!(
                    kind,
                    expansion::Kind::SplitLeft | expansion::Kind::SplitRight
                )
            {
                continue;
            }
            let mut program = crate::parse(
                "road street: sidewalk carlane carlane sidewalk; end a=street(0,0,1,0);",
            )
            .unwrap();
            let mut picker = Picker::default();
            picker.start();
            picker.refresh(&program);
            picker.choose_end(&program, usize::from(at_end));
            picker.selected = picker
                .proposals
                .iter()
                .position(|p| p.kind == kind)
                .unwrap();
            let ghost = picker.selected().unwrap().ghosts.last().unwrap();
            let (expected, heading) = if at_end {
                let tangent = ghost.tangent_at(1.);
                (
                    [ghost.end.x, ghost.end.y, ghost.end_z],
                    tangent.y.atan2(tangent.x),
                )
            } else {
                let tangent = ghost.tangent_at(0.);
                (
                    [ghost.start.x, ghost.start.y, ghost.start_z],
                    (-tangent.y).atan2(-tangent.x),
                )
            };
            picker.accept(&mut program);
            let target = picker.take_follow_target().unwrap();
            assert_eq!(target.position, expected, "{kind:?}");
            assert!((target.heading - heading).abs() < 0.001, "{kind:?}");
            assert_eq!(target.length, 1.);
            assert!(picker.take_follow_target().is_none());
            let next = &picker.ends[picker.end.unwrap()];
            assert_eq!([next.point.x, next.point.y, next.height], expected);
            assert_eq!(
                picker.selected().unwrap().kind,
                expansion::Kind::Continue,
                "{kind:?} at_end={at_end}: {:?}",
                picker.selected().unwrap().end.point
            );
            picker.accept(&mut program);
            assert!(
                picker.take_follow_target().is_some(),
                "can place again without reselecting {kind:?}"
            );
        }
    }
}

#[test]
fn closing_an_end_or_replacing_the_document_does_not_move_the_camera() {
    let mut program = crate::parse("road street: carlane carlane; end street(0,0,1,0);").unwrap();
    let mut picker = Picker::default();
    picker.start();
    picker.refresh(&program);
    picker.choose_end(&program, 1);
    picker.selected = picker
        .proposals
        .iter()
        .position(|p| p.kind == expansion::Kind::Stop)
        .unwrap();
    picker.accept(&mut program);
    assert!(picker.take_follow_target().is_none());
    assert!(picker.selected().is_none());
    picker.choose_end(&program, 0);
    picker.accept(&mut program);
    let replacement = crate::parse("road other: carlane; end other(0,0,0,1);").unwrap();
    picker.refresh(&replacement);
    assert!(picker.take_follow_target().is_none());
}
