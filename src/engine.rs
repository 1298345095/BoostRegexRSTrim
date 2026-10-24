// Ported from the retained perl_matcher*.hpp state machine. BSL-1.0.

use crate::ast::{Anchor, ByteSet, Condition, Node, NodeId, Program, Reference, Verb, name_hash};
use crate::character::CharacterRules;
use crate::{Error, Match};

type Pc = usize;

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Forward,
    Match,
    Literal(Vec<u8>, bool),
    Dot(bool),
    Combining,
    Set(ByteSet),
    Anchor(Anchor),
    Start(usize),
    End(usize),
    Alt(Pc),
    Jump,
    Repeat {
        body: Pc,
        min: usize,
        max: usize,
        greedy: bool,
        id: usize,
    },
    SingleRepeat {
        atom: Atom,
        min: usize,
        max: usize,
        greedy: bool,
    },
    Assert {
        body: Pc,
        positive: bool,
    },
    Atomic(Pc),
    ScopeEnd,
    Backstep(usize),
    Backref(Reference, bool),
    Recurse(Reference),
    Conditional {
        condition: Condition,
        yes: Pc,
        no: Pc,
    },
    ConditionTest(Condition),
    ResetStart,
    ToggleCase(bool),
    Verb(Verb),
}

#[derive(Clone, Debug)]
pub(crate) enum Atom {
    Byte(u8, bool),
    Dot(bool),
    Set(ByteSet),
}

#[derive(Clone, Debug)]
pub(crate) struct Instruction {
    pub kind: Kind,
    pub next: Pc,
    pub linear_next: Pc,
}

#[derive(Clone, Debug)]
pub(crate) struct Engine {
    code: Vec<Instruction>,
    entry: Pc,
    group_entries: Vec<Pc>,
    assertion_entries: Vec<Option<Pc>>,
    assertion_positive: Vec<bool>,
    first: Vec<First>,
    restart_type: Restart,
    leading_repeat: Option<Pc>,
    names: Vec<(Vec<u8>, usize)>,
    mark_count: usize,
    pattern_len: usize,
}

#[derive(Clone, Copy, Debug)]
enum Restart {
    Any,
    Line,
    Word,
    Buffer,
    Continue,
}

pub(crate) struct Execution {
    pub result: Result<Option<Match>, Error>,
    #[cfg(feature = "diagnostics")]
    pub state_count: usize,
    #[cfg(feature = "diagnostics")]
    pub max_state_count: usize,
    #[cfg(feature = "diagnostics")]
    pub blocks_remaining_after_cleanup: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct First {
    pub bytes: [bool; 256],
    pub nullable: bool,
}

impl First {
    pub fn empty() -> Self {
        Self {
            bytes: [false; 256],
            nullable: false,
        }
    }
    pub fn any() -> Self {
        Self {
            bytes: [true; 256],
            nullable: true,
        }
    }
    pub fn union(&mut self, other: &Self) {
        for (left, right) in self.bytes.iter_mut().zip(&other.bytes) {
            *left |= right;
        }
        self.nullable |= other.nullable;
    }
    fn allows(&self, input: &[u8], position: usize) -> bool {
        input
            .get(position)
            .map_or(self.nullable, |&byte| self.bytes[usize::from(byte)])
    }
}

struct Compiler<'a> {
    program: &'a Program,
    code: Vec<Instruction>,
    entries: Vec<Option<Pc>>,
    assertion_entries: Vec<Option<Pc>>,
    pending: Vec<(NodeId, Pc, Pc)>,
}

