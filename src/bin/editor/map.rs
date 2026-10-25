//! Roads use cubic Bézier control points. A junction call gives connected roads
//! a persistent shared endpoint; older numeric road calls remain readable.

use std::collections::{HashMap, HashSet};

use opencity::{
    Argument, Call, Program, RoadDefinition, RoadDirection, RoadElement, RoadElementKind, Statement,
};

#[path = "map/model.rs"]
mod model;
pub use model::{
    ElementEdit, ExitPlacement, ExitSide, LaneSplit, MapPoint, Projection, Segment, SegmentSource,
    SplitMode, WorldPoint,
};

#[path = "map/intersection_footprints.rs"]
mod intersection_footprints;
#[path = "map/intersections.rs"]
mod intersections;
pub use intersections::{
    intersection_arms, intersection_footprint_half, intersection_port_direction, intersections,
    place_intersection, port_faces_road, port_in_use, remove_placed_intersection, right_angle,
    rotate_intersection, set_intersection, translate_intersection, Intersection, IntersectionKind,
    IntersectionPort,
};

#[path = "map/projection.rs"]
mod projection;
pub use projection::project;
use projection::{line_controls, number, point_args};

#[path = "map/attachments.rs"]
mod attachments;
use attachments::element_direction;
pub use attachments::{
    anchored_reference, create_lane_point, create_lane_split, create_two_segment_split,
    element_position, element_world_width, lane_attachment, lane_splits, set_lane_attachment,
    set_split_mode, update_lane_attachments,
};

#[path = "map/placement.rs"]
mod placement;
pub use placement::{
    append, append_connected, append_connected_curve, attach_endpoint_to_port, link_endpoint,
    rotate_endpoint,
};
use placement::{
    attachment_direction, ensure_point, junction_direction, name_available, number_arg,
    placement_grade, reference_height,
};

#[path = "map/exits.rs"]
mod exits;
pub use exits::append_exit;

#[path = "map/editing.rs"]
mod editing;
use editing::set_control_direct;
pub use editing::{
    append_curve, prune_unused_points, remove, remove_unused_point, set_connected_endpoint,
    set_control, set_name, set_road_type,
};

#[cfg(test)]
#[path = "map/intersection_tests.rs"]
mod intersection_tests;

#[cfg(test)]
#[path = "map/exit_projection_tests.rs"]
mod exit_projection_tests;

#[cfg(test)]
#[path = "map/placement_tests.rs"]
mod placement_tests;
