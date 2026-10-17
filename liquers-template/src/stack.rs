//! The Mustache context stack: the frames a name is looked up in, top first.
//!
//! A frame is either an owned [`AnyNode`] (the state and `data` of a render, a query result) or a
//! node borrowed from a frame below it while a section iterates a sync list, so a row is never
//! copied to be looked at. See `specs/design/liquers-mustache/phase2-architecture.md` §Interfaces.

use std::sync::Arc;

use liquers_core::data_node::{AnyNode, AsyncDataNode, DataNode, NodeRef};

/// What a frame is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameKind {
    /// A context: names are looked up in it, and it is `{{.}}` and the input of an apply tag while
    /// it is the top `Data` frame.
    Data,
    /// Named values only (`render`'s `data`): looked up, never `{{.}}`.
    Names,
}

pub(crate) enum FrameNode<'a> {
    Borrowed(&'a dyn DataNode),
    Ref(NodeRef<'a>),
    Any(AnyNode),
}

/// A frame's node, as a consumer reads it.
pub(crate) enum FrameView<'b> {
    Sync(&'b dyn DataNode),
    Async(&'b Arc<dyn AsyncDataNode>),
}

pub(crate) struct Frame<'a> {
    node: FrameNode<'a>,
    kind: FrameKind,
}

impl<'a> Frame<'a> {
    pub(crate) fn view(&self) -> FrameView<'_> {
        match &self.node {
            FrameNode::Borrowed(n) => FrameView::Sync(*n),
            FrameNode::Ref(r) => FrameView::Sync(r.node()),
            FrameNode::Any(AnyNode::Sync(n)) => FrameView::Sync(n.as_ref()),
            FrameNode::Any(AnyNode::Async(n)) => FrameView::Async(n),
        }
    }

    pub(crate) fn kind(&self) -> FrameKind {
        self.kind
    }

    /// The frame's node as an owned node: shared nodes stay shared, a borrowed one is snapshotted.
    pub(crate) fn to_any(&self) -> AnyNode {
        match &self.node {
            FrameNode::Any(a) => a.clone(),
            FrameNode::Ref(r) => crate::name::own(r),
            FrameNode::Borrowed(n) => crate::name::own(&NodeRef::Borrowed(*n)),
        }
    }

    fn reborrow(&self) -> Frame<'_> {
        let node = match &self.node {
            FrameNode::Any(a) => FrameNode::Any(a.clone()),
            FrameNode::Ref(NodeRef::Shared(a)) => FrameNode::Any(AnyNode::Sync(a.clone())),
            FrameNode::Ref(r) => FrameNode::Borrowed(r.node()),
            FrameNode::Borrowed(n) => FrameNode::Borrowed(*n),
        };
        Frame {
            node,
            kind: self.kind,
        }
    }
}

/// The frames of a render, bottom first.
#[derive(Default)]
pub struct ContextStack<'a> {
    frames: Vec<Frame<'a>>,
}

impl<'a> ContextStack<'a> {
    pub fn new() -> Self {
        ContextStack { frames: Vec::new() }
    }

    /// Pushes `node` on top.
    pub fn push(&mut self, node: AnyNode, kind: FrameKind) {
        self.frames.push(Frame {
            node: FrameNode::Any(node),
            kind,
        });
    }

    /// The number of frames.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub(crate) fn push_ref(&mut self, node: NodeRef<'a>, kind: FrameKind) {
        self.frames.push(Frame {
            node: FrameNode::Ref(node),
            kind,
        });
    }

    /// A stack with the same frames, borrowed, to push section frames onto.
    pub(crate) fn reborrow(&self) -> ContextStack<'_> {
        ContextStack {
            frames: self.frames.iter().map(Frame::reborrow).collect(),
        }
    }

    /// The frames, top first.
    pub(crate) fn top_down(&self) -> impl Iterator<Item = &Frame<'a>> {
        self.frames.iter().rev()
    }

    /// The top `Data` frame: `{{.}}` and the input of an apply tag.
    pub(crate) fn top_data(&self) -> Option<&Frame<'a>> {
        self.top_down().find(|f| match f.kind() {
            FrameKind::Data => true,
            FrameKind::Names => false,
        })
    }
}