impl Compiler<'_> {
    fn emit(&mut self, kind: Kind, next: Pc) -> Pc {
        let pc = self.code.len();
        self.code.push(Instruction {
            kind,
            next,
            linear_next: next,
        });
        pc
    }

    fn reserve(&mut self, id: NodeId, next: Pc) -> Pc {
        let entry = self.emit(Kind::Forward, next);
        self.entries[id] = Some(entry);
        self.pending.push((id, entry, next));
        entry
    }

    // Lower on a heap worklist: accepted nested groups must not consume the
    // caller thread's native stack, including unoptimized consumer builds.
    #[allow(clippy::too_many_lines)]
    fn compile(&mut self, id: NodeId, next: Pc) -> Pc {
        let root = self.reserve(id, next);
        while let Some((id, entry, next)) = self.pending.pop() {
            let mut successor = next;
            let kind = match self.program.nodes[id].clone() {
                Node::Empty => Kind::Forward,
                Node::Literal { bytes, icase } => Kind::Literal(bytes, icase),
                Node::Dot { newline } => Kind::Dot(newline),
                Node::Combining => Kind::Combining,
                Node::Set(set) => Kind::Set(set),
                Node::Anchor(anchor) => Kind::Anchor(anchor),
                Node::Sequence(nodes) => {
                    for node in nodes.into_iter().rev() {
                        successor = self.reserve(node, successor);
                    }
                    Kind::Forward
                }
                Node::Alternation(nodes) => {
                    let mut branches = nodes.into_iter().rev();
                    let mut tail = branches
                        .next()
                        .map_or(next, |node| self.reserve(node, next));
                    for node in branches {
                        let jump = self.emit(Kind::Jump, next);
                        let branch = self.reserve(node, jump);
                        tail = self.emit(Kind::Alt(tail), branch);
                    }
                    successor = tail;
                    Kind::Forward
                }
                Node::Capture { index, body } => {
                    let end = self.emit(Kind::End(index), next);
                    successor = self.reserve(body, end);
                    Kind::Start(index)
                }
                Node::Group { body } => {
                    let end = self.emit(Kind::End(0), next);
                    successor = self.reserve(body, end);
                    Kind::Start(0)
                }
                Node::Repeat {
                    body,
                    min,
                    max,
                    greedy,
                    possessive,
                } => {
                    let exit = if possessive {
                        self.emit(Kind::ScopeEnd, next)
                    } else {
                        next
                    };
                    let atom = match &self.program.nodes[body] {
                        Node::Literal { bytes, icase } if bytes.len() == 1 => {
                            Some(Atom::Byte(bytes[0], *icase))
                        }
                        Node::Dot { newline } => Some(Atom::Dot(*newline)),
                        Node::Set(set) if !set.long => Some(Atom::Set(set.clone())),
                        _ => None,
                    };
                    let repeat = if possessive {
                        self.emit(Kind::Forward, exit)
                    } else {
                        entry
                    };
                    let repeat_kind = if let Some(atom) = atom {
                        Kind::SingleRepeat {
                            atom,
                            min,
                            max,
                            greedy,
                        }
                    } else {
                        let jump = self.emit(Kind::Jump, repeat);
                        let body = self.reserve(body, jump);
                        Kind::Repeat {
                            body,
                            min,
                            max,
                            greedy,
                            id: 0,
                        }
                    };
                    if possessive {
                        self.code[repeat].kind = repeat_kind;
                        Kind::Atomic(repeat)
                    } else {
                        successor = exit;
                        repeat_kind
                    }
                }
                Node::Assert {
                    body,
                    positive,
                    behind,
                } => {
                    let end = self.emit(Kind::ScopeEnd, next);
                    let mut body = self.reserve(body, end);
                    if let Some(width) = behind {
                        body = self.emit(Kind::Backstep(width), body);
                    }
                    Kind::Assert { body, positive }
                }
                Node::Atomic { body } => {
                    let end = self.emit(Kind::ScopeEnd, next);
                    let body = self.reserve(body, end);
                    Kind::Atomic(body)
                }
                Node::Backref { target, icase } => Kind::Backref(target, icase),
                Node::Recurse(target) => Kind::Recurse(target),
                Node::Conditional {
                    condition,
                    yes,
                    no,
                    explicit_else,
                } => {
                    if let Condition::Assertion(assertion) = &condition {
                        let end = self.emit(Kind::ScopeEnd, next);
                        let (body, behind) = match self.program.nodes[*assertion] {
                            Node::Assert { body, behind, .. } => (body, behind),
                            Node::Atomic { body } => (body, None),
                            _ => unreachable!(
                                "condition parser emits an assertion or atomic condition"
                            ),
                        };
                        let mut body = self.reserve(body, end);
                        if let Some(width) = behind {
                            body = self.emit(Kind::Backstep(width), body);
                        }
                        self.assertion_entries[*assertion] = Some(body);
                    }
                    let end = self.emit(Kind::End(0), next);
                    let yes_end = if explicit_else {
                        self.emit(Kind::Jump, end)
                    } else {
                        end
                    };
                    let yes = self.reserve(yes, yes_end);
                    let no = self.reserve(no, end);
                    Kind::Conditional { condition, yes, no }
                }
                Node::ConditionTest(condition) => Kind::ConditionTest(condition),
                Node::ResetStart => Kind::ResetStart,
                Node::ToggleCase(value) => Kind::ToggleCase(value),
                Node::Verb(verb) => Kind::Verb(verb),
            };
            self.code[entry] = Instruction {
                kind,
                next: successor,
                linear_next: successor,
            };
        }
        root
    }

    fn compact(&mut self, root: Pc) -> Pc {
        let mut remap = vec![0; self.code.len()];
        let mut length = 0;
        for (pc, instruction) in self.code.iter().enumerate() {
            if !matches!(instruction.kind, Kind::Forward) {
                remap[pc] = length;
                length += 1;
            }
        }
        for pc in 0..self.code.len() {
            if matches!(self.code[pc].kind, Kind::Forward) {
                let mut target = self.code[pc].next;
                while matches!(self.code[target].kind, Kind::Forward) {
                    target = self.code[target].next;
                }
                remap[pc] = remap[target];
            }
        }
        let mut compact = Vec::with_capacity(length);
        for mut instruction in self.code.drain(..) {
            if matches!(instruction.kind, Kind::Forward) {
                continue;
            }
            instruction.next = remap[instruction.next];
            instruction.linear_next = instruction.next;
            match &mut instruction.kind {
                Kind::Alt(other) | Kind::Atomic(other) => *other = remap[*other],
                Kind::Repeat { body, .. } | Kind::Assert { body, .. } => *body = remap[*body],
                Kind::Conditional { yes, no, .. } => {
                    *yes = remap[*yes];
                    *no = remap[*no];
                }
                _ => {}
            }
            compact.push(instruction);
        }
        self.code = compact;
        for pc in self
            .entries
            .iter_mut()
            .chain(&mut self.assertion_entries)
            .flatten()
        {
            *pc = remap[*pc];
        }
        remap[root]
    }
}

impl Engine {
    pub(crate) fn new(program: &Program, rules: &CharacterRules) -> Result<Self, Error> {
        let mut compiler = Compiler {
            program,
            code: Vec::new(),
            entries: vec![None; program.nodes.len()],
            assertion_entries: vec![None; program.nodes.len()],
            pending: Vec::new(),
        };
        let done = compiler.emit(Kind::Match, 0);
        let body = compiler.compile(program.root, done);
        let mut first = body;
        while matches!(compiler.code[first].kind, Kind::Forward) {
            first = compiler.code[first].next;
        }
        // The source inserts a top-level alternative before its original
        // Start(0), leaving that start marker in the first branch only.
        let root = if matches!(compiler.code[first].kind, Kind::Alt(_)) {
            let start = compiler.emit(Kind::Start(0), compiler.code[first].next);
            compiler.code[first].next = start;
            body
        } else {
            compiler.emit(Kind::Start(0), body)
        };
        let entry = compiler.compact(root);
        let mut group_entries: Vec<Pc> = program
            .groups
            .iter()
            .map(|id| id.and_then(|id| compiler.entries[id]).unwrap_or(entry))
            .collect();
        group_entries[0] = entry;
        let mut result = Self {
            first: vec![First::empty(); compiler.code.len()],
            code: compiler.code,
            entry,
            group_entries,
            assertion_entries: compiler.assertion_entries,
            assertion_positive: program
                .nodes
                .iter()
                .map(|node| {
                    !matches!(
                        node,
                        Node::Assert {
                            positive: false,
                            ..
                        }
                    )
                })
                .collect(),
            restart_type: Restart::Any,
            leading_repeat: None,
            names: program.names.clone(),
            mark_count: program.mark_count,
            pattern_len: program.pattern_len,
        };
        let mut repeat_id = 0;
        let order = crate::startmap::lexical_order(&result.code, result.entry);
        for pair in order.windows(2) {
            result.code[pair[0]].linear_next = pair[1];
        }
        for pc in order {
            match &mut result.code[pc].kind {
                Kind::Repeat { id, .. } => {
                    *id = repeat_id;
                    repeat_id += 1;
                }
                Kind::SingleRepeat { .. } => repeat_id += 1,
                _ => {}
            }
        }
        result.first =
            crate::startmap::build(&result.code, &result.group_entries, &result.names, rules)?;
        result.restart_type = result.restart_type();
        result.leading_repeat = result.leading_repeat();
        Ok(result)
    }

