//! Text spans of an editor tree (AC-10): which bytes of the query text are which element,
//! `ns` instruction, argument or variadic item, so the cursor in the text can jump to a field and
//! back. Derived from the parse positions of the query text; no core change.

use std::sync::Arc;

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::parse::parse_query;
use liquers_core::pipeline::{ElementKind, PipelineEditor, QueryEditor};
use liquers_core::query::{ActionParameter, ActionRequest, QuerySegment};

use super::tree::{NodePath, PathStep, SpanTarget, TextSpan};

const NS: &str = "ns";

/// The spans of `text`, whose tree was built from `editor` under `prefix`; `base` is the byte
/// offset of `text` inside the editor's whole text (non-zero inside a link). Returns no spans
/// when the text and the editor's pipeline do not line up (a realm header, an unparsable text),
/// so a jump is simply unavailable there.
pub(crate) fn spans_of(
    text: &str,
    editor: &PipelineEditor,
    prefix: &NodePath,
    base: usize,
    registry: &Arc<CommandMetadataRegistry>,
) -> Vec<TextSpan> {
    let Ok(query) = parse_query(text) else {
        return vec![];
    };
    let elements = &editor.pipeline().elements;
    let mut out = Vec::new();
    let mut i = 0usize;
    for segment in query.segments.iter() {
        match segment {
            QuerySegment::Resource(r) => {
                let start = r.position().offset;
                let Some(range) = matching(text, start, &[r.encode(), r.encode_with_header()]) else {
                    return vec![];
                };
                if !matches!(
                    elements.get(i).map(|e| &e.item),
                    Some(ElementKind::Resource { .. } | ElementKind::KeyValue(_) | ElementKind::Cwd(_))
                ) {
                    return vec![];
                }
                out.push(span(prefix.then(PathStep::Element(i)), SpanTarget::Element, range, base));
                i += 1;
            }
            QuerySegment::Transform(t) => {
                if let Some(h) = &t.header {
                    if !h.name.is_empty() || !h.parameters.is_empty() {
                        return vec![]; // a realm: the pipeline nests, spans are not derived
                    }
                }
                for action in t.query.iter() {
                    let Some(range) = matching(text, action.position.offset, &[action.encode()]) else {
                        return vec![];
                    };
                    if action.name == NS {
                        out.push(span(prefix.then(PathStep::Element(i)), SpanTarget::Namespaces, range, base));
                        continue;
                    }
                    let path = prefix.then(PathStep::Element(i));
                    match elements.get(i).map(|e| &e.item) {
                        Some(ElementKind::Action(_)) => {
                            out.push(span(path.clone(), SpanTarget::Element, range, base));
                            argument_spans(text, editor, i, action, &path, base, registry, &mut out);
                        }
                        Some(ElementKind::Quote | ElementKind::Volatile) => {
                            out.push(span(path, SpanTarget::Element, range, base));
                        }
                        Some(_) | None => return vec![],
                    }
                    i += 1;
                }
            }
        }
    }
    if i != elements.len() {
        return vec![];
    }
    out
}

fn span(path: NodePath, target: SpanTarget, range: std::ops::Range<usize>, base: usize) -> TextSpan {
    TextSpan { path, target, range: range.start + base..range.end + base }
}

/// The range starting at `start` whose text is one of `encoded`.
fn matching(text: &str, start: usize, encoded: &[String]) -> Option<std::ops::Range<usize>> {
    let rest = text.get(start..)?;
    encoded
        .iter()
        .filter(|e| !e.is_empty())
        .find(|e| rest.starts_with(e.as_str()))
        .map(|e| start..start + e.len())
}

#[allow(clippy::too_many_arguments)]
fn argument_spans(
    text: &str,
    editor: &PipelineEditor,
    at: usize,
    action: &ActionRequest,
    path: &NodePath,
    base: usize,
    registry: &Arc<CommandMetadataRegistry>,
    out: &mut Vec<TextSpan>,
) {
    let Ok(md) = editor.command_metadata(at) else {
        return;
    };
    let slots: Vec<_> = md.arguments.iter().filter(|a| !a.injected).collect();
    let variadic = slots.iter().position(|a| a.multiple);
    for (k, parameter) in action.parameters.iter().enumerate() {
        let (arg, item) = match variadic {
            Some(v) if k >= v => (slots[v].name.clone(), Some(k - v)),
            Some(_) | None => match slots.get(k) {
                Some(a) => (a.name.clone(), None),
                None => continue, // an excess parameter
            },
        };
        let start = parameter.position().offset;
        let Some(range) = matching(text, start, &[parameter.encode()]) else {
            continue;
        };
        out.push(span(path.clone(), SpanTarget::Argument { arg: arg.clone(), item }, range.clone(), base));
        if let ActionParameter::Link(link, _) = parameter {
            // The link's own tree hangs under the argument (`field_value`).
            let inner_text = link.encode();
            let inner_start = range.start + "~X~".len();
            if text.get(inner_start..).is_some_and(|t| t.starts_with(&inner_text)) {
                if let Ok(q) = parse_query(&inner_text) {
                    if let Ok(inner) = QueryEditor::from_query(&q, registry.clone()) {
                        let arg_path = path.then(PathStep::Argument(arg));
                        out.extend(spans_of(&inner_text, inner.editor(), &arg_path, base + inner_start, registry));
                    }
                }
            }
        }
    }
}
