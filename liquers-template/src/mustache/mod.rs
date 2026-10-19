//! The Mustache syntax: AST, lexer and parser, encoder and renderer.

pub mod ast;
pub mod encode;
pub(crate) mod lexer;
pub mod parse;
pub(crate) mod render;