    fn restart_type(&self) -> Restart {
        let mut pc = self.entry;
        loop {
            match self.code[pc].kind {
                Kind::Start(_) | Kind::End(_) | Kind::ResetStart => pc = self.code[pc].next,
                Kind::Anchor(Anchor::LineStart) => return Restart::Line,
                Kind::Anchor(Anchor::WordStart) => return Restart::Word,
                Kind::Anchor(Anchor::BufferStart) => return Restart::Buffer,
                Kind::Anchor(Anchor::Continue) => return Restart::Continue,
                _ => return Restart::Any,
            }
        }
    }

    fn leading_repeat(&self) -> Option<Pc> {
        if self
            .code
            .iter()
            .any(|instruction| matches!(instruction.kind, Kind::Backref(..)))
        {
            return None;
        }
        let mut pc = self.entry;
        loop {
            match self.code[pc].kind {
                Kind::Start(_) | Kind::End(_) | Kind::Assert { .. } => pc = self.code[pc].next,
                Kind::Anchor(anchor) if !matches!(anchor, Anchor::SoftEnd) => {
                    pc = self.code[pc].next;
                }
                Kind::Atomic(body) => pc = body,
                Kind::SingleRepeat { .. } => return Some(pc),
                _ => return None,
            }
        }
    }

    fn resolve(&self, target: &Reference, captures: Option<&[Capture]>) -> Option<usize> {
        match target {
            Reference::Number(index) => (*index < self.group_entries.len()).then_some(*index),
            Reference::Name(name) => {
                let hash = name_hash(name);
                let mut fallback = None;
                for (candidate, index) in &self.names {
                    if name_hash(candidate) == hash {
                        fallback = Some(*index);
                        if captures.is_none_or(|captures| captures[*index].matched) {
                            return Some(*index);
                        }
                    }
                }
                fallback
            }
        }
    }

    pub(crate) fn search(
        &self,
        input: &[u8],
        rules: &CharacterRules,
        start: usize,
        full: bool,
        not_initial_null: bool,
    ) -> Result<Option<Match>, Error> {
        self.run(input, rules, start, full, not_initial_null).result
    }

    pub(crate) fn run(
        &self,
        input: &[u8],
        rules: &CharacterRules,
        start: usize,
        full: bool,
        not_initial_null: bool,
    ) -> Execution {
        let mut vm = Machine::new(self, input, rules, start, full, not_initial_null);
        let result = vm.search();
        vm.stack.clear();
        vm.calls.clear();
        vm.storage = Storage::new();
        Execution {
            result,
            #[cfg(feature = "diagnostics")]
            state_count: vm.state_count,
            #[cfg(feature = "diagnostics")]
            max_state_count: vm.max_state_count,
            #[cfg(feature = "diagnostics")]
            blocks_remaining_after_cleanup: 1024 - vm.storage.blocks,
        }
    }
}

#[derive(Clone, Debug)]
struct Capture {
    start: usize,
    end: usize,
    matched: bool,
}

#[derive(Clone, Debug)]
struct Results {
    captures: Vec<Capture>,
    last_closed: Option<usize>,
}

#[derive(Clone, Debug)]
struct Call {
    index: usize,
    position: usize,
    return_pc: Pc,
    prior: Results,
}

#[derive(Clone, Copy, Debug)]
struct Storage {
    blocks: usize,
    free: usize,
}

impl Storage {
    fn new() -> Self {
        Self {
            blocks: 0,
            free: 4096 - 16,
        }
    }
    fn push(&mut self, bytes: usize) -> Result<Self, Error> {
        let previous = *self;
        if bytes > self.free {
            if self.blocks == 1024 {
                return Err(Error::execution(19));
            }
            self.blocks += 1;
            self.free = 4096 - 24;
        }
        self.free -= bytes;
        Ok(previous)
    }
}

#[derive(Debug)]
enum Frame {
    Capture(usize, Capture),
    Alt {
        pc: Pc,
        position: usize,
        lazy: bool,
    },
    Counter {
        id: isize,
        count: usize,
        position: usize,
    },
    Single {
        pc: Pc,
        count: usize,
        position: usize,
        minimum: usize,
        maximum: usize,
        greedy: bool,
    },
    Scope {
        next: Pc,
        position: usize,
        positive: bool,
        atomic: bool,
        prior_independent: bool,
    },
    Condition {
        yes: Pc,
        no: Pc,
        position: usize,
        positive: bool,
    },
    EnterCall,
    ReturnCall {
        call: Call,
        internal: Results,
    },
    Commit,
    Then,
    Case(bool),
}

// The source's COMMIT and THEN unwind handlers call unwind again. Keep those
// calls on the heap so deeply nested verbs do not consume the native stack.
enum ControlUnwind {
    Commit(bool),
    Then(bool),
    FinishThen,
}

#[derive(Debug)]
struct Saved {
    frame: Frame,
    before: Storage,
}

// Independent options and matcher flags follow the retained source's state.
#[allow(clippy::struct_excessive_bools)]
struct Machine<'a> {
    engine: &'a Engine,
    input: &'a [u8],
    rules: &'a CharacterRules,
    search_base: usize,
    full: bool,
    not_initial_null: bool,
    state_count: usize,
    max_state_count: usize,
    pc: Pc,
    position: usize,
    restart: usize,
    results: Results,
    stack: Vec<Saved>,
    storage: Storage,
    calls: Vec<Call>,
    native_depth: usize,
    icase: bool,
    independent: bool,
}

