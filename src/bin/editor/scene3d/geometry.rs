//! World-space painter capture, road surfaces and junction seams.
use super::*;

pub(super) fn capture_geometry(
    ctx: &egui::Context,
    paint: impl FnOnce(&egui::Painter),
    height: impl Fn(WorldPoint) -> f32,
    scene: &mut Scene,
) {
    // The tessellated vertices are kept in world space and reused at every
    // zoom. A one-pixel egui feather here becomes a 24-pixel blur at 2400x.
    // Let the scene target resolve edge coverage in screen pixels instead.
    ctx.tessellation_options_mut(|options| {
        options.feathering = false;
        options.round_line_segments_to_pixels = false;
        options.round_rects_to_pixels = false;
    });
    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(GEOMETRY_RECT),
        ..Default::default()
    });
    let painter = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(GEOMETRY_RECT);
    paint(&painter);
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
    // A painter shape is one visual layer. Give later shapes a small depth
    // priority without moving their geometry: raising each tessellated
    // triangle made adjacent road facets visible as separate rectangles.
    for (layer, shape) in output.shapes.into_iter().enumerate() {
        for clipped in ctx.tessellate(vec![shape], 1.0) {
            let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive else {
                continue;
            };
            if mesh
                .vertices
                .iter()
                .any(|vertex| vertex.uv != egui::epaint::WHITE_UV)
            {
                continue;
            }
            let vertices: Vec<_> = mesh
                .vertices
                .iter()
                .map(|vertex| {
                    let world =
                        world_of(GEOMETRY_RECT, egui::Vec2::ZERO, GEOMETRY_SCALE, vertex.pos);
                    WorldVertex {
                        point: world,
                        height: height(world),
                        depth_bias: -(layer as f32) * 0.0003,
                        color: vertex.color,
                    }
                })
                .collect();
            scene.vertices.reserve(mesh.indices.len());
            scene
                .vertices
                .extend(mesh.indices.iter().map(|index| vertices[*index as usize]));
        }
    }
}

fn road_height(samples: &[WorldPoint; 17], heights: &[f32; 17], point: WorldPoint) -> f32 {
    let mut best = (f32::INFINITY, 0.0);
    for step in 0..16 {
        let a = samples[step];
        let b = samples[step + 1];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let along = (((point.x - a.x) * dx + (point.y - a.y) * dy)
            / (dx * dx + dy * dy).max(0.000001))
        .clamp(0.0, 1.0);
        let x = a.x + along * dx;
        let y = a.y + along * dy;
        let distance = (x - point.x).powi(2) + (y - point.y).powi(2);
        if distance < best.0 {
            best = (
                distance,
                heights[step] + (heights[step + 1] - heights[step]) * along,
            );
        }
    }
    best.1
}

pub(super) fn add_elevated_edges(
    scene: &mut Scene,
    geometry: &opencity::road_geometry::RoadGeometry,
    index: usize,
    color: Color32,
) {
    for wall in geometry.walls.iter().filter(|w| w.segment == index) {
        let prior = WorldVertex {
            point: wall.start.point,
            height: wall.start.height + PAVEMENT_OFFSET,
            depth_bias: 0.0,
            color,
        };
        let current = WorldVertex {
            point: wall.end.point,
            height: wall.end.height + PAVEMENT_OFFSET,
            depth_bias: 0.0,
            color,
        };
        let prior_bottom = WorldVertex {
            height: wall.start.height - 0.08,
            ..prior
        };
        let current_bottom = WorldVertex {
            height: wall.end.height - 0.08,
            ..current
        };
        scene.vertices.extend_from_slice(&[
            prior,
            current,
            current_bottom,
            prior,
            current_bottom,
            prior_bottom,
        ]);
    }
}

