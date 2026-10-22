//! Serves Liquers commands as MCP tools.
//!
//! A binary declares tool sets ([`ToolSetDecl`]): each tool is one encoded query whose last action
//! is the command it runs, and each set may add a query tool limited by an allow-list. The engine
//! derives argument schemas from command metadata, joins a key argument onto the query
//! structurally ([`fill_key`]), evaluates the call as a recipe and returns the value serialized in
//! the format the query's filename selects. It knows nothing about any particular command set.

pub mod decl;
pub mod schema;
pub mod server;

pub use decl::{KeyArgumentDecl, QueryToolDecl, ResourceDecl, ToolDecl, ToolSetDecl};
pub use schema::argument_schema;
pub use server::{check_allowed, fill_key, tool_result, McpServer, ToolOutput, ToolRegistry};

use liquers_core::context::Environment;
use liquers_core::error::Error;
use rmcp::ServiceExt;

/// Serves `server` over standard input and output until the client disconnects.
pub async fn serve_stdio<E: Environment>(server: McpServer<E>) -> Result<(), Error> {
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| Error::general_error(format!("MCP server failed to start: {e}")))?;
    running
        .waiting()
        .await
        .map_err(|e| Error::general_error(format!("MCP server stopped: {e}")))?;
    Ok(())
}
