//! Rendering a Mustache template: the Mustache rules, query tags through the host, partials with a
//! cycle check and a depth cap, and the error modes.

use futures::StreamExt;
use liquers_core::data_node::{AnyNode, DataNode, NodeKind};
use liquers_core::error::Error;
use liquers_core::maybe_send::{BoxFuture, MaybeBoxed};
use liquers_core::parse::parse_query;
use liquers_core::query::{Position, Query};

use crate::host::{QueryRequest, TemplateHost};
use crate::mustache::ast::{
    ApplyTarget, ClassicPiece, ClassicQuery, Element, Escape, MustacheTemplate, PartialTag,
    PartialTarget, QueryBody, QueryTag, Section, SectionHead,
};
use crate::mustache::parse::parse_mustache;
use crate::name::{kind_name, resolve, Resolved};
use crate::render::{expand_error, html_escape, ErrorMode, RenderOptions};
use crate::stack::{ContextStack, FrameKind};

/// The partials being rendered: each key, and the stack depth it was entered at.
type PartialPath = Vec<(String, usize)>;

pub(crate) async fn render<H: TemplateHost>(
    template: &MustacheTemplate,
    stack: &ContextStack<'_>,
    host: &H,
    options: &RenderOptions,
) -> Result<String, Error> {
    match options.errors {
        ErrorMode::Strict => {
            if let Some(error) = template.errors().first() {
                return Err((*error).clone());
            }
        }
        ErrorMode::Expand(_) | ErrorMode::Silent => {}
    }
    let renderer = Renderer { host, options };
    let mut out = String::new();
    renderer
        .elements(&template.elements, stack, &Vec::new(), &mut out)
        .await?;
    Ok(out)
}

struct Renderer<'h, H: TemplateHost> {
    host: &'h H,
    options: &'h RenderOptions,
}

/// The text a value is written as: a scalar's text, JSON for anything else, nothing for missing.
async fn text_of(value: &Resolved<'_>) -> Result<String, Error> {
    match value {
        Resolved::Missing => Ok(String::new()),
        Resolved::Node(n) => sync_text(n.node()),
        Resolved::Any(a) => any_text(a).await,
    }
}

fn sync_text(node: &dyn DataNode) -> Result<String, Error> {
    match node.scalar() {
        Some(s) => Ok(s.to_text().into_owned()),
        None => match node.kind() {
            NodeKind::Opaque => Ok(String::new()),
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::List
            | NodeKind::Map => Ok(node.to_json()?.to_string()),
        },
    }
}

pub(crate) async fn any_text(node: &AnyNode) -> Result<String, Error> {
    match node {
        AnyNode::Sync(n) => sync_text(n.as_ref()),
        AnyNode::Async(n) => {
            if n.kind().is_scalar() {
                Ok(n.scalar().await?.map(|s| s.to_text().into_owned()).unwrap_or_default())
            } else {
                match n.kind() {
                    NodeKind::Opaque => Ok(String::new()),
                    NodeKind::Null
                    | NodeKind::Bool
                    | NodeKind::Integer
                    | NodeKind::Float
                    | NodeKind::Text
                    | NodeKind::List
                    | NodeKind::Map => Ok(n.to_json().await?.to_string()),
                }
            }
        }
    }
}

/// Mustache truthiness of a value that is not a list.
async fn truthy(value: &Resolved<'_>) -> Result<bool, Error> {
    Ok(match value {
        Resolved::Missing => false,
        Resolved::Node(n) => n.node().is_truthy(),
        Resolved::Any(AnyNode::Sync(n)) => n.is_truthy(),
        Resolved::Any(AnyNode::Async(n)) => {
            if n.kind().is_scalar() {
                n.scalar().await?.is_some_and(|s| s.is_truthy())
            } else {
                match n.kind() {
                    NodeKind::Opaque | NodeKind::Map => true,
                    NodeKind::Null => false,
                    NodeKind::Bool | NodeKind::Integer | NodeKind::Float | NodeKind::Text | NodeKind::List => true,
                }
            }
        }
    })
}