impl<'a> Machine<'a> {
    fn new(
        engine: &'a Engine,
        input: &'a [u8],
        rules: &'a CharacterRules,
        start: usize,
        full: bool,
        not_initial_null: bool,
    ) -> Self {
        Self {
            engine,
            input,
            rules,
            search_base: start,
            full,
            not_initial_null,
            state_count: 0,
            max_state_count: maximum_states(input.len() - start, engine.pattern_len),
            pc: engine.entry,
            position: start,
            restart: start,
            results: Results {
                captures: Vec::new(),
                last_closed: None,
            },
            stack: Vec::new(),
            storage: Storage::new(),
            calls: Vec::new(),
            native_depth: 1,
            icase: false,
            independent: false,
        }
    }

    fn push(&mut self, frame: Frame, bytes: usize) -> Result<(), Error> {
        let before = self.storage.push(bytes)?;
        self.stack.push(Saved { frame, before });
        Ok(())
    }

    fn pop(&mut self) -> Option<Frame> {
        self.stack.pop().map(|saved| {
            self.storage = saved.before;
            saved.frame
        })
    }

    // Restart selection is observable through the source's dispatch budget:
    // anchored searches try their initial position even when its start map
    // would reject the byte, whereas ordinary searches consult the map first.
    fn search(&mut self) -> Result<Option<Match>, Error> {
        if self.full {
            return Ok(self
                .attempt(self.search_base)?
                .filter(|found| found.span().start == 0));
        }
        match self.engine.restart_type {
            Restart::Any => self.search_any(),
            Restart::Line => self.search_line(),
            Restart::Word => self.search_word(),
            Restart::Buffer if self.search_base != 0 => Ok(None),
            Restart::Buffer | Restart::Continue => self.attempt(self.search_base),
        }
    }

    fn search_any(&mut self) -> Result<Option<Match>, Error> {
        let mut position = self.search_base;
        while position <= self.input.len() {
            if self.engine.first[self.engine.entry].allows(self.input, position) {
                if let Some(found) = self.attempt(position)? {
                    return Ok(Some(found));
                }
                position = self.restart;
            }
            if position == self.input.len() {
                break;
            }
            position += 1;
        }
        Ok(None)
    }

    fn search_line(&mut self) -> Result<Option<Match>, Error> {
        if let Some(found) = self.attempt(self.search_base)? {
            return Ok(Some(found));
        }
        let mut position = self.restart;
        while position < self.input.len() {
            while self
                .input
                .get(position)
                .is_some_and(|&byte| !separator(byte))
            {
                position += 1;
            }
            if position == self.input.len() {
                break;
            }
            position += 1;
            if self.engine.first[self.engine.entry].allows(self.input, position) {
                if let Some(found) = self.attempt(position)? {
                    return Ok(Some(found));
                }
                position = self.restart;
            }
        }
        Ok(None)
    }

    fn search_word(&mut self) -> Result<Option<Match>, Error> {
        let mut position = if self.search_base == 0 {
            if let Some(found) = self.attempt(0)? {
                return Ok(Some(found));
            }
            self.restart
        } else {
            self.search_base - 1
        };
        while position < self.input.len() {
            while self
                .input
                .get(position)
                .is_some_and(|&byte| self.rules.is_class(byte, b"word").unwrap_or(false))
            {
                position += 1;
            }
            while self
                .input
                .get(position)
                .is_some_and(|&byte| !self.rules.is_class(byte, b"word").unwrap_or(false))
            {
                position += 1;
            }
            if position == self.input.len() {
                break;
            }
            if self.engine.first[self.engine.entry].allows(self.input, position) {
                if let Some(found) = self.attempt(position)? {
                    return Ok(Some(found));
                }
                position = self.restart;
            }
        }
        Ok(None)
    }

