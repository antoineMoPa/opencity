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
mod error;
mod lexer;
mod parser;
mod writer;

pub use ast::{
    Argument, BlockDefinition, Call, Program, RoadDefinition, RoadElement, RoadElementKind,
    Statement,
};
pub use error::{Error, SyntaxError};
pub use lexer::{tokenize, Span, Token, TokenKind};
pub use parser::{parse, parse_road_file};