/// Each line of `text` prefixed with `indent`, a final line end excepted.
fn indent_lines(text: &str, indent: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_line_start = true;
    for c in text.chars() {
        if at_line_start {
            out.push_str(indent);
            at_line_start = false;
        }
        out.push(c);
        if c == '\n' {
            at_line_start = true;
        }
    }
    out
}

impl<'h, H: TemplateHost> Renderer<'h, H> {
    /// A recoverable error: fails the render in strict mode, else is written as the mode says and
    /// logged.
    fn recover(&self, error: Error, position: &Position, out: &mut String) -> Result<(), Error> {
        let error = if error.position.is_unknown() {
            error.with_position(position)
        } else {
            error
        };
        match self.options.errors {
            ErrorMode::Strict => Err(error),
            ErrorMode::Expand(format) => {
                self.host.warn(&error);
                out.push_str(&expand_error(&error, format, position));
                Ok(())
            }
            ErrorMode::Silent => {
                self.host.warn(&error);
                Ok(())
            }
        }
    }

    fn write(&self, text: &str, escape: Escape, out: &mut String) {
        match escape {
            Escape::Escaped if self.options.html_escape => out.push_str(&html_escape(text)),
            Escape::Escaped | Escape::Triple | Escape::Ampersand => out.push_str(text),
        }
    }

