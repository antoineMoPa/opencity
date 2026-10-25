//! Parse OpenCity definitions into an AST and write them as `.road` files.
//!
//! ```
//! use opencity::parse;
//!
//! let source = "road lane: sidewalk carlane; end lane(0, 0, 1, 0);";
//! let ast = parse(source)?;
//! let road_text = ast.to_road_source();
//! assert_eq!(parse(&road_text)?, ast);
//! # Ok::<(), opencity::Error>(())
//! ```
//!
//! Design checks can inspect a parsed road network without changing the source:
//!
//! ```
//! use opencity::{check_design_rules, parse};
//! let program = parse("road street: carlane; end street(0, 0, 1, 0);")?;
//! assert!(check_design_rules(&program).is_empty());
//! # Ok::<(), opencity::Error>(())
//! ```

mod ast;
mod blocks;
pub mod cli;
mod design_rules;
mod elevation;
mod error;
mod landscape;
mod lexer;
pub mod network;
mod parser;
mod patterns;
pub mod road_geometry;
pub mod road_profiles;
mod writer;

#[path = "bin/editor/map.rs"]
pub mod map;

extern crate self as opencity;
#[cfg(all(feature = "editor", target_arch = "wasm32"))]
#[path = "bin/editor.rs"]
mod web_editor;

pub use ast::{
    Argument, BlockDefinition, BlockParameter, BlockParameterType, Call, Program, RoadDefinition,
    RoadDirection, RoadElement, RoadElementKind, RoadRole, Statement,
};
pub use blocks::BlockInstance;
pub use design_rules::{
    check_design_rules, check_design_rules_with_projection, check_vehicle_lane_continuity,
    check_vehicle_lane_intrusions, DesignIssue, DesignRule,
};
pub use elevation::{
    block_height, elevation_in_calls, elevation_of, grade_in_calls, grade_of, set_elevation,
    set_grade,
};
pub use error::{Error, SyntaxError};
pub use landscape::{
    project_landscape_features, terrain_walls, LandscapeError, LandscapeFeature, LandscapeKind,
    LandscapeProjection, TerrainWall,
};
pub use lexer::{tokenize, Span, Token, TokenKind};
pub use parser::{parse, parse_road_file};
