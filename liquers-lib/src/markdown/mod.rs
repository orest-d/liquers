//! Markdown navigation: the `ns-md` commands over the `liquers-markdown` block parser.
//!
//! The parser itself is the `liquers-markdown` crate, re-exported here. Reference:
//! `specs/reference/MARKDOWN.md`; design: `specs/design/markdown-navigation/`.

pub use liquers_markdown::*;

pub mod commands;

/// Register the `ns-md` commands. The caller must define `type CommandEnvironment = ...`.
#[macro_export]
macro_rules! register_markdown_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::markdown::commands::{links, outline, section, sections, table};

        register_command!($cr,
            fn outline(state) -> result
            namespace: "md"
            label: "Outline"
            doc: "The headings of a Markdown document as records: n, level, heading, address, number, path, slug, line, end_line, words. address is the section-number path that ns-md/section takes"
        )?;
        register_command!($cr,
            fn section(state, address: String, children: bool = true, heading: bool = true) -> result
            namespace: "md"
            label: "Section"
            doc: "The text of one section of a Markdown document. address: steps separated by '.', each a section number (1.2), heading text (phase~.2.solution; '..' is a literal dot) or '_' for any one section; '__' is an underscore. children=false drops the subsections, heading=false the heading line"
        )?;
        register_command!($cr,
            fn sections(state, address: String = "", level: i64 = 0) -> result
            namespace: "md"
            label: "Sections"
            doc: "The sections of a Markdown document (or of one section, by address) as records: the outline's fields and text; level=N keeps sections of that level"
        )?;
        register_command!($cr,
            fn links(state, address: String = "") -> result
            namespace: "md"
            label: "Links"
            doc: "The links and images of a Markdown document (or of one section) as records: kind, text, target, target_kind (relative, anchor, url, query), anchor, key (a relative target resolved against the input's key), section, line"
        )?;
        register_command!($cr,
            fn table(state, address: String, n: i64 = 1) -> result
            namespace: "md"
            label: "Table"
            doc: "The n-th Markdown table inside a section, as records, read as the records Markdown table format reads a table"
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}