    fn elements<'a>(
        &'a self,
        elements: &'a [Element],
        stack: &'a ContextStack<'a>,
        path: &'a PartialPath,
        out: &'a mut String,
    ) -> BoxFuture<'a, Result<(), Error>> {
        async move {
            for element in elements {
                self.element(element, stack, path, out).await?;
            }
            Ok(())
        }
        .maybe_boxed()
    }

    async fn element(
        &self,
        element: &Element,
        stack: &ContextStack<'_>,
        path: &PartialPath,
        out: &mut String,
    ) -> Result<(), Error> {
        match element {
            Element::Text(t) => {
                out.push_str(&t.text);
                Ok(())
            }
            Element::Variable(v) => {
                let text = match resolve(&v.name, stack).await {
                    Ok(value) => text_of(&value).await,
                    Err(e) => Err(e),
                };
                match text {
                    Ok(text) => {
                        self.write(&text, v.escape, out);
                        Ok(())
                    }
                    Err(e) => self.recover(e, &v.position, out),
                }
            }
            Element::Query(q) => {
                let text = match self.query(q, stack).await {
                    Ok(value) => any_text(&value).await,
                    Err(e) => Err(e),
                };
                match text {
                    Ok(text) => {
                        self.write(&text, q.escape, out);
                        Ok(())
                    }
                    Err(e) => self.recover(e, &q.position, out),
                }
            }
            Element::Section(s) => self.section(s, stack, path, out).await,
            Element::Partial(p) => self.partial(p, stack, path, out).await,
            Element::Comment(_) | Element::SetDelimiter(_) => Ok(()),
            Element::Invalid(i) => self.recover(i.error.clone(), &i.position, out),
        }
    }

    async fn section(
        &self,
        section: &Section,
        stack: &ContextStack<'_>,
        path: &PartialPath,
        out: &mut String,
    ) -> Result<(), Error> {
        let value = match &section.head {
            SectionHead::Name(n) => resolve(n, stack).await,
            SectionHead::Query(q) => self.query(q, stack).await.map(Resolved::Any),
            SectionHead::Invalid(i) => Err(i.error.clone()),
        };
        let value = match value {
            Ok(value) => value,
            Err(e) => return self.recover(e, &section.position, out),
        };
        if value.kind() == NodeKind::List {
            return self.list_section(section, value, stack, path, out).await;
        }
        let is_truthy = match truthy(&value).await {
            Ok(t) => t,
            Err(e) => return self.recover(e, &section.position, out),
        };
        if section.inverted {
            if !is_truthy {
                self.elements(&section.elements, stack, path, out).await?;
            }
            return Ok(());
        }
        if is_truthy {
            let mut inner = stack.reborrow();
            match value {
                Resolved::Node(n) => inner.push_ref(n, FrameKind::Data),
                Resolved::Any(a) => inner.push(a, FrameKind::Data),
                Resolved::Missing => {}
            }
            self.elements(&section.elements, &inner, path, out).await?;
        }
        Ok(())
    }

    async fn list_section(
        &self,
        section: &Section,
        value: Resolved<'_>,
        stack: &ContextStack<'_>,
        path: &PartialPath,
        out: &mut String,
    ) -> Result<(), Error> {
        match &value {
            Resolved::Missing => Ok(()),
            Resolved::Node(n) => self.sync_list(section, n.node(), stack, path, out).await,
            Resolved::Any(AnyNode::Sync(n)) => self.sync_list(section, n.as_ref(), stack, path, out).await,
            Resolved::Any(AnyNode::Async(n)) => {
                let mut items = n.items();
                let mut empty = true;
                while let Some(item) = items.next().await {
                    let item = match item {
                        Ok(item) => item,
                        Err(e) => return self.recover(e, &section.position, out),
                    };
                    empty = false;
                    if section.inverted {
                        break;
                    }
                    let mut inner = stack.reborrow();
                    inner.push(item, FrameKind::Data);
                    self.elements(&section.elements, &inner, path, out).await?;
                }
                if section.inverted && empty {
                    self.elements(&section.elements, stack, path, out).await?;
                }
                Ok(())
            }
        }
    }

    async fn sync_list(
        &self,
        section: &Section,
        node: &dyn DataNode,
        stack: &ContextStack<'_>,
        path: &PartialPath,
        out: &mut String,
    ) -> Result<(), Error> {
        if section.inverted {
            let empty = match node.len() {
                Some(n) => n == 0,
                None => node.items().next().is_none(),
            };
            if empty {
                self.elements(&section.elements, stack, path, out).await?;
            }
            return Ok(());
        }
        for item in node.items() {
            let mut inner = stack.reborrow();
            inner.push_ref(item, FrameKind::Data);
            self.elements(&section.elements, &inner, path, out).await?;
        }
        Ok(())
    }

    async fn partial(
        &self,
        partial: &PartialTag,
        stack: &ContextStack<'_>,
        path: &PartialPath,
        out: &mut String,
    ) -> Result<(), Error> {
        let loaded = self.load_partial(partial, stack, path).await;
        let (key, template) = match loaded {
            Ok(loaded) => loaded,
            Err(e) => return self.recover(e, &partial.position, out),
        };
        let mut inner_path = path.clone();
        inner_path.push((key, stack.len()));
        self.elements(&template.elements, stack, &inner_path, out).await
    }

    async fn load_partial(
        &self,
        partial: &PartialTag,
        stack: &ContextStack<'_>,
        path: &PartialPath,
    ) -> Result<(String, MustacheTemplate), Error> {
        let (key, source) = match &partial.target {
            PartialTarget::Key(k) => {
                let key = k.encode();
                self.check_partial(&key, stack, path, &partial.position)?;
                (key, self.host.partial(k).await?)
            }
            PartialTarget::Query(q) => {
                let (value, query) = self.evaluate(q, stack).await?;
                let key = format!("$ {}", query.encode());
                self.check_partial(&key, stack, path, &partial.position)?;
                match value.kind() {
                    NodeKind::Text | NodeKind::Null => (key, any_text(&value).await?),
                    other @ (NodeKind::Bool
                    | NodeKind::Integer
                    | NodeKind::Float
                    | NodeKind::List
                    | NodeKind::Map
                    | NodeKind::Opaque) => {
                        return Err(Error::general_error(format!(
                            "a dynamic partial is the text of a template, not a {}",
                            kind_name(other)
                        ))
                        .with_position(&partial.position))
                    }
                }
            }
            PartialTarget::Invalid(i) => return Err(i.error.clone()),
        };
        let source = match &partial.standalone {
            Some(s) if !s.indent.is_empty() => indent_lines(&source, &s.indent),
            Some(_) | None => source,
        };
        let template = parse_mustache(&source)?;
        match self.options.errors {
            ErrorMode::Strict => {
                if let Some(error) = template.errors().first() {
                    return Err((*error).clone());
                }
            }
            ErrorMode::Expand(_) | ErrorMode::Silent => {}
        }
        Ok((key, template))
    }

    fn check_partial(
        &self,
        key: &str,
        stack: &ContextStack<'_>,
        path: &PartialPath,
        position: &Position,
    ) -> Result<(), Error> {
        // A partial that includes itself with no section in between can never stop; one that
        // recurses through a section stops when the data does, which the depth cap bounds.
        if path.iter().any(|(k, depth)| k == key && *depth == stack.len()) {
            return Err(Error::execution_error(format!(
                "the partial '{key}' includes itself"
            ))
            .with_position(position));
        }
        if path.len() >= self.options.max_partial_depth {
            return Err(Error::execution_error(format!(
                "partials nest deeper than {} levels at '{key}'",
                self.options.max_partial_depth
            ))
            .with_position(position));
        }
        Ok(())
    }

    async fn build_query(
        &self,
        tag: &QueryTag,
        stack: &ContextStack<'_>,
    ) -> Result<(Query, Vec<(String, serde_json::Value)>), Error> {
        match &tag.body {
            QueryBody::Hygienic(t) => {
                let i = t.instantiate(stack, self.host.command_registry()).await?;
                Ok((i.query, i.overrides))
            }
            QueryBody::Classic(c) => Ok((classic(c, stack).await?, Vec::new())),
            QueryBody::Invalid(i) => Err(i.error.clone()),
        }
    }

    async fn query(&self, tag: &QueryTag, stack: &ContextStack<'_>) -> Result<AnyNode, Error> {
        Ok(self.evaluate(tag, stack).await?.0)
    }

    /// Evaluates a query tag: its result, and the query it asked for.
    async fn evaluate(&self, tag: &QueryTag, stack: &ContextStack<'_>) -> Result<(AnyNode, Query), Error> {
        let (query, overrides) = self.build_query(tag, stack).await?;
        let input = match &tag.apply {
            None => None,
            Some(ApplyTarget::Current(position)) => match stack.top_data() {
                Some(frame) => Some(frame.to_any()),
                None => {
                    return Err(Error::general_error(
                        "there is no current context to apply the query to".to_string(),
                    )
                    .with_position(position))
                }
            },
            Some(ApplyTarget::Name(name)) => match resolve(name, stack).await? {
                Resolved::Missing => {
                    return Err(Error::general_error(format!(
                        "'{name}', the input of this query, is missing"
                    ))
                    .with_position(&name.position))
                }
                value => Some(value.to_any()),
            },
        };
        let value = self
            .host
            .evaluate(QueryRequest {
                query: query.clone(),
                overrides,
                input,
                position: tag.position.clone(),
            })
            .await
            .map_err(|e| {
                if e.position.is_unknown() {
                    e.with_position(&tag.position)
                } else {
                    e
                }
            })?;
        Ok((value, query))
    }
}

