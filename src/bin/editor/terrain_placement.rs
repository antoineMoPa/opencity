//! Placement against projected terrain tops. Clearance deliberately lifts the
//! entire endpoint-linear grade; it is not a terrain-following road design.
use super::*;
use opencity::{road_geometry::RoadGeometry, LandscapeFeature};

fn terrain(program: &Program) -> Vec<LandscapeFeature> {
    project_landscape_features(program)
        .features
        .into_iter()
        .filter(|f| {
            f.kind == LandscapeKind::Terrain
                && f.elevation.is_finite()
                && f.points.len() >= 3
                && f.points.iter().flatten().all(|n| n.is_finite())
        })
        .collect()
}

fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f64 {
    (b[0] as f64 - a[0] as f64) * (c[1] as f64 - a[1] as f64)
        - (b[1] as f64 - a[1] as f64) * (c[0] as f64 - a[0] as f64)
}
fn on_edge(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> bool {
    cross(a, b, p).abs() <= 1e-10
        && (0..2).all(|i| p[i] >= a[i].min(b[i]) - 1e-7 && p[i] <= a[i].max(b[i]) + 1e-7)
}
fn inside(p: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    let mut hit = false;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        if on_edge(p, a, b) {
            return true;
        }
        if (a[1] > p[1]) != (b[1] > p[1])
            && (p[0] as f64)
                < (b[0] as f64 - a[0] as f64) * (p[1] as f64 - a[1] as f64)
                    / (b[1] as f64 - a[1] as f64)
                    + a[0] as f64
        {
            hit = !hit;
        }
    }
    hit
}
fn intersects(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> bool {
    if on_edge(a, c, d) || on_edge(b, c, d) || on_edge(c, a, b) || on_edge(d, a, b) {
        return true;
    }
    (cross(a, b, c) > 0.0) != (cross(a, b, d) > 0.0)
        && (cross(c, d, a) > 0.0) != (cross(c, d, b) > 0.0)
}
pub(super) fn overlaps(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    a.iter().any(|p| inside(*p, b))
        || b.iter().any(|p| inside(*p, a))
        || (0..a.len()).any(|i| {
            (0..b.len()).any(|j| intersects(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]))
        })
}

pub(super) fn height(program: &Program, point: WorldPoint) -> f32 {
    terrain(program)
        .iter()
        .filter(|f| inside([point.x, point.y], &f.points))
        .map(|f| f.elevation)
        .fold(0.0, f32::max)
}

/// Native standalone junctions have ground-level surfaces and ports.
pub(super) fn intersection_obstructed(program: &Program, point: WorldPoint, half: f32) -> bool {
    let square = [
        [point.x - half, point.y - half],
        [point.x + half, point.y - half],
        [point.x + half, point.y + half],
        [point.x - half, point.y + half],
    ];
    terrain(program)
        .iter()
        .any(|f| f.elevation > 0.0 && overlaps(&square, &f.points))
}

pub(super) fn iso_pick(
    program: &Program,
    at_height: impl Fn(f32) -> WorldPoint,
) -> (WorldPoint, f32) {
    // Along an orthographic downward ray the highest intersected top is first.
    let mut picked = (at_height(0.0), 0.0);
    for f in terrain(program) {
        let p = at_height(f.elevation);
        if f.elevation > picked.1 && inside([p.x, p.y], &f.points) {
            picked = (p, f.elevation);
        }
    }
    picked
}