    // Keep dispatch cases together so they can be audited against the 34 source states.
    #[allow(clippy::too_many_lines)]
    fn attempt(&mut self, start: usize) -> Result<Option<Match>, Error> {
        // set_first resets capture bounds but retains the source's last-closed
        // index until set_size starts the next search operation.
        let last_closed = self.results.last_closed;
        self.results = Results {
            captures: vec![
                Capture {
                    start: self.input.len(),
                    end: self.input.len(),
                    matched: false
                };
                self.engine.mark_count + 1
            ],
            last_closed,
        };
        self.results.captures[0].start = start;
        self.stack.clear();
        self.calls.clear();
        self.storage = Storage::new();
        self.native_depth = 1;
        self.independent = false;
        self.pc = self.engine.entry;
        self.position = start;
        self.restart = start;
        loop {
            self.state_count = self.state_count.saturating_add(1);
            let instruction = &self.engine.code[self.pc];
            let mut success = true;
            match &instruction.kind {
                Kind::Forward => unreachable!("compiler aliases are eliminated"),
                Kind::Match => {
                    if self.calls.last().is_some_and(|call| call.index == 0) {
                        self.return_call()?;
                        continue;
                    }
                    success = (!self.full || self.position == self.input.len())
                        && (!self.not_initial_null || self.position != self.search_base);
                    if success {
                        return Ok(Some(self.finish()));
                    }
                }
                Kind::Literal(bytes, icase) => {
                    for &byte in bytes {
                        if self
                            .input
                            .get(self.position)
                            .is_some_and(|&actual| equal_byte(actual, byte, *icase, self.rules))
                        {
                            self.position += 1;
                        } else {
                            success = false;
                            break;
                        }
                    }
                    self.pc = instruction.next;
                }
                Kind::Dot(newline) => {
                    success = self
                        .input
                        .get(self.position)
                        .is_some_and(|&byte| *newline || !separator(byte));
                    if success {
                        self.position += 1;
                    }
                    self.pc = instruction.next;
                }
                Kind::Combining => {
                    success = self.position < self.input.len();
                    if success {
                        self.position += 1;
                    }
                    self.pc = instruction.next;
                }
                Kind::Set(set) => {
                    if let Some(end) = set_match(set, self.input, self.position, self.rules) {
                        self.position = end;
                    } else {
                        success = false;
                    }
                    self.pc = instruction.next;
                }
                Kind::Anchor(anchor) => {
                    success = self.anchor(*anchor);
                    self.pc = instruction.next;
                }
                Kind::Start(index) => {
                    if *index != 0 {
                        self.push(
                            Frame::Capture(*index, self.results.captures[*index].clone()),
                            40,
                        )?;
                        self.results.captures[*index].start = self.position;
                    }
                    self.pc = instruction.next;
                }
                Kind::End(index) => {
                    if *index != 0 {
                        self.close_capture(*index);
                        if self.calls.last().is_some_and(|call| call.index == *index) {
                            self.return_call()?;
                            continue;
                        }
                    }
                    self.pc = instruction.next;
                }
                Kind::Alt(other) => {
                    let first =
                        self.engine.first[instruction.next].allows(self.input, self.position);
                    let second = self.engine.first[*other].allows(self.input, self.position);
                    if first {
                        if second {
                            self.push(
                                Frame::Alt {
                                    pc: *other,
                                    position: self.position,
                                    lazy: false,
                                },
                                24,
                            )?;
                        }
                        self.pc = instruction.next;
                    } else if second {
                        self.pc = *other;
                    } else {
                        success = false;
                    }
                }
                Kind::Jump => self.pc = instruction.next,
                Kind::Repeat {
                    body,
                    min,
                    max,
                    greedy,
                    id,
                } => success = self.repeat(*body, instruction.next, *min, *max, *greedy, *id)?,
                Kind::SingleRepeat {
                    atom,
                    min,
                    max,
                    greedy,
                } => {
                    let desired = if *greedy { *max } else { *min };
                    let mut count = 0;
                    while count < desired {
                        if let Some(end) = atom_match(atom, self.input, self.position, self.rules) {
                            self.position = end;
                            count += 1;
                        } else {
                            break;
                        }
                    }
                    success = count >= *min;
                    if success {
                        if *greedy && count < *max && self.engine.leading_repeat == Some(self.pc) {
                            self.restart = self.position;
                        }
                        if (*greedy && count > *min) || (!*greedy && count < *max) {
                            self.push(
                                Frame::Single {
                                    pc: self.pc,
                                    count,
                                    position: self.position,
                                    minimum: *min,
                                    maximum: *max,
                                    greedy: *greedy,
                                },
                                32,
                            )?;
                        }
                        self.pc = instruction.next;
                        if !*greedy {
                            success = self.engine.first[self.pc].allows(self.input, self.position);
                        }
                    }
                }
                Kind::Assert { body, positive } => {
                    self.push(
                        Frame::Scope {
                            next: instruction.next,
                            position: self.position,
                            positive: *positive,
                            atomic: false,
                            prior_independent: self.independent,
                        },
                        32,
                    )?;
                    self.pc = *body;
                }
                Kind::Atomic(body) => {
                    let prior_independent = self.independent;
                    self.independent = true;
                    self.native_depth += 1;
                    if self.native_depth > 80 {
                        return Err(Error::execution(18));
                    }
                    self.push(
                        Frame::Scope {
                            next: instruction.next,
                            position: self.position,
                            positive: true,
                            atomic: true,
                            prior_independent,
                        },
                        8,
                    )?;
                    self.pc = *body;
                }
                Kind::ScopeEnd => {
                    success = self.unwind(true)?;
                    if !success {
                        return Ok(None);
                    }
                    continue;
                }
                Kind::Backstep(width) => {
                    if let Some(position) = self.position.checked_sub(*width) {
                        self.position = position;
                    } else {
                        success = false;
                    }
                    self.pc = instruction.next;
                }
                Kind::Backref(reference, icase) => {
                    if let Some(index) =
                        self.engine.resolve(reference, Some(&self.results.captures))
                    {
                        let capture = &self.results.captures[index];
                        success = capture.matched;
                        if success {
                            for offset in capture.start..capture.end {
                                if self.input.get(self.position).is_some_and(|&actual| {
                                    equal_byte(actual, self.input[offset], *icase, self.rules)
                                }) {
                                    self.position += 1;
                                } else {
                                    success = false;
                                    break;
                                }
                            }
                        }
                    } else {
                        success = false;
                    }
                    self.pc = instruction.next;
                }
                Kind::Recurse(reference) => {
                    let Some(index) = self.engine.resolve(reference, None) else {
                        return Err(Error::execution(6));
                    };
                    success = self
                        .calls
                        .iter()
                        .rev()
                        .find(|call| call.index == index)
                        .is_none_or(|call| call.position != self.position);
                    if success {
                        self.push(Frame::EnterCall, 8)?;
                        self.calls.push(Call {
                            index,
                            position: self.position,
                            return_pc: instruction.next,
                            prior: self.results.clone(),
                        });
                        self.push(
                            Frame::Counter {
                                id: -2 - index.cast_signed(),
                                count: 0,
                                position: self.position,
                            },
                            48,
                        )?;
                        self.pc = self.engine.group_entries[index];
                    }
                }
                Kind::Conditional { condition, yes, no } => {
                    if let Condition::Assertion(node) = condition {
                        self.native_depth += 1;
                        if self.native_depth > 80 {
                            return Err(Error::execution(18));
                        }
                        self.push(
                            Frame::Condition {
                                yes: *yes,
                                no: *no,
                                position: self.position,
                                positive: self.engine.assertion_positive[*node],
                            },
                            8,
                        )?;
                        self.pc = self.engine.assertion_entries[*node]
                            .expect("compiled assertion condition");
                    } else {
                        self.pc = if self.condition(condition) { *yes } else { *no };
                    }
                }
                Kind::ConditionTest(condition) => {
                    success = self.condition(condition);
                    self.pc = instruction.next;
                }
                Kind::ResetStart => {
                    self.push(Frame::Capture(0, self.results.captures[0].clone()), 40)?;
                    self.results.captures[0].start = self.position;
                    self.pc = instruction.next;
                }
                Kind::ToggleCase(value) => {
                    self.push(Frame::Case(self.icase), 16)?;
                    self.icase = *value;
                    self.pc = instruction.next;
                }
                Kind::Verb(verb) => {
                    match verb {
                        Verb::Fail => success = false,
                        Verb::Accept => {
                            let target = self.calls.last().map_or(usize::MAX, |call| call.index);
                            self.skip_until_group(target, true)?;
                            continue;
                        }
                        Verb::Commit | Verb::Prune | Verb::Skip => {
                            if *verb == Verb::Commit {
                                self.restart = self.input.len();
                            }
                            if *verb == Verb::Skip && self.position != self.search_base {
                                self.restart = self.position.saturating_sub(1);
                            }
                            self.push(Frame::Commit, 8)?;
                        }
                        Verb::Then => self.push(Frame::Then, 8)?,
                    }
                    self.pc = instruction.next;
                }
            }
            if !success {
                if self.state_count > self.max_state_count {
                    return Err(Error::execution(18));
                }
                if !self.unwind(false)? {
                    return Ok(None);
                }
            }
        }
    }