/// Render ordinary road bands and their edge/center markings from the same
/// sampled cross-sections as the sidewalls and geometry validator.
pub(super) fn add_resolved_road_surface(
    scene: &mut Scene,
    geometry: &opencity::road_geometry::RoadGeometry,
    index: usize,
    road: &RoadDefinition,
    profile: &PaintProfile,
    palette: EditorPalette,
    selected: bool,
) {
    let rows = geometry.cross_sections(index);
    let emit = |scene: &mut Scene,
                corners: [opencity::road_geometry::Vertex; 4],
                color: Color32,
                bias: f32| {
        for i in [0, 1, 2, 0, 2, 3] {
            let vertex = corners[i];
            scene.vertices.push(WorldVertex {
                point: vertex.point,
                height: vertex.height + PAVEMENT_OFFSET,
                depth_bias: bias,
                color,
            });
        }
    };
    let between =
        |a: opencity::road_geometry::Vertex, b: opencity::road_geometry::Vertex, t: f32| {
            opencity::road_geometry::Vertex {
                point: WorldPoint {
                    x: a.point.x + (b.point.x - a.point.x) * t,
                    y: a.point.y + (b.point.y - a.point.y) * t,
                },
                height: a.height + (b.height - a.height) * t,
            }
        };
    for pair in rows.windows(2) {
        for (band, element) in road.elements.iter().enumerate() {
            emit(
                scene,
                [
                    pair[0].boundaries[band],
                    pair[1].boundaries[band],
                    pair[1].boundaries[band + 1],
                    pair[0].boundaries[band + 1],
                ],
                kind_color(element.kind, palette.theme),
                0.0,
            );
        }
        let left = [pair[0].boundaries[0], pair[1].boundaries[0]];
        let right = [
            *pair[0].boundaries.last().unwrap(),
            *pair[1].boundaries.last().unwrap(),
        ];
        let width = (left[0].point.x - right[0].point.x)
            .hypot(left[0].point.y - right[0].point.y)
            .max(0.000001);
        let marking = (0.004 / width).min(0.25);
        // The center marking is a coplanar decal, with depth ordering only.
        emit(
            scene,
            [
                between(left[0], right[0], 0.5 - marking),
                between(left[1], right[1], 0.5 - marking),
                between(left[1], right[1], 0.5 + marking),
                between(left[0], right[0], 0.5 + marking),
            ],
            palette.map_centerline,
            -0.003,
        );
        for side in [0, 1] {
            let edge = if side == 0 { left } else { right };
            let midpoint = between(edge[0], edge[1], 0.5).point;
            let hidden = [profile.start_parent.as_ref(), profile.end_parent.as_ref()]
                .into_iter()
                .flatten()
                .any(|parent| {
                    screen_point_in_road(
                        screen_of(GEOMETRY_RECT, egui::Vec2::ZERO, GEOMETRY_SCALE, midpoint),
                        parent,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                    )
                });
            if hidden {
                continue;
            }
            let inset = if side == 0 { marking } else { 1.0 - marking };
            emit(
                scene,
                [
                    edge[0],
                    edge[1],
                    between(left[1], right[1], inset),
                    between(left[0], right[0], inset),
                ],
                if selected {
                    palette.map_selected
                } else {
                    palette.map_outline
                },
                -0.003,
            );
        }
    }
}