/// Preserve anchored endpoints. For a free road preserve the initial slope and
/// raise both ends conservatively above all terrain intersecting its complete
/// compiled surface. Polygon intersections detect even patches between rows.
pub(super) fn apply(
    program: &mut Program,
    source: SegmentSource,
    start: f32,
    end: f32,
    anchors: (bool, bool),
) -> Result<(), &'static str> {
    if !start.is_finite() || !end.is_finite() || start.abs().max(end.abs()) > 100_000.0 {
        return Err("Road height is outside the supported range");
    }
    let projection = map::project(program);
    let Some(index) = projection.segments.iter().position(|s| s.source == source) else {
        return Err("Road could not be projected");
    };
    let segment = &projection.segments[index];
    let features = terrain(program);
    // Keep the established connection/grade behavior when there is no terrain.
    if features.is_empty() {
        return Ok(());
    }
    let points = [
        segment.start,
        segment.controls[0],
        segment.controls[1],
        segment.end,
    ];
    if points
        .iter()
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > 100_000.0)
        || points
            .windows(2)
            .map(|p| (p[1].x - p[0].x).hypot(p[1].y - p[0].y))
            .sum::<f32>()
            > 1000.0
    {
        return Err("Road is outside the supported terrain placement range");
    }
    let Some(name) = segment.name.clone() else {
        return Err("Road requires a source name");
    };
    let start = if anchors.0 { segment.start_z } else { start };
    let end = if anchors.1 { segment.end_z } else { end };
    let start = if anchors.0 {
        start
    } else {
        (start * 10_000.0).ceil() / 10_000.0
    };
    let end = if anchors.1 {
        end
    } else {
        (end * 10_000.0).ceil() / 10_000.0
    };
    if (start != segment.start_z || end != segment.end_z) && !set_grade(program, &name, start, end)
    {
        return Err("Road grade could not be saved");
    }
    let projection = map::project(program);
    let Some((rows, resolved)) =
        RoadGeometry::placement_cross_sections(program, &projection, index)
    else {
        return Err("Road footprint could not be compiled");
    };
    if !resolved {
        return Err("Road curve is too complex for terrain clearance");
    }
    let mut lift = 0.0_f32;
    for rows in rows.windows(2) {
        for i in 0..rows[0]
            .boundaries
            .len()
            .min(rows[1].boundaries.len())
            .saturating_sub(1)
        {
            let (a, b, c, d) = (
                rows[0].boundaries[i],
                rows[0].boundaries[i + 1],
                rows[1].boundaries[i],
                rows[1].boundaries[i + 1],
            );
            for triangle in [[a, b, d], [a, d, c]] {
                let polygon = triangle.map(|v| [v.point.x, v.point.y]);
                if polygon.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("Road footprint is invalid");
                }
                let low = triangle
                    .iter()
                    .map(|v| v.height)
                    .fold(f32::INFINITY, f32::min);
                for feature in &features {
                    if feature.elevation > low && overlaps(&polygon, &feature.points) {
                        lift = lift.max(feature.elevation - low);
                    }
                }
            }
        }
    }
    if lift > 1e-6 {
        if anchors.0 || anchors.1 {
            return Err("Connected road would pass through terrain; choose another route");
        }
        let lift = (lift * 10_000.0).ceil() / 10_000.0;
        if !set_grade(program, &name, start + lift, end + lift) {
            return Err("Road clearance could not be saved");
        }
    }
    Ok(())
}