    fn counter(&self) -> Option<usize> {
        self.stack
            .iter()
            .rposition(|saved| matches!(saved.frame, Frame::Counter { .. }))
    }

    fn repeat(
        &mut self,
        body: Pc,
        exit: Pc,
        min: usize,
        max: usize,
        greedy: bool,
        id: usize,
    ) -> Result<bool, Error> {
        let id = id.cast_signed();
        if !self.stack.last().is_some_and(
            |saved| matches!(saved.frame, Frame::Counter { id: value, .. } if value == id),
        ) {
            let current_recursion = self
                .calls
                .last()
                .map_or(-5, |call| -2 - call.index.cast_signed());
            let previous = self.counter().and_then(|index| {
                let Frame::Counter { id: recent, .. } = self.stack[index].frame else {
                    unreachable!()
                };
                if recent >= 0 && id > recent {
                    return None;
                }
                for saved in self.stack[..=index].iter().rev() {
                    if let Frame::Counter {
                        id: found,
                        count,
                        position,
                    } = saved.frame
                    {
                        if found == current_recursion {
                            break;
                        }
                        if found == id {
                            return Some((count, position));
                        }
                    }
                }
                None
            });
            let (count, position) = previous.unwrap_or((0, self.position));
            self.push(
                Frame::Counter {
                    id,
                    count,
                    position,
                },
                48,
            )?;
        }
        let counter = self.counter().expect("repeat has a counter");
        let Frame::Counter {
            count, position, ..
        } = &mut self.stack[counter].frame
        else {
            unreachable!()
        };
        if *count != 0 && *position == self.position {
            *count = max;
        } else {
            *position = self.position;
        }
        let count = *count;
        let take = self.engine.first[body].allows(self.input, self.position);
        let skip = self.engine.first[exit].allows(self.input, self.position);
        if count < min || (greedy && count < max && take) || (!skip && count < max) {
            if !take {
                return Ok(false);
            }
            if count >= min && greedy && skip {
                self.push(
                    Frame::Alt {
                        pc: exit,
                        position: self.position,
                        lazy: false,
                    },
                    24,
                )?;
            }
            if let Frame::Counter { count, .. } = &mut self.stack[counter].frame {
                *count += 1;
            }
            self.pc = body;
            Ok(true)
        } else if skip {
            if !greedy && count < max && take {
                self.push(
                    Frame::Alt {
                        pc: body,
                        position: self.position,
                        lazy: true,
                    },
                    24,
                )?;
            }
            self.pc = exit;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // Keep the source's rollback cases together for state-by-state auditing.
    #[allow(clippy::too_many_lines)]
    fn unwind(&mut self, mut matched: bool) -> Result<bool, Error> {
        let mut controls = Vec::new();
        let mut unwound_lookahead = false;
        let mut unwound_alt = false;
        let mut native_stop = None;
        let mut draining = false;
        'invoke: loop {
            let resumed = loop {
                let Some(frame) = self.pop() else {
                    break false;
                };
                match frame {
                    Frame::Capture(index, previous) => {
                        if !matched {
                            self.results.captures[index] = previous;
                            if index != 0 {
                                self.results.last_closed = Some(index);
                            }
                        }
                    }
                    Frame::Alt { pc, position, lazy } => {
                        // A lazy repeater resumes like an alternative but uses a
                        // different source unwinder and is not a THEN boundary.
                        unwound_alt = !matched && !lazy;
                        if !matched {
                            self.pc = pc;
                            self.position = position;
                            if lazy
                                && let Some(index) = self.counter()
                                && let Frame::Counter { count, .. } = &mut self.stack[index].frame
                            {
                                *count += 1;
                            }
                            break true;
                        }
                    }
                    Frame::Counter { .. } => {}
                    Frame::Single {
                        pc,
                        mut count,
                        mut position,
                        minimum,
                        maximum,
                        greedy,
                    } => {
                        if matched {
                            continue;
                        }
                        let Kind::SingleRepeat { atom, .. } = &self.engine.code[pc].kind else {
                            unreachable!()
                        };
                        let exit = self.engine.code[pc].next;
                        let mut available = false;
                        if greedy {
                            while count > minimum {
                                position = position.saturating_sub(1);
                                count -= 1;
                                self.state_count += 1;
                                if self.engine.first[exit].allows(self.input, position) {
                                    available = true;
                                    break;
                                }
                            }
                        } else {
                            while count < maximum {
                                let Some(end) = atom_match(atom, self.input, position, self.rules)
                                else {
                                    break;
                                };
                                position = end;
                                count += 1;
                                self.state_count += 1;
                                if self.engine.first[exit].allows(self.input, position) {
                                    available = true;
                                    break;
                                }
                            }
                            if count < maximum && self.engine.leading_repeat == Some(pc) {
                                self.restart = position;
                            }
                        }
                        if available {
                            if (greedy && count > minimum)
                                || (!greedy && count < maximum && position != self.input.len())
                            {
                                self.push(
                                    Frame::Single {
                                        pc,
                                        count,
                                        position,
                                        minimum,
                                        maximum,
                                        greedy,
                                    },
                                    32,
                                )?;
                            }
                            self.pc = exit;
                            self.position = position;
                            break true;
                        }
                    }
                    Frame::Scope {
                        next,
                        position,
                        positive,
                        atomic,
                        prior_independent,
                    } => {
                        if atomic {
                            // This frame includes the recursive matcher's stopper.
                            // Nested control unwind calls must return here before
                            // the caller handles the atomic result.
                            native_stop = Some(Frame::Scope {
                                next,
                                position,
                                positive,
                                atomic,
                                prior_independent,
                            });
                            break false;
                        }
                        self.position = position;
                        self.pc = next;
                        matched = matched == positive;
                        unwound_lookahead = true;
                        if matched {
                            break true;
                        }
                    }
                    Frame::Condition {
                        yes,
                        no,
                        position,
                        positive,
                    } => {
                        native_stop = Some(Frame::Condition {
                            yes,
                            no,
                            position,
                            positive,
                        });
                        break false;
                    }
                    Frame::EnterCall => {
                        if !matched && let Some(call) = self.calls.pop() {
                            self.position = call.position;
                            self.results = call.prior;
                        }
                    }
                    Frame::ReturnCall { call, internal } => {
                        if !matched {
                            self.results = internal;
                            self.calls.push(call);
                        }
                    }
                    Frame::Commit => {
                        controls.push(ControlUnwind::Commit(matched));
                        unwound_lookahead = false;
                        unwound_alt = false;
                    }
                    Frame::Then => {
                        controls.push(ControlUnwind::Then(matched));
                        unwound_lookahead = false;
                        unwound_alt = false;
                    }
                    Frame::Case(value) => self.icase = value,
                }
            };

            // Finish the source's nested unwind calls before interpreting a native
            // matcher return. Normal choices still perform their capture rollback,
            // byte skipping and state accounting, even when a verb discards them.
            while let Some(control) = controls.pop() {
                match control {
                    ControlUnwind::Commit(argument) => {
                        if resumed && !unwound_lookahead {
                            controls.push(ControlUnwind::Commit(argument));
                            matched = argument;
                            unwound_lookahead = false;
                            unwound_alt = false;
                            continue 'invoke;
                        }
                        if resumed && unwound_lookahead {
                            unwound_lookahead = false;
                            self.push(Frame::Commit, 8)?;
                        }
                        self.independent = false;
                    }
                    ControlUnwind::Then(argument) => {
                        if resumed {
                            controls.push(if unwound_alt {
                                ControlUnwind::FinishThen
                            } else {
                                ControlUnwind::Then(argument)
                            });
                            matched = argument;
                            unwound_lookahead = false;
                            unwound_alt = false;
                            continue 'invoke;
                        }
                    }
                    ControlUnwind::FinishThen => {}
                }
            }

            if let Some(stopper) = native_stop.take() {
                self.native_depth -= 1;
                match stopper {
                    Frame::Scope {
                        next,
                        prior_independent,
                        ..
                    } if !draining => {
                        if !matched && !self.independent {
                            // match_startmark(-3) drains all saved states after a
                            // COMMIT/PRUNE/SKIP escapes its recursive matcher.
                            draining = true;
                        } else {
                            self.independent = prior_independent;
                            if matched {
                                self.pc = next;
                                return Ok(true);
                            }
                            if self.state_count > self.max_state_count {
                                return Err(Error::execution(18));
                            }
                        }
                    }
                    Frame::Condition {
                        yes,
                        no,
                        position,
                        positive,
                    } if !draining => {
                        self.position = position;
                        self.pc = if matched == positive { yes } else { no };
                        return Ok(true);
                    }
                    _ => {}
                }
                matched = false;
                unwound_lookahead = false;
                unwound_alt = false;
                continue 'invoke;
            }
            if draining {
                if self.stack.is_empty() {
                    if self.state_count > self.max_state_count {
                        return Err(Error::execution(18));
                    }
                    return Ok(false);
                }
                matched = false;
                unwound_lookahead = false;
                unwound_alt = false;
                continue 'invoke;
            }
            return Ok(resumed);
        }
    }

    fn close_capture(&mut self, index: usize) {
        self.results.captures[index].end = self.position;
        self.results.captures[index].matched = true;
        self.results.last_closed = Some(index);
    }

    fn return_call(&mut self) -> Result<(), Error> {
        let call = self.calls.pop().expect("return has a call");
        let numbered = call.index != 0;
        let internal = if numbered {
            call.prior.clone()
        } else {
            self.results.clone()
        };
        self.results = call.prior.clone();
        // The retained endmark advances past the return case-toggle; the root
        // match state returns directly to it. These are distinct source paths.
        self.pc = if numbered {
            self.engine.code[call.return_pc].next
        } else {
            call.return_pc
        };
        let id = -2 - call.index.cast_signed();
        self.push(Frame::ReturnCall { call, internal }, 184)?;
        if numbered {
            self.push(
                Frame::Counter {
                    id,
                    count: 0,
                    position: self.position,
                },
                48,
            )?;
        }
        Ok(())
    }

    // ACCEPT follows lexical states, closing already-open groups and crossing
    // assertion barriers, without executing the skipped consuming states.
    fn skip_until_group(&mut self, target: usize, have_match: bool) -> Result<(), Error> {
        loop {
            let instruction = &self.engine.code[self.pc];
            match instruction.kind {
                Kind::Match => return Ok(()),
                Kind::End(index) => {
                    if index == target && !have_match {
                        self.pc = instruction.next;
                        return Ok(());
                    }
                    if index > 0 {
                        self.close_capture(index);
                        if self.calls.last().is_some_and(|call| call.index == index) {
                            self.return_call()?;
                            return Ok(());
                        }
                    }
                    self.pc = instruction.next;
                    if index == target {
                        return Ok(());
                    }
                }
                Kind::ScopeEnd => {
                    let next = instruction.next;
                    if !self.unwind(true)? {
                        self.pc = next;
                    }
                }
                Kind::Start(index) => {
                    if index > 0 {
                        self.push(
                            Frame::Capture(index, self.results.captures[index].clone()),
                            40,
                        )?;
                        self.results.captures[index].start = self.position;
                    }
                    self.pc = instruction.next;
                    self.skip_until_group(index, false)?;
                }
                _ => self.pc = instruction.linear_next,
            }
        }
    }

    fn finish(&mut self) -> Match {
        self.results.captures[0].end = self.position;
        self.results.captures[0].matched = true;
        Match {
            formatting_captures: self
                .results
                .captures
                .iter()
                .map(|capture| capture.start..capture.end)
                .collect(),
            captures: self
                .results
                .captures
                .iter()
                .map(|capture| capture.matched.then_some(capture.start..capture.end))
                .collect(),
            last_closed: self.results.last_closed,
        }
    }

    fn condition(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Capture(reference) => self
                .engine
                .resolve(reference, Some(&self.results.captures))
                .is_some_and(|index| self.results.captures[index].matched),
            Condition::Recursion(None) => !self.calls.is_empty(),
            Condition::Recursion(Some(reference)) => {
                self.calls.last().is_some_and(|call| match reference {
                    Reference::Number(index) => call.index == *index,
                    Reference::Name(name) => self.engine.names.iter().any(|(candidate, index)| {
                        name_hash(candidate) == name_hash(name) && *index == call.index
                    }),
                })
            }
            Condition::Define => false,
            Condition::Assertion(_) => unreachable!("assertions execute through a scope"),
        }
    }

