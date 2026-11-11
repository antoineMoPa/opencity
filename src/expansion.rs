//! Deterministic road-end proposals. Accepted edits remain ordinary `.road` calls.
mod obstacles;
use crate::{
    map::{self, Segment, SegmentSource, WorldPoint},
    *,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Stop,
    Continue,
    BranchLeft,
    BranchRight,
    CurveLeft,
    CurveRight,
    Rise,
    Fall,
    AddElement,
    RemoveElement,
    SplitLeft,
    SplitRight,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stop => "End here",
            Self::Continue => "Continue",
            Self::BranchLeft => "Branch left",
            Self::BranchRight => "Branch right",
            Self::CurveLeft => "Curve left",
            Self::CurveRight => "Curve right",
            Self::Rise => "Go up",
            Self::Fall => "Go down",
            Self::AddElement => "Start element",
            Self::RemoveElement => "End element",
            Self::SplitLeft => "Split left lane",
            Self::SplitRight => "Split right lane",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    Road { source: SegmentSource, at_end: bool },
    IntersectionPort(String),
}
#[derive(Clone, Debug)]
pub struct End {
    pub anchor: Anchor,
    pub at_end: bool,
    pub point: WorldPoint,
    pub height: f32,
    pub heading: f32,
    pub road: RoadDefinition,
}
#[derive(Clone)]
pub struct Proposal {
    pub kind: Kind,
    pub end: End,
    pub ghosts: Vec<Segment>,
    before: Program,
    after: Program,
}
impl Proposal {
    pub fn ghost_width(&self, segment: &Segment) -> f32 {
        definition(&self.after, segment).map(width).unwrap_or(0.)
    }
    /// A stale preview cannot overwrite edits made since it was generated.
    pub fn apply(&self, program: &mut Program) -> Result<(), String> {
        if *program != self.before {
            return Err("Map changed; refresh the possibilities".into());
        }
        *program = self.after.clone();
        Ok(())
    }
}
fn distance(a: WorldPoint, b: WorldPoint) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn width(road: &RoadDefinition) -> f32 {
    road.elements
        .iter()
        .map(|e| map::element_world_width(e.kind))
        .sum()
}
fn definition<'a>(program: &'a Program, segment: &Segment) -> Option<&'a RoadDefinition> {
    program.statements.iter().find_map(|s| match s {
        Statement::Road(r) if r.name == segment.road_name => Some(r),
        _ => None,
    })
}
fn stopped(program: &Program, reference: Option<&str>) -> bool {
    reference.is_some_and(|reference| {
        program.statements.iter().any(|s| {
            matches!(s, Statement::Call(c) if c.name == "expansion_stop"
                && c.args.first() == Some(&Argument::Identifier(reference.into())))
        })
    })
}
pub fn ends(program: &Program) -> Vec<End> {
    let projection = map::project(program);
    let mut result = Vec::new();
    for segment in &projection.segments {
        if !segment.source.editable() {
            continue;
        }
        let Some(road) = definition(program, segment) else {
            continue;
        };
        if !road.elements.iter().any(|e| e.kind.is_travel_lane()) {
            continue;
        }
        for at_end in [false, true] {
            let (point, height, reference, tangent) = if at_end {
                (
                    segment.end,
                    segment.end_z,
                    segment.end_ref.as_deref(),
                    segment.tangent_at(1.),
                )
            } else {
                let t = segment.tangent_at(0.);
                (
                    segment.start,
                    segment.start_z,
                    segment.start_ref.as_deref(),
                    WorldPoint { x: -t.x, y: -t.y },
                )
            };
            if stopped(program, reference) || tangent.x.hypot(tangent.y) < 0.00001 {
                continue;
            }
            let connected = projection.segments.iter().any(|other| {
                other.source != segment.source
                    && ((distance(point, other.start) < 0.001
                        && (height - other.start_z).abs() < 0.001)
                        || (distance(point, other.end) < 0.001
                            && (height - other.end_z).abs() < 0.001))
            });
            let attached = reference.is_some_and(|r| {
                map::lane_attachment(program, r).is_some()
                    || map::intersection_port_direction(program, r).is_some()
            });
            if !connected && !attached {
                result.push(End {
                    anchor: Anchor::Road {
                        source: segment.source,
                        at_end,
                    },
                    at_end,
                    point,
                    height,
                    heading: tangent.y.atan2(tangent.x),
                    road: road.clone(),
                });
            }
        }
    }
    for junction in map::intersections(program, &projection) {
        for port in &junction.ports {
            if stopped(program, Some(&port.point_ref))
                || map::port_in_use(&projection, &port.point_ref)
            {
                continue;
            }
            let name = junction.road_name_for_direction(port.direction);
            let Some(road) = program.statements.iter().find_map(|s| match s {
                Statement::Road(r) if r.name == name => Some(r.clone()),
                _ => None,
            }) else {
                continue;
            };
            result.push(End {
                anchor: Anchor::IntersectionPort(port.point_ref.clone()),
                at_end: true,
                point: port.position,
                height: crate::elevation_of(program, &junction.point_ref).unwrap_or(0.),
                heading: port.direction.y.atan2(port.direction.x),
                road,
            });
        }
    }
    result
}
fn unique(program: &Program, prefix: &str) -> String {
    (1..)
        .map(|i| format!("{prefix}_{i}"))
        .find(|name| {
            !program.statements.iter().any(|s| match s {
                Statement::Road(r) => r.name == *name,
                Statement::Block(b) => b.name == *name,
                Statement::Network(n) => n.name.as_ref() == Some(name),
                Statement::Call(c) => c.assigns.contains(name),
            })
        })
        .unwrap()
}
fn shifted(point: WorldPoint, heading: f32, length: f32) -> WorldPoint {
    WorldPoint {
        x: point.x + heading.cos() * length,
        y: point.y + heading.sin() * length,
    }
}
fn add(
    program: &mut Program,
    end: &End,
    road: &RoadDefinition,
    reference: &str,
    length: f32,
    angle: f32,
    dz: f32,
) {
    let finish = shifted(end.point, end.heading + angle / 2., length);
    let first = shifted(end.point, end.heading, length / 3.);
    let second = shifted(finish, end.heading + angle, -length / 3.);
    // Preserve point order at the starting end, including travel and cross-section sides.
    let (index, _) = if end.at_end {
        map::append_connected_curve(
            program,
            road,
            end.point,
            Some(reference),
            first,
            second,
            finish,
            None,
        )
    } else {
        map::append_connected_curve(
            program,
            road,
            finish,
            None,
            second,
            first,
            end.point,
            Some(reference),
        )
    };
    let Statement::Call(c) = &program.statements[index] else {
        unreachable!()
    };
    let name = c.assigns[0].clone();
    let (a, b) = if end.at_end {
        (end.height, end.height + dz)
    } else {
        (end.height + dz, end.height)
    };
    set_grade(program, &name, a, b);
}
/// Length is in map units; turns are finite alternatives, never random geometry.
pub fn possibilities(program: &Program, end: &End, length: f32) -> Vec<Proposal> {
    possibilities_with_shape(program, end, length, 0.8, 0.04)
}

