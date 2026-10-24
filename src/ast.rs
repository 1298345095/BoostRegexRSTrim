// Ported from the retained Boost.Regex parser/creator. Distributed under BSL-1.0.

pub(crate) type NodeId = usize;

#[derive(Clone, Debug)]
pub(crate) struct Program {
    pub nodes: Vec<Node>,
    pub root: NodeId,
    pub groups: Vec<Option<NodeId>>,
    pub names: Vec<(Vec<u8>, usize)>,
    pub mark_count: usize,
    pub pattern_len: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum Node {
    Empty,
    Literal {
        bytes: Vec<u8>,
        icase: bool,
    },
    Dot {
        newline: bool,
    },
    Combining,
    Set(ByteSet),
    Anchor(Anchor),
    Sequence(Vec<NodeId>),
    Alternation(Vec<NodeId>),
    Capture {
        index: usize,
        body: NodeId,
    },
    Group {
        body: NodeId,
    },
    Repeat {
        body: NodeId,
        min: usize,
        max: usize,
        greedy: bool,
        possessive: bool,
    },
    Assert {
        body: NodeId,
        positive: bool,
        behind: Option<usize>,
    },
    Atomic {
        body: NodeId,
    },
    Backref {
        target: Reference,
        icase: bool,
    },
    Recurse(Reference),
    ConditionTest(Condition),
    Conditional {
        condition: Condition,
        yes: NodeId,
        no: NodeId,
        explicit_else: bool,
    },
    ResetStart,
    ToggleCase(bool),
    Verb(Verb),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    LineStart,
    LineEnd,
    BufferStart,
    BufferEnd,
    SoftEnd,
    WordBoundary,
    NotWordBoundary,
    WordStart,
    WordEnd,
    Continue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    Number(usize),
    Name(Vec<u8>),
}

#[derive(Clone, Debug)]
pub(crate) enum Condition {
    Capture(Reference),
    Recursion(Option<Reference>),
    Assertion(NodeId),
    Define,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    Fail,
    Accept,
    Commit,
    Prune,
    Skip,
    Then,
}

#[derive(Clone, Debug)]
pub(crate) struct ByteSet {
    pub map: Box<[bool; 256]>,
    pub sequences: Vec<Vec<u8>>,
    pub ranges: Vec<(Vec<u8>, Vec<u8>)>,
    pub equivalents: Vec<Vec<u8>>,
    pub long: bool,
    pub negated: bool,
    pub icase: bool,
}

impl ByteSet {
    pub fn new(icase: bool) -> Self {
        Self {
            map: Box::new([false; 256]),
            sequences: Vec::new(),
            ranges: Vec::new(),
            equivalents: Vec::new(),
            long: false,
            negated: false,
            icase,
        }
    }
}

pub(crate) fn name_hash(bytes: &[u8]) -> usize {
    let mut result = 0_usize;
    for &byte in bytes {
        let value = if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            usize::from(byte)
        } else {
            isize::from(byte.cast_signed()).cast_unsigned()
        };
        result ^= value
            .wrapping_add(0x9e37_79b9)
            .wrapping_add(result << 6)
            .wrapping_add(result >> 2);
    }
    (result % 2_147_483_647) | 0x4000_0000
}