impl RoadEditor {
    pub(super) fn terrain_draw_preview(
        &self,
        projection: &map::Projection,
        rect: egui::Rect,
        pointer: egui::Pos2,
    ) -> Option<map::Segment> {
        let start = self.map_start?;
        let road = self.road()?.clone();
        let mut program = self
            .map_placement_draft
            .as_ref()
            .unwrap_or(&self.program)
            .clone();
        if !program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Road(r) if r.name == road.name))
        {
            program.statements.push(Statement::Road(road.clone()));
        }
        let (mut point, _) = iso_pick(&program, |height| {
            iso_world_at_height(
                rect,
                self.map_pan,
                self.map_zoom,
                self.orbit_target,
                self.orbit_angle,
                self.orbit_elevation,
                pointer,
                height,
            )
        });
        let mut reference = None;
        if let Some((source, end)) = self.draw_endpoint_near(projection, rect, pointer, true) {
            if let Some((p, r)) = map::link_endpoint(&mut program, source, end) {
                point = p;
                reference = Some(r);
            }
        } else if let Some(candidate) = iso_element_end_near(
            &self.draw_element_ends(projection, rect, true),
            projection,
            rect,
            self.map_pan,
            self.map_zoom,
            self.orbit_target,
            self.orbit_angle,
            self.orbit_elevation,
            pointer,
        ) {
            point = candidate.position;
            reference = map::create_lane_point(
                &mut program,
                candidate.source,
                candidate.element_index,
                candidate.t,
            )
            .map(|(r, _)| r);
        }
        if start == point {
            return None;
        }
        let (index, _) = map::append_connected(
            &mut program,
            &road,
            start,
            self.map_start_ref.as_deref(),
            point,
            reference.as_deref(),
        );
        let start_height = self
            .map_start_height
            .unwrap_or_else(|| height(&program, start));
        let end_height = height(&program, point);
        // An invalid anchored preview stays at its anchored grade; clicking it
        // reports why it cannot be committed without moving existing roads.
        let _ = apply(
            &mut program,
            SegmentSource::TopLevel(index),
            start_height,
            end_height,
            (self.map_start_ref.is_some(), reference.is_some()),
        );
        map::project(&program)
            .segments
            .into_iter()
            .find(|s| s.source == SegmentSource::TopLevel(index))
    }

    fn draw_surface_visible(&self, rect: egui::Rect, point: WorldPoint, z: f32, iso: bool) -> bool {
        let surface = if iso {
            let screen = iso_screen(
                rect,
                self.map_pan,
                self.map_zoom,
                self.orbit_target,
                self.orbit_angle,
                self.orbit_elevation,
                point,
                z,
            );
            iso_pick(&self.program, |height| {
                iso_world_at_height(
                    rect,
                    self.map_pan,
                    self.map_zoom,
                    self.orbit_target,
                    self.orbit_angle,
                    self.orbit_elevation,
                    screen,
                    height,
                )
            })
            .1
        } else {
            height(&self.program, point)
        };
        z + 1e-5 >= surface
    }

    pub(super) fn draw_endpoint_near(
        &self,
        projection: &map::Projection,
        rect: egui::Rect,
        pointer: egui::Pos2,
        iso: bool,
    ) -> Option<(SegmentSource, bool)> {
        projection
            .segments
            .iter()
            .filter(|s| s.source.editable())
            .flat_map(|s| {
                [
                    (false, s.start, s.start_z, s.start_ref.as_deref()),
                    (true, s.end, s.end_z, s.end_ref.as_deref()),
                ]
                .into_iter()
                .map(move |(end, p, z, r)| (s.source, end, p, z, r))
            })
            .filter(|(_, _, p, z, r)| {
                !r.is_some_and(|r| {
                    map::anchored_reference(&self.program, r)
                        && map::lane_attachment(&self.program, r).is_none()
                }) && self.draw_surface_visible(rect, *p, *z, iso)
            })
            .filter_map(|(source, end, p, z, _)| {
                let screen = if iso {
                    iso_screen(
                        rect,
                        self.map_pan,
                        self.map_zoom,
                        self.orbit_target,
                        self.orbit_angle,
                        self.orbit_elevation,
                        p,
                        z,
                    )
                } else {
                    screen_of(rect, self.map_pan, self.map_zoom, p)
                };
                let distance = pointer.distance_sq(screen);
                (distance <= 81.0).then_some((source, end, distance, z))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2).then_with(|| b.3.total_cmp(&a.3)))
            .map(|(source, end, _, _)| (source, end))
    }

    pub(super) fn draw_element_ends(
        &self,
        projection: &map::Projection,
        rect: egui::Rect,
        iso: bool,
    ) -> Vec<ElementEndCandidate> {
        element_end_candidates(projection, &self.program)
            .into_iter()
            .filter(|candidate| {
                projection
                    .segments
                    .iter()
                    .find(|s| s.source == candidate.source)
                    .is_some_and(|s| {
                        self.draw_surface_visible(
                            rect,
                            candidate.position,
                            s.height_at(candidate.t),
                            iso,
                        )
                    })
            })
            .collect()
    }

    pub(super) fn begin_road_click(&mut self) -> Option<Program> {
        if self
            .map_placement_before
            .as_ref()
            .is_some_and(|before| before != &self.program)
        {
            self.map_placement_before = None;
            self.map_placement_draft = None;
            self.map_start = None;
            self.map_start_ref = None;
            self.map_start_height = None;
            self.minimal.placement_error =
                Some("Map changed during placement; choose a new start".into());
            self.status = "Map changed during placement; choose a new start".into();
            return None;
        }
        if self.map_placement_before.is_none() {
            self.map_placement_before = Some(self.program.clone());
        }
        if let Some(draft) = self.map_placement_draft.take() {
            self.program = draft;
        }
        Some(self.program.clone())
    }

    pub(super) fn hold_road_draft(&mut self) {
        if let Some(before) = &self.map_placement_before {
            self.map_placement_draft = Some(std::mem::replace(&mut self.program, before.clone()));
        }
    }

    pub(super) fn rollback_road_placement(&mut self) {
        self.minimal.pending_road = None;
        self.minimal.placement_error = None;
        if let Some(before) = self.map_placement_before.take() {
            // Held drafts never enter the authoritative source between clicks.
            // Preserve newer external edits if the source changed meanwhile.
            if self.map_placement_draft.is_none() {
                self.program = before;
            }
        }
        self.map_placement_draft = None;
        self.map_start_height = None;
    }

    pub(super) fn finish_terrain_road(
        &mut self,
        index: usize,
        point: WorldPoint,
        point_ref: Option<&str>,
        pending: Option<RoadDefinition>,
        selected_road: Option<usize>,
        before_click: Program,
    ) -> bool {
        let start = self.map_start.unwrap();
        let start_height = self
            .map_start_height
            .unwrap_or_else(|| height(&self.program, start));
        let end_height = height(&self.program, point);
        let result = apply(&mut self.program, SegmentSource::TopLevel(index),
            start_height, end_height,
            (self.map_start_ref.is_some(), point_ref.is_some())).and_then(|()| {
                // A branch can add an entry and a remaining through segment.
                // Their fixed seams must also clear terrain, without lifting
                // either side of an existing connection.
                let additional: Vec<_> = map::project(&self.program).segments.into_iter().filter(|s|
                    matches!(s.source, SegmentSource::TopLevel(i) if i >= before_click.statements.len() && i != index))
                    .collect();
                for segment in additional {
                    apply(&mut self.program,segment.source,segment.start_z,segment.end_z,(true,true))?;
                }
                Ok(())
            });
        if let Err(message) = result {
            self.program = before_click;
            self.minimal.pending_road = pending;
            self.selected_road = selected_road;
            self.status = message.into();
            self.minimal.placement_error = Some(message.into());
            self.hold_road_draft();
            return false;
        }
        if let Some(before) = self.map_placement_before.take() {
            self.undo_stack.push(before);
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
            self.skip_history = true;
        }
        self.map_start_height = None;
        self.map_placement_draft = None;
        self.minimal.placement_error = None;
        true
    }
}

#[cfg(test)]
#[path = "terrain_placement/tests.rs"]
mod tests;