    fn anchor(&self, anchor: Anchor) -> bool {
        let previous = self
            .position
            .checked_sub(1)
            .and_then(|index| self.input.get(index))
            .copied();
        let current = self.input.get(self.position).copied();
        let word_before =
            previous.is_some_and(|b| self.rules.is_class(b, b"word").unwrap_or(false));
        let word_after = current.is_some_and(|b| self.rules.is_class(b, b"word").unwrap_or(false));
        match anchor {
            Anchor::LineStart => {
                self.position == 0
                    || (previous.is_some_and(separator)
                        && !(previous == Some(b'\r') && current == Some(b'\n')))
            }
            Anchor::LineEnd => {
                current.is_none()
                    || (current.is_some_and(separator)
                        && !(previous == Some(b'\r') && current == Some(b'\n')))
            }
            Anchor::BufferStart => self.position == 0,
            Anchor::BufferEnd => self.position == self.input.len(),
            Anchor::SoftEnd => self.input[self.position..].iter().copied().all(separator),
            Anchor::WordBoundary => word_before != word_after,
            Anchor::NotWordBoundary => word_before == word_after,
            Anchor::WordStart => !word_before && word_after,
            Anchor::WordEnd => word_before && !word_after,
            Anchor::Continue => self.position == self.search_base,
        }
    }
}