/// Expands a classic query's text, then parses it.
async fn classic(query: &ClassicQuery, stack: &ContextStack<'_>) -> Result<Query, Error> {
    let mut text = String::new();
    for piece in &query.pieces {
        match piece {
            ClassicPiece::Text(t, _) => text.push_str(t),
            ClassicPiece::Slot(slot) => {
                let value = resolve(&slot.name, stack).await?;
                match value.kind() {
                    NodeKind::Null => {
                        return Err(Error::general_error(format!(
                            "{{{{{}}}}} is missing or null, which a query cannot take",
                            slot.name
                        ))
                        .with_position(&slot.position))
                    }
                    NodeKind::Bool | NodeKind::Integer | NodeKind::Float | NodeKind::Text => {
                        text.push_str(&text_of(&value).await?)
                    }
                    other @ (NodeKind::List | NodeKind::Map | NodeKind::Opaque) => {
                        return Err(Error::not_supported(format!(
                            "{{{{{}}}}} is a {}; a classic query inserts text only",
                            slot.name,
                            kind_name(other)
                        ))
                        .with_position(&slot.position))
                    }
                }
            }
        }
    }
    parse_query(&text).map_err(|e| {
        let offset = e.position.offset;
        Error::query_parse_error(
            &text,
            &format!("{} (offset {offset} of the expanded query)", e.message),
            &query.position,
        )
    })
}
