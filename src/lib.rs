//! Parse OpenCity definitions into an AST and write them as `.road` files.
//!
//! ```
//! use opencity::parse;
//!
//! let source = "road lane: sidewalk-carlane; end; lane(0, 0, 1, 0);";
//! let ast = parse(source)?;
//! let road_text = ast.to_road_source();
//! assert_eq!(parse(&road_text)?, ast);
//! # Ok::<(), opencity::Error>(())
//! ```

mod ast;
mod elevation;
mod error;
mod landscape;
mod lexer;
mod parser;
mod writer;

pub use ast::{
    Argument, BlockDefinition, Call, Program, RoadDefinition, RoadElement, RoadElementKind,
    Statement,
};
pub use elevation::{
    block_height, elevation_in_calls, elevation_of, grade_in_calls, grade_of, set_elevation,
    set_grade,
};
pub use error::{Error, SyntaxError};
pub use landscape::{
    project_landscape_features, LandscapeError, LandscapeFeature, LandscapeKind,
    LandscapeProjection,
};
pub use lexer::{tokenize, Span, Token, TokenKind};
pub use parser::{parse, parse_road_file};