fn equal_byte(left: u8, right: u8, icase: bool, rules: &CharacterRules) -> bool {
    if icase {
        rules.lower(left) == rules.lower(right)
    } else {
        left == right
    }
}

fn separator(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r' | 12)
}

pub(crate) fn atom_match(
    atom: &Atom,
    input: &[u8],
    position: usize,
    rules: &CharacterRules,
) -> Option<usize> {
    match atom {
        Atom::Byte(byte, icase) => input
            .get(position)
            .filter(|&&actual| equal_byte(actual, *byte, *icase, rules))
            .map(|_| position + 1),
        Atom::Dot(newline) => input
            .get(position)
            .filter(|&&byte| *newline || !separator(byte))
            .map(|_| position + 1),
        Atom::Set(set) => set_match(set, input, position, rules),
    }
}

pub(crate) fn set_match(
    set: &ByteSet,
    input: &[u8],
    position: usize,
    rules: &CharacterRules,
) -> Option<usize> {
    let &byte = input.get(position)?;
    let value = if set.icase { rules.lower(byte) } else { byte };
    let mut width = usize::from(set.map[usize::from(value)]);
    for sequence in &set.sequences {
        if input
            .get(position..position + sequence.len())
            .is_some_and(|part| {
                part.iter()
                    .zip(sequence)
                    .all(|(&a, &b)| equal_byte(a, b, set.icase, rules))
            })
        {
            width = sequence.len().max(1);
            break;
        }
    }
    if width == 0 && set.long {
        let value = [value];
        if set.ranges.iter().any(|(low, high)| {
            signed_compare(&value, low).is_ge() && signed_compare(&value, high).is_le()
        }) {
            width = 1;
        }
        if width == 0 && !set.equivalents.is_empty() {
            let primary = rules.transform_primary(&value);
            if set
                .equivalents
                .iter()
                .any(|key| signed_compare(&primary, key).is_eq())
            {
                width = 1;
            }
        }
    }
    if set.negated {
        (width == 0).then_some(position + 1)
    } else {
        (width != 0).then_some(position + width)
    }
}

fn signed_compare(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        left.cmp(right)
    } else {
        left.iter()
            .map(|&b| b.cast_signed())
            .cmp(right.iter().map(|&b| b.cast_signed()))
    }
}

fn maximum_states(input_len: usize, pattern_len: usize) -> usize {
    let n = input_len.max(1);
    let s = pattern_len.max(1);
    let first = s
        .checked_mul(s)
        .and_then(|v| v.checked_mul(n))
        .and_then(|v| v.checked_add(100_000));
    let second = n.checked_mul(n).and_then(|v| v.checked_add(100_000));
    match (first, second) {
        (Some(first), Some(second))
            if isize::try_from(first).is_ok() && isize::try_from(second).is_ok() =>
        {
            first.max(second.min(100_000_000))
        }
        _ => 100_000_000.min(isize::MAX as usize - 2),
    }
}

#[cfg(test)]
mod tests {
    use super::maximum_states;

    #[test]
    fn reference_budget_caps_only_the_quadratic_input_term() {
        assert_eq!(maximum_states(0, 0), 100_001);
        assert_eq!(maximum_states(100, 20), 140_000);
        assert_eq!(maximum_states(10_000, 200), 400_100_000);
        assert_eq!(maximum_states(usize::MAX, 2), 100_000_000);
    }
}