/// Curve angle is in radians and grade is height gained per map unit.
pub fn possibilities_with_shape(
    program: &Program,
    end: &End,
    length: f32,
    curve: f32,
    grade: f32,
) -> Vec<Proposal> {
    propose(
        program,
        end,
        Options {
            length,
            curve,
            grade,
            ..Default::default()
        },
    )
}

#[derive(Clone, Copy)]
pub struct Options {
    pub length: f32,
    pub curve: f32,
    pub grade: f32,
    pub element: RoadElementKind,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            length: 1.,
            curve: 0.8,
            grade: 0.04,
            element: RoadElementKind::CarLane,
        }
    }
}
pub fn propose(program: &Program, end: &End, options: Options) -> Vec<Proposal> {
    let Options {
        length,
        curve,
        grade,
        element,
    } = options;
    if !curve.is_finite()
        || !(0.05..=2.6).contains(&curve)
        || !grade.is_finite()
        || !(0.005..=0.1).contains(&grade)
    {
        return vec![];
    }
    let Some(current) = ends(program).into_iter().find(|e| e.anchor == end.anchor) else {
        return vec![];
    };
    let end = &current;
    if !length.is_finite() || length <= 0. || length > 100. {
        return vec![];
    }
    let original = map::project(program);
    let obstacles = obstacles::Obstacles::new(program);
    let mut result = Vec::new();
    for kind in [
        Kind::Stop,
        Kind::Continue,
        Kind::BranchLeft,
        Kind::BranchRight,
        Kind::CurveLeft,
        Kind::CurveRight,
        Kind::Rise,
        Kind::Fall,
        Kind::AddElement,
        Kind::RemoveElement,
        Kind::SplitLeft,
        Kind::SplitRight,
    ] {
        if kind != Kind::Stop && length <= width(&end.road) * 2. {
            continue;
        }
        let mut after = program.clone();
        let reference = match &end.anchor {
            Anchor::Road { source, at_end } => {
                let Some((_, r)) = map::link_endpoint(&mut after, *source, *at_end) else {
                    continue;
                };
                r
            }
            Anchor::IntersectionPort(r) => r.clone(),
        };
        let mut road = end.road.clone();
        if matches!(kind, Kind::AddElement | Kind::RemoveElement) {
            if kind == Kind::AddElement {
                let at = road
                    .elements
                    .iter()
                    .rposition(|e| e.kind == element)
                    .or_else(|| road.elements.iter().rposition(|e| e.kind.is_travel_lane()))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                road.elements.insert(
                    at,
                    RoadElement {
                        kind: element,
                        catch: None,
                    },
                );
            } else {
                if element.is_travel_lane()
                    && road
                        .elements
                        .iter()
                        .filter(|e| e.kind.is_travel_lane())
                        .count()
                        < 2
                {
                    continue;
                }
                let Some(at) = road.elements.iter().rposition(|e| e.kind == element) else {
                    continue;
                };
                road.elements.remove(at);
            }
            road.name = unique(&after, "expanded_profile");
            after.statements.push(Statement::Road(road.clone()));
        }
        let before_count = map::project(&after).segments.len();
        match kind {
            Kind::Stop => after.statements.push(Statement::Call(Call {
                assigns: vec![],
                name: "expansion_stop".into(),
                args: vec![Argument::Identifier(reference)],
            })),
            Kind::BranchLeft | Kind::BranchRight => {
                add(&mut after, end, &road, &reference, length, 0., 0.);
                add(
                    &mut after,
                    end,
                    &road,
                    &reference,
                    length,
                    if kind == Kind::BranchLeft {
                        1.57
                    } else {
                        -1.57
                    },
                    0.,
                );
            }
            Kind::SplitLeft | Kind::SplitRight => {
                // Author lane attachments and profile transitions with the same API as the editor.
                let lanes: Vec<_> = road
                    .elements
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.kind.is_travel_lane())
                    .map(|(i, _)| i)
                    .collect();
                if lanes.len() < 2 || !end.at_end {
                    continue;
                }
                add(&mut after, end, &road, &reference, length, 0., 0.);
                let projection = map::project(&after);
                let parent = &projection.segments[before_count];
                let lane = if kind == Kind::SplitLeft {
                    lanes[0]
                } else {
                    *lanes.last().unwrap()
                };
                let Some((attachment, start)) =
                    map::create_lane_point(&mut after, parent.source, lane, 0.35)
                else {
                    continue;
                };
                if map::append_exit(
                    &mut after,
                    &road,
                    map::SplitMode::Dedicated,
                    start,
                    &attachment,
                    shifted(
                        end.point,
                        end.heading + if kind == Kind::SplitLeft { 0.6 } else { -0.6 },
                        length,
                    ),
                    None,
                )
                .is_none()
                {
                    continue;
                }
            }
            _ => add(
                &mut after,
                end,
                &road,
                &reference,
                length,
                match kind {
                    Kind::CurveLeft => curve,
                    Kind::CurveRight => -curve,
                    _ => 0.,
                },
                match kind {
                    Kind::Rise => length * grade,
                    Kind::Fall => -length * grade,
                    _ => 0.,
                },
            ),
        }
        let ghosts: Vec<_> = map::project(&after)
            .segments
            .into_iter()
            .filter(|s| !original.segments.iter().any(|o| o.source == s.source))
            .collect();
        if kind == Kind::Stop || !obstacles.clashes(&after, &ghosts, end) {
            result.push(Proposal {
                kind,
                end: end.clone(),
                ghosts,
                before: program.clone(),
                after,
            });
        }
    }
    result
}
fn point_line(p: WorldPoint, a: WorldPoint, b: WorldPoint) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let d = dx * dx + dy * dy;
    let t = if d > 0. {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / d
    } else {
        0.
    }
    .clamp(0., 1.);
    distance(
        p,
        WorldPoint {
            x: a.x + dx * t,
            y: a.y + dy * t,
        },
    )
}
/// Bounded, repeatable experiment. Each accepted step recalculates open ends and obstacles.
pub fn grow(program: &mut Program, seed: u64, steps: usize, length: f32) -> usize {
    grow_with_options(
        program,
        seed,
        steps,
        Options {
            length,
            ..Default::default()
        },
    )
}
pub fn grow_with_options(
    program: &mut Program,
    seed: u64,
    steps: usize,
    options: Options,
) -> usize {
    let mut state = seed;
    let mut accepted = 0;
    for _ in 0..steps.min(128) {
        let ends = ends(program);
        if ends.is_empty() {
            break;
        }
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let first = (state >> 32) as usize % ends.len();
        let mut grew = false;
        for offset in 0..ends.len() {
            let choices = propose(program, &ends[(first + offset) % ends.len()], options);
            let choices: Vec<_> = choices.iter().filter(|p| p.kind != Kind::Stop).collect();
            if choices.is_empty() {
                continue;
            }
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            if choices[(state >> 32) as usize % choices.len()]
                .apply(program)
                .is_ok()
            {
                accepted += 1;
                grew = true;
                break;
            }
        }
        if !grew {
            break;
        }
    }
    accepted
}