/// Preserve each connected road's height along its entire junction edge.
/// Between edges, inverse squared distance blends differing approach heights.
/// A junction without roads rests on the same zero-grade pavement plane.
pub(super) fn intersection_surface_height(
    point: WorldPoint,
    seams: &[(WorldPoint, WorldPoint, f32)],
) -> f32 {
    let mut weighted_height = 0.0;
    let mut total_weight = 0.0;
    for &(a, b, height) in seams {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length_squared = dx * dx + dy * dy;
        let t = if length_squared > 0.0 {
            (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let distance_squared = (point.x - a.x - dx * t).powi(2) + (point.y - a.y - dy * t).powi(2);
        if distance_squared < 1e-10 {
            return height + PAVEMENT_OFFSET;
        }
        let weight = distance_squared.recip();
        weighted_height += height * weight;
        total_weight += weight;
    }
    if total_weight > 0.0 {
        weighted_height / total_weight + PAVEMENT_OFFSET
    } else {
        PAVEMENT_OFFSET
    }
}

pub(super) fn append_intersection(
    scene: &mut Scene,
    ctx: &egui::Context,
    program: &Program,
    projection: &map::Projection,
    geometry: &opencity::road_geometry::RoadGeometry,
    intersection: &map::Intersection,
    palette: EditorPalette,
) {
    let mut seams = Vec::new();
    for (index, segment) in projection.segments.iter().enumerate() {
        for (reference, is_end) in [(&segment.start_ref, false), (&segment.end_ref, true)] {
            let Some(reference) = reference else { continue };
            if reference != &intersection.point_ref
                && !intersection
                    .ports
                    .iter()
                    .any(|port| &port.point_ref == reference)
            {
                continue;
            }
            let rows = geometry.cross_sections(index);
            let row = if is_end { rows.last() } else { rows.first() };
            if let Some(row) = row {
                if let (Some(a), Some(b)) = (row.boundaries.first(), row.boundaries.last()) {
                    seams.push((a.point, b.point, a.height));
                }
            }
        }
    }
    capture_geometry(
        ctx,
        |painter| {
            paint_intersection(
                painter,
                GEOMETRY_RECT,
                egui::Vec2::ZERO,
                GEOMETRY_SCALE,
                program,
                projection,
                intersection,
                palette,
            );
        },
        |point| intersection_surface_height(point, &seams),
        scene,
    );
}

pub(super) fn append_road(
    scene: &mut Scene,
    ctx: &egui::Context,
    program: &Program,
    projection: &map::Projection,
    profiles: &[PaintProfile],
    geometry: &opencity::road_geometry::RoadGeometry,
    index: usize,
    palette: EditorPalette,
    selected: &[SegmentSource],
) {
    let first_vertex = scene.vertices.len() as u32;
    let segment = &projection.segments[index];
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == segment.road_name => Some(road),
            _ => None,
        });
    let is_selected = selected.contains(&segment.source);
    let samples = std::array::from_fn(|step| segment.point_at(step as f32 / 16.0));
    let heights = std::array::from_fn(|step| segment.height_at(step as f32 / 16.0));
    let shared_surface = road.filter(|road| {
        !road.elements.iter().any(|element| {
            matches!(
                element.kind,
                RoadElementKind::TrainTrack
                    | RoadElementKind::TramTrack
                    | RoadElementKind::SubwayTrack
                    | RoadElementKind::Runway
                    | RoadElementKind::Taxiway
                    | RoadElementKind::Apron
            )
        })
    });
    if let Some(road) = shared_surface {
        add_resolved_road_surface(
            scene,
            geometry,
            index,
            road,
            &profiles[index],
            palette,
            is_selected,
        );
    } else {
        capture_geometry(
            ctx,
            |painter| {
                if let Some(road) = road.filter(|_| {
                    profiles[index].start_alignment.is_some()
                        || profiles[index].merge_alignment.is_some()
                        || !profiles[index].start_taper.is_empty()
                        || !profiles[index].end_taper.is_empty()
                }) {
                    paint_profiled_road(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        segment,
                        road,
                        is_selected,
                        palette,
                        &profiles[index],
                    );
                } else {
                    paint_road_segment(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        segment,
                        road,
                        is_selected,
                        palette,
                    );
                }
            },
            |point| {
                if segment.start_z == segment.end_z {
                    segment.start_z + PAVEMENT_OFFSET
                } else {
                    road_height(&samples, &heights, point) + PAVEMENT_OFFSET
                }
            },
            scene,
        );
    }
    // A declared merge replaces the receiving edge's pavement locally. Keep
    // that overlay ahead of the parent's band/outline layers in every camera.
    if road.is_some_and(|road| road.role == opencity::RoadRole::Merge) {
        for vertex in &mut scene.vertices[first_vertex as usize..] {
            vertex.depth_bias -= 0.02;
        }
    }
    add_elevated_edges(
        scene,
        geometry,
        index,
        palette.map_outline.gamma_multiply(0.75),
    );
    let end_vertex = scene.vertices.len() as u32;
    if first_vertex < end_vertex {
        let mut min = WorldPoint {
            x: f32::INFINITY,
            y: f32::INFINITY,
        };
        let mut max = WorldPoint {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        };
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        for vertex in &scene.vertices[first_vertex as usize..end_vertex as usize] {
            min.x = min.x.min(vertex.point.x);
            min.y = min.y.min(vertex.point.y);
            max.x = max.x.max(vertex.point.x);
            max.y = max.y.max(vertex.point.y);
            min_height = min_height.min(vertex.height);
            max_height = max_height.max(vertex.height);
        }
        scene.road_ranges.push(RoadDrawRange {
            vertices: first_vertex..end_vertex,
            min,
            max,
            min_height,
            max_height,
        });
        let span = scene.road_span.get_or_insert(first_vertex..end_vertex);
        span.end = end_vertex;
    }
}
