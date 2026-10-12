//! What Atlas knows of the libraries and frameworks code uses, as data: one table per kind of
//! fact. A reader looks a fact up here; it never tests a library's name in its own control flow.

/// Servers that register a function as a tool an agent may call, by a decorator on a value of
/// the server's class: `@server.tool`, `@server.resource(..)`, `@server.prompt`. By package and
/// class.
pub const TOOL_SERVERS: &[(&str, &str)] = &[("fastmcp", "FastMCP"), ("mcp", "FastMCP")];

/// The decorators of a tool server that register a function.
pub const TOOL_DECORATORS: &[&str] = &["tool", "resource", "prompt"];

/// The parameters a call to the web names what it asks for by, the first that a call's library
/// has: a request's URL, the Hub repository a download or an upload names (`repo_id`, and the
/// datasets library's `path`).
pub const ASKED_BY: &[&str] = &["url", "repo_id", "path"];