/// Preserve comments and untouched formatting when the game accepts a proposal.
/// Expansion changes road calls and appends definitions/calls.
pub fn edited_source(source: &str, before: &Program, after: &Program) -> Result<String, String> {
    if parse(source).map_err(|e| e.to_string())? != *before
        || after.statements.len() < before.statements.len()
    {
        return Err("Map source changed".into());
    }
    let tokens = tokenize(source).map_err(|e| e.to_string())?;
    let mut replacements = Vec::new();
    for (index, (old, new)) in before.statements.iter().zip(&after.statements).enumerate() {
        if old == new {
            continue;
        }
        let Statement::Call(call) = old else {
            return Err("Expansion cannot rewrite a definition".into());
        };
        let name = call.assigns.first().unwrap_or(&call.name);
        let mut found = None;
        for (i, token) in tokens.iter().enumerate().filter(|(_, t)| t.text == *name) {
            let Some(end) = tokens[i..].iter().find(|t| t.kind == TokenKind::Semicolon) else {
                continue;
            };
            let range = token.span.offset..end.span.offset + 1;
            if parse(&source[..range.start])
                .ok()
                .is_some_and(|p| p.statements.len() == index)
                && parse(&source[range.clone()])
                    .ok()
                    .is_some_and(|p| p.statements == vec![old.clone()])
            {
                found = Some(range);
                break;
            }
        }
        let range = found.ok_or("Could not locate the authored road call")?;
        let replacement = Program {
            statements: vec![new.clone()],
        }
        .to_road_source();
        replacements.push((range, replacement.trim().to_owned()));
    }
    replacements.sort_by_key(|(r, _)| r.start);
    let mut output = source.to_owned();
    for (range, value) in replacements.into_iter().rev() {
        output.replace_range(range, &value);
    }
    let appended = Program {
        statements: after.statements[before.statements.len()..].to_vec(),
    }
    .to_road_source();
    output.push('\n');
    output.push_str(&appended);
    if parse(&output).map_err(|e| e.to_string())? != *after {
        return Err("Expansion source validation failed".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seed() -> Program {
        parse("road street: sidewalk carlane middle carlane sidewalk; end a = street(0, 0, 1, 0);")
            .unwrap()
    }
    fn end(program: &Program) -> End {
        ends(program).into_iter().find(|e| e.at_end).unwrap()
    }
    #[test]
    fn seven_families_round_trip_and_continue_from_both_ends() {
        let program = seed();
        let choices = possibilities(&program, &end(&program), 1.);
        for kind in [
            Kind::Stop,
            Kind::Continue,
            Kind::BranchLeft,
            Kind::BranchRight,
            Kind::CurveLeft,
            Kind::CurveRight,
            Kind::Rise,
            Kind::Fall,
            Kind::AddElement,
            Kind::RemoveElement,
            Kind::SplitLeft,
            Kind::SplitRight,
        ] {
            let p = choices
                .iter()
                .find(|p| p.kind == kind)
                .unwrap_or_else(|| panic!("missing {kind:?}"));
            let mut next = program.clone();
            p.apply(&mut next).unwrap();
            assert_eq!(parse(&next.to_road_source()).unwrap(), next);
            assert_eq!(map::project(&next).unresolved, 0);
            #[cfg(feature = "game-runtime")]
            crate::runtime::City::from_program(&next, None).unwrap();
            if kind == Kind::Rise {
                assert!((p.ghosts[0].start_z - p.end.height).abs() < 1e-5);
                assert!(p.ghosts[0].end_z > p.end.height);
            }
        }
        let start = ends(&program).into_iter().find(|e| !e.at_end).unwrap();
        let p = possibilities(&program, &start, 1.)
            .into_iter()
            .find(|p| p.kind == Kind::Continue)
            .unwrap();
        assert!(p.ghosts[0].start.x < 0.);
        assert_eq!(p.ghosts[0].end, start.point);
        assert_eq!(
            p.ghosts[0].direction,
            program
                .statements
                .iter()
                .find_map(|s| if let Statement::Road(r) = s {
                    Some(r.direction)
                } else {
                    None
                })
                .unwrap()
        );
    }
    #[test]
    fn stop_is_persistent_and_preview_rejects_stale_edits() {
        let mut program = seed();
        let choices = possibilities(&program, &end(&program), 1.);
        choices
            .iter()
            .find(|p| p.kind == Kind::Stop)
            .unwrap()
            .apply(&mut program)
            .unwrap();
        assert_eq!(ends(&parse(&program.to_road_source()).unwrap()).len(), 1);
        assert!(choices
            .iter()
            .find(|p| p.kind == Kind::Continue)
            .unwrap()
            .apply(&mut program)
            .is_err());
    }
    #[test]
    fn pruning_checks_interiors_and_height_clearance() {
        let mut program = seed();
        program.statements.extend(
            parse("obstacle = street(1.5, -1, 1.5, 1);")
                .unwrap()
                .statements,
        );
        assert!(!possibilities(&program, &end(&program), 1.)
            .iter()
            .any(|p| p.kind == Kind::Continue));
        set_grade(&mut program, "obstacle", 0.2, 0.2);
        assert!(possibilities(&program, &end(&program), 1.)
            .iter()
            .any(|p| p.kind == Kind::Continue));
        for obstacle in [
            "building(0.3, 1.4,-0.3, 1.6,-0.3, 1.6,0.3, 1.4,0.3);",
            "lake(1.4,-0.3, 1.6,-0.3, 1.6,0.3, 1.4,0.3);",
        ] {
            let mut program = seed();
            program
                .statements
                .extend(parse(obstacle).unwrap().statements);
            assert!(!possibilities(&program, &end(&program), 1.)
                .iter()
                .any(|p| p.kind == Kind::Continue));
        }
    }
    #[test]
    fn source_edits_preserve_comments_and_unchanged_text() {
        let source = "# seed\nroad street: sidewalk carlane middle carlane sidewalk; end\n\
                      # keep this\na = street(0, 0, 1, 0); # end note\n";
        let before = parse(source).unwrap();
        let mut after = before.clone();
        possibilities(&before, &end(&before), 1.)
            .iter()
            .find(|p| p.kind == Kind::Continue)
            .unwrap()
            .apply(&mut after)
            .unwrap();
        let output = edited_source(source, &before, &after).unwrap();
        assert!(output.starts_with(
            "# seed\nroad street: sidewalk carlane middle carlane sidewalk; end\n# keep this\n"
        ));
        assert!(output.contains("# end note"));
        assert_eq!(parse(&output).unwrap(), after);
        assert!(edited_source("", &before, &after).is_err());
    }
    #[test]
    fn bounded_repeatable_growth_from_intersection() {
        let mut a = parse(include_str!("../examples/expansion-seed.road")).unwrap();
        let mut b = a.clone();
        assert!(grow(&mut a, 17, 6, 1.) > 0);
        grow(&mut b, 17, 6, 1.);
        assert_eq!(a, b);
        assert_eq!(map::project(&a).unresolved, 0);
        assert!(possibilities(&a, &end(&a), f32::NAN).is_empty());
    }
    #[test]
    fn custom_curves_and_grades_preserve_join_height_and_heading() {
        let program = seed();
        let end = end(&program);
        let choices = possibilities_with_shape(&program, &end, 2., 1.2, 0.08);
        let rise = choices.iter().find(|p| p.kind == Kind::Rise).unwrap();
        assert!((rise.ghosts[0].end_z - end.height - 0.16).abs() < 0.0001);
        let curve = choices.iter().find(|p| p.kind == Kind::CurveLeft).unwrap();
        let tangent = curve.ghosts[0].tangent_at(1.);
        assert!((tangent.y.atan2(tangent.x) - end.heading - 1.2).abs() < 0.001);
        assert!(possibilities_with_shape(&program, &end, 1., f32::NAN, 0.04).is_empty());
        let mut blocked = program.clone();
        blocked
            .statements
            .extend(parse("barrier=street(1.02,-1,1.02,1);").unwrap().statements);
        assert!(!possibilities(&blocked, &end, 1.)
            .iter()
            .any(|p| p.kind == Kind::Continue));
    }
    #[test]
    fn unnamed_road_source_can_be_extended_without_rewriting_blocks() {
        let source = "road street: carlane; end\n\
            block saved: street(0,0,1,0); end\n# leave the block intact\nstreet(0,0,1,0);";
        let before = parse(source).unwrap();
        let mut after = before.clone();
        possibilities(&before, &end(&before), 1.)
            .iter()
            .find(|p| p.kind == Kind::Continue)
            .unwrap()
            .apply(&mut after)
            .unwrap();
        let output = edited_source(source, &before, &after).unwrap();
        assert!(output.contains("block saved: street(0,0,1,0); end"));
        assert_eq!(parse(&output).unwrap(), after);
    }
    #[test]
    fn non_lane_elements_can_start_and_end_with_the_existing_taper_system() {
        let program = seed();
        let end = end(&program);
        let choices = propose(
            &program,
            &end,
            Options {
                element: RoadElementKind::Sidewalk,
                ..Default::default()
            },
        );
        for kind in [Kind::AddElement, Kind::RemoveElement] {
            let proposal = choices.iter().find(|p| p.kind == kind).unwrap();
            let mut next = program.clone();
            proposal.apply(&mut next).unwrap();
            assert_eq!(map::project(&next).unresolved, 0);
            #[cfg(feature = "game-runtime")]
            crate::runtime::City::from_program(&next, None).unwrap();
        }
    }
}
