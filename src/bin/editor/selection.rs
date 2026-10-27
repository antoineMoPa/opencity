//! Selection for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn copy_selection(&mut self) {
        let projection = self.projected_map();
        let sources: Vec<_> = if self.selected_segments.is_empty() {
            self.selected_segment.into_iter().collect()
        } else {
            self.selected_segments.clone()
        };
        self.clipboard = projection
            .segments
            .iter()
            .filter(|segment| sources.contains(&segment.source))
            .map(|segment| CopiedSegment {
                road_name: segment.road_name.clone(),
                name: segment.name.clone(),
                points: [
                    segment.start,
                    segment.controls[0],
                    segment.controls[1],
                    segment.end,
                ],
                start_z: segment.start_z,
                end_z: segment.end_z,
            })
            .collect();
        self.status = format!("Copied {} segment(s)", self.clipboard.len());
    }

    pub(super) fn paste_selection(&mut self) {
        let mut pasted = Vec::new();
        for segment in self.clipboard.clone() {
            let Some(road) = self
                .program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == segment.road_name => Some(road.clone()),
                    _ => None,
                })
            else {
                continue;
            };
            let [start, first, second, end] = segment.points.map(|point| WorldPoint {
                x: point.x + 1.0,
                y: point.y - 1.0,
            });
            let index = map::append_curve(&mut self.program, &road, start, first, second, end);
            let source = SegmentSource::TopLevel(index);
            if let Some(name) = segment.name {
                map::set_name(&mut self.program, source, format!("{name}_copy"));
            }
            if let Some(name) = map::project(&self.program)
                .segments
                .iter()
                .find(|item| item.source == source)
                .and_then(|item| item.name.clone())
            {
                set_grade(&mut self.program, &name, segment.start_z, segment.end_z);
            }
            pasted.push(source);
        }
        if let Some(source) = pasted.last().copied() {
            self.selected_segment = Some(source);
            self.focus_orbit_on_selection();
        }
        self.selected_segments = pasted;
        if !self.selected_segments.is_empty() {
            self.dirty = true;
        }
        self.status = format!("Pasted {} segment(s)", self.selected_segments.len());
    }

    pub(super) fn delete_selected_segments(&mut self) -> usize {
        let sources = if self.selected_segments.is_empty() {
            self.selected_segment.into_iter().collect::<Vec<_>>()
        } else {
            self.selected_segments.clone()
        };
        let calls: Vec<_> = sources
            .into_iter()
            .filter_map(|source| match source {
                SegmentSource::TopLevel(index) => match self.program.statements.get(index) {
                    Some(Statement::Call(call)) => Some(call.clone()),
                    _ => None,
                },
                SegmentSource::BlockInstance { .. } => None,
            })
            .collect();
        let mut removed = 0;
        for call in calls {
            let index = self.program.statements.iter().position(|statement| {
                matches!(statement,
                Statement::Call(current) if current == &call)
            });
            if let Some(index) = index {
                if map::remove(&mut self.program, SegmentSource::TopLevel(index)) {
                    removed += 1;
                }
            }
        }
        if removed > 0 {
            self.selected_segment = None;
            self.selected_segments.clear();
            self.selected_endpoint = None;
            self.segment_name_draft.clear();
            self.dirty = true;
            self.status = format!("Removed {removed} road placement(s)");
        }
        removed
    }

    pub(super) fn start_extension(&mut self, source: SegmentSource, is_end: bool) -> bool {
        let projection = map::project(&self.program);
        let Some(segment) = projection
            .segments
            .iter()
            .find(|segment| segment.source == source)
        else {
            return false;
        };
        let Some(road_index) = self.program.statements.iter().position(|statement|
            matches!(statement, Statement::Road(road) if road.name == segment.road_name)) else { return false };
        let Some((point, reference)) = map::link_endpoint(&mut self.program, source, is_end) else {
            return false;
        };
        self.selected_road = Some(road_index);
        self.road_name_draft = segment.road_name.clone();
        self.map_tool = MapTool::Draw;
        self.map_start = Some(point);
        self.map_start_ref = Some(reference);
        self.selected_endpoint = None;
        self.dirty = true;
        self.status = "Extending road".into();
        true
    }
}
