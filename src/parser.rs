// Ported from boost/regex/v5/basic_regex_parser.hpp and basic_regex_creator.hpp.
// Distributed under the Boost Software License, Version 1.0.

use crate::ast::{Anchor, ByteSet, Condition, Node, NodeId, Program, Reference, Verb, name_hash};
use crate::character::CharacterRules;
use crate::error::Error;

// These are the four independently selectable retained inline option flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct Options {
    icase: bool,
    multiline: bool,
    dot_newline: bool,
    extended: bool,
}

pub(crate) fn parse(pattern: &[u8], rules: &CharacterRules) -> Result<Program, Error> {
    parse_inner(pattern, rules).map_err(|error| *error)
}

// Keep recursive success/error returns small even in unoptimized builds. The
// source accepts 399 nested groups, including on platforms with small stacks.
type ParseResult<T> = Result<T, Box<Error>>;

fn parse_inner(pattern: &[u8], rules: &CharacterRules) -> ParseResult<Program> {
    let mut parser = Parser {
        pattern,
        rules,
        pos: 0,
        nodes: Vec::new(),
        groups: vec![None],
        names: Vec::new(),
        mark_count: 0,
        max_backref: 0,
        depth: 0,
        has_case_change: false,
        options: Options {
            icase: false,
            multiline: true,
            dot_newline: true,
            extended: false,
        },
    };
    let root = parser.expression(None)?;
    if parser.pos != pattern.len() {
        return Err(parser.error(
            8,
            parser.pos,
            Some(b"Found a closing ) with no corresponding opening parenthesis."),
        ));
    }
    if parser.max_backref > parser.mark_count {
        return Err(parser.error(
            6,
            parser.pos,
            Some(b"Found a backreference to a non-existant sub-expression."),
        ));
    }
    parser.groups[0] = Some(root);
    parser.finish()?;
    Ok(Program {
        nodes: parser.nodes,
        root,
        groups: parser.groups,
        names: parser.names,
        mark_count: parser.mark_count,
        pattern_len: pattern.len(),
    })
}

struct Parser<'a> {
    pattern: &'a [u8],
    rules: &'a CharacterRules,
    pos: usize,
    nodes: Vec<Node>,
    groups: Vec<Option<NodeId>>,
    names: Vec<(Vec<u8>, usize)>,
    mark_count: usize,
    max_backref: usize,
    depth: usize,
    options: Options,
    has_case_change: bool,
}

impl Parser<'_> {
    // Boxing here keeps every recursive Result temporary pointer-sized. The
    // one-MiB nesting regression guards this native-stack requirement.
    #[allow(clippy::unnecessary_box_returns)]
    fn error(&self, code: u32, position: usize, message: Option<&[u8]>) -> Box<Error> {
        Box::new(Error::compile(code, position, self.pattern, message, None))
    }
    fn node(&mut self, node: Node) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }
    fn peek(&self) -> Option<u8> {
        self.pattern.get(self.pos).copied()
    }
    fn space(&self, byte: u8) -> bool {
        self.rules.is_class(byte, b"s").unwrap_or(false)
    }
    fn skip_space(&mut self) {
        while self.peek().is_some_and(|b| self.space(b)) {
            self.pos += 1;
        }
    }
    fn number(&mut self, radix: u32, end: usize) -> i64 {
        self.rules.toi(
            self.pattern,
            &mut self.pos,
            end.min(self.pattern.len()),
            radix,
        )
    }
    fn char_max() -> i64 {
        if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            255
        } else {
            127
        }
    }
    fn literal(&mut self, byte: u8) -> NodeId {
        self.node(Node::Literal {
            bytes: vec![byte],
            icase: self.options.icase,
        })
    }
    fn sequence(&mut self, items: Vec<NodeId>) -> NodeId {
        if items.is_empty() {
            return self.node(Node::Empty);
        }
        if items.len() == 1 {
            return items[0];
        }
        self.node(Node::Sequence(items))
    }
    fn push(&mut self, items: &mut Vec<NodeId>, item: NodeId) {
        if matches!(&self.nodes[item],Node::Literal{bytes,..} if bytes.is_empty()) {
            return;
        }
        if let Some(&previous) = items.last()
            && let (
                Node::Literal {
                    bytes: old,
                    icase: a,
                },
                Node::Literal {
                    bytes: new,
                    icase: b,
                },
            ) = (&self.nodes[previous], &self.nodes[item])
            && a == b
        {
            let mut combined = old.clone();
            combined.extend_from_slice(new);
            self.nodes[previous] = Node::Literal {
                bytes: combined,
                icase: *a,
            };
            return;
        }
        items.push(item);
    }

    fn expression(&mut self, branch_reset: Option<usize>) -> ParseResult<NodeId> {
        self.expression_from(branch_reset, None)
    }

    fn expression_from(
        &mut self,
        branch_reset: Option<usize>,
        initial: Option<NodeId>,
    ) -> ParseResult<NodeId> {
        self.depth += 1;
        if self.depth > 400 {
            return Err(self.error(18, self.pos, Some(b"Exceeded nested brace limit.")));
        }
        let mut branches = Vec::new();
        let mut items = initial.map_or_else(Vec::new, |id| vec![id]);
        let mut max_mark = self.mark_count;
        while let Some(byte) = self.peek() {
            if byte == b')' {
                break;
            }
            if self.options.extended && self.space(byte) {
                self.pos += 1;
                continue;
            }
            if self.options.extended && byte == b'#' {
                while let Some(b) = self.peek() {
                    self.pos += 1;
                    if matches!(b, b'\n' | b'\r' | 12) {
                        break;
                    }
                }
                continue;
            }
            if byte == b'|' {
                self.next_branch(&mut branches, &mut items, &mut max_mark, branch_reset);
                continue;
            }
            if matches!(byte, b'*' | b'+' | b'?') {
                self.simple_repeat(&mut items, byte)?;
                continue;
            }
            if byte == b'{' {
                self.bounded_repeat(&mut items)?;
                continue;
            }
            let item = if byte == b'(' {
                self.group()?
            } else {
                self.atom()?
            };
            if let Some(item) = item {
                self.push(&mut items, item);
            }
        }
        Ok(self.finish_expression(branches, items, max_mark))
    }

    fn next_branch(
        &mut self,
        branches: &mut Vec<NodeId>,
        items: &mut Vec<NodeId>,
        max_mark: &mut usize,
        branch_reset: Option<usize>,
    ) {
        self.pos += 1;
        branches.push(self.sequence(std::mem::take(items)));
        *max_mark = (*max_mark).max(self.mark_count);
        if let Some(mark) = branch_reset {
            self.mark_count = mark;
        }
        if self.has_case_change {
            items.push(self.node(Node::ToggleCase(self.options.icase)));
        }
    }

    fn simple_repeat(&mut self, items: &mut Vec<NodeId>, byte: u8) -> ParseResult<()> {
        if self.pos == 0 {
            let message = format!(
                "The repeat operator \"{}\" cannot start a regular expression.",
                char::from(byte)
            );
            return Err(self.error(13, 0, Some(message.as_bytes())));
        }
        self.pos += 1;
        let (min, max) = match byte {
            b'*' => (0, usize::MAX),
            b'+' => (1, usize::MAX),
            _ => (0, 1),
        };
        self.repeat(items, min, max)
    }

    fn bounded_repeat(&mut self, items: &mut Vec<NodeId>) -> ParseResult<()> {
        if let Some((min, max)) = self.repeat_range()? {
            self.repeat(items, min, max)?;
        } else {
            let item = self.literal(b'{');
            self.pos += 1;
            self.push(items, item);
        }
        Ok(())
    }

    fn finish_expression(
        &mut self,
        mut branches: Vec<NodeId>,
        items: Vec<NodeId>,
        max_mark: usize,
    ) -> NodeId {
        self.mark_count = max_mark.max(self.mark_count);
        branches.push(self.sequence(items));
        self.depth -= 1;
        if branches.len() == 1 {
            branches[0]
        } else {
            self.node(Node::Alternation(branches))
        }
    }

    fn atom(&mut self) -> ParseResult<Option<NodeId>> {
        let byte = self.pattern[self.pos];
        let id = match byte {
            b'[' => self.set()?,
            b'\\' => self.escape()?,
            b'.' => {
                self.pos += 1;
                self.node(Node::Dot {
                    newline: self.options.dot_newline,
                })
            }
            b'^' => {
                self.pos += 1;
                self.node(Node::Anchor(if self.options.multiline {
                    Anchor::LineStart
                } else {
                    Anchor::BufferStart
                }))
            }
            b'$' => {
                self.pos += 1;
                self.node(Node::Anchor(if self.options.multiline {
                    Anchor::LineEnd
                } else {
                    Anchor::BufferEnd
                }))
            }
            _ => {
                self.pos += 1;
                self.literal(byte)
            }
        };
        Ok(Some(id))
    }

    fn repeat_range(&mut self) -> ParseResult<Option<(usize, usize)>> {
        let start = self.pos;
        self.pos += 1;
        self.skip_space();
        let low = self.number(10, self.pattern.len());
        self.skip_space();
        if low < 0 || self.peek().is_none() {
            self.pos = start;
            return Ok(None);
        }
        let mut high = usize::try_from(low).expect("validated nonnegative bound");
        if self.peek() == Some(b',') {
            self.pos += 1;
            self.skip_space();
            if self.peek().is_none() {
                self.pos = start;
                return Ok(None);
            }
            let n = self.number(10, self.pattern.len());
            high = if (0..i64::MAX).contains(&n) {
                usize::try_from(n).expect("validated nonnegative integer")
            } else {
                usize::MAX
            };
        }
        self.skip_space();
        if self.peek() != Some(b'}') {
            self.pos = start;
            return Ok(None);
        }
        self.pos += 1;
        if usize::try_from(low).expect("validated nonnegative bound") > high {
            let mut at = self.pos.saturating_sub(2);
            while at > 0 && self.rules.is_class(self.pattern[at], b"w").unwrap_or(false) {
                at -= 1;
            }
            return Err(self.error(10, at + 1, None));
        }
        Ok(Some((
            usize::try_from(low).expect("validated nonnegative bound"),
            high,
        )))
    }

    fn repeat(&mut self, items: &mut Vec<NodeId>, min: usize, max: usize) -> ParseResult<()> {
        if self.options.extended {
            self.skip_space();
        }
        let greedy = if self.peek() == Some(b'?') {
            self.pos += 1;
            false
        } else {
            true
        };
        let possessive = if self.peek() == Some(b'+') {
            self.pos += 1;
            true
        } else {
            false
        };
        let Some(mut body) = items.pop() else {
            return Err(self.error(13, self.pos, None));
        };
        if matches!(
            &self.nodes[body],
            Node::Anchor(_) | Node::ResetStart | Node::ToggleCase(_) | Node::Repeat { .. }
        ) {
            return Err(self.error(13, self.pos, None));
        }
        if let Node::Literal { bytes, icase } = &self.nodes[body]
            && bytes.len() > 1
        {
            let mut prefix = bytes.clone();
            let last = prefix.pop().expect("nonempty literal");
            let case = *icase;
            self.nodes[body] = Node::Literal {
                bytes: prefix,
                icase: case,
            };
            items.push(body);
            body = self.node(Node::Literal {
                bytes: vec![last],
                icase: case,
            });
        }
        if possessive {
            loop {
                if self.options.extended {
                    self.skip_space();
                }
                if self.pattern.get(self.pos..self.pos + 3) == Some(b"(?#") {
                    self.pos += 3;
                    while self.peek().is_some_and(|b| b != b')') {
                        self.pos += 1;
                    }
                    if self.peek().is_some() {
                        self.pos += 1;
                    }
                    continue;
                }
                if self.options.extended && self.peek() == Some(b'#') {
                    while let Some(b) = self.peek() {
                        self.pos += 1;
                        if matches!(b, b'\n' | b'\r' | 12) {
                            break;
                        }
                    }
                    continue;
                }
                break;
            }
            if self
                .peek()
                .is_some_and(|b| matches!(b, b'*' | b'+' | b'?' | b'{'))
            {
                return Err(self.error(13, self.pos, None));
            }
        }
        items.push(self.node(Node::Repeat {
            body,
            min,
            max,
            greedy,
            possessive,
        }));
        Ok(())
    }

    // Parse the header separately from recursive group bodies. Keeping this
    // dispatch frame small preserves the source's 400-level parser limit on
    // native debug stacks without changing that logical limit.
    fn group(&mut self) -> ParseResult<Option<NodeId>> {
        let start = self.pos;
        self.pos += 1;
        if self.peek().is_none() {
            return Err(self.error(8, self.pos, None));
        }
        if self.peek() == Some(b'*') {
            return self.verb_group(start).map(Some);
        }
        let old_options = self.options;
        let old_case_change = self.has_case_change;
        self.has_case_change = false;
        if self.peek() != Some(b'?') {
            let index = self.new_capture();
            return self
                .capture_group(index, old_options, old_case_change)
                .map(Some);
        }
        self.pos += 1;
        let Some(ext) = self.peek() else {
            return Err(self.error(20, start, None));
        };
        if ext == b'#' {
            self.pos += 1;
            while self.peek().is_some_and(|b| b != b')') {
                self.pos += 1;
            }
            if self.peek().is_some() {
                self.pos += 1;
            }
            self.has_case_change = old_case_change;
            return Ok(None);
        }
        let node = match ext {
            b')' => return Err(self.error(20, start, None)),
            b':' | b'|' => {
                self.pos += 1;
                let reset = (ext == b'|').then_some(self.mark_count);
                self.non_capture_group(reset, old_options, old_case_change)?
            }
            b'=' | b'!' | b'>' => {
                self.pos += 1;
                self.assertion_group(start, ext, false, old_options, old_case_change)?
            }
            b'<' if self
                .pattern
                .get(self.pos + 1)
                .is_some_and(|b| matches!(b, b'=' | b'!')) =>
            {
                let assertion = self.pattern[self.pos + 1];
                self.pos += 2;
                self.assertion_group(start, assertion, true, old_options, old_case_change)?
            }
            b'<' | b'\'' => {
                let index = self.capture_name(start, ext)?;
                self.capture_group(index, old_options, old_case_change)?
            }
            b'(' => {
                let node = self.conditional(start)?;
                self.options = old_options;
                self.has_case_change = old_case_change;
                node
            }
            b'R' | b'&' | b'0'..=b'9' | b'+' => {
                self.has_case_change = old_case_change;
                return self.recurse_group(start, ext);
            }
            b'-' if self
                .pattern
                .get(self.pos + 1)
                .is_some_and(u8::is_ascii_digit) =>
            {
                self.has_case_change = old_case_change;
                return self.recurse_group(start, ext);
            }
            b'P' if self.pattern.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 1;
                self.has_case_change = old_case_change;
                return self.recurse_group(start, b'&');
            }
            _ => self.option_group(start, ext, old_options, old_case_change)?,
        };
        Ok(Some(node))
    }

    fn verb_group(&mut self, start: usize) -> ParseResult<NodeId> {
        self.pos += 1;
        let name_start = self.pos;
        while self.peek().is_some_and(|b| b != b')') {
            self.pos += 1;
        }
        let verb = match self.pattern.get(name_start..self.pos).unwrap_or_default() {
            b"F" | b"FAIL" => Verb::Fail,
            b"ACCEPT" => Verb::Accept,
            b"COMMIT" => Verb::Commit,
            b"PRUNE" => Verb::Prune,
            b"SKIP" => Verb::Skip,
            b"THEN" => Verb::Then,
            _ => return Err(self.error(20, start, None)),
        };
        if self.peek().is_none() {
            return Err(self.error(20, start, None));
        }
        self.pos += 1;
        Ok(self.node(Node::Verb(verb)))
    }

    fn capture_group(
        &mut self,
        index: usize,
        options: Options,
        changed: bool,
    ) -> ParseResult<NodeId> {
        let body = self.expression(None)?;
        self.close_group()?;
        let body = self.restore_scope(body, options, changed);
        let node = self.node(Node::Capture { index, body });
        if self.groups[index].is_none() {
            self.groups[index] = Some(node);
        }
        Ok(node)
    }

    fn capture_name(&mut self, start: usize, ext: u8) -> ParseResult<usize> {
        let delim = if ext == b'<' { b'>' } else { b'\'' };
        self.pos += 1;
        let name_start = self.pos;
        while self.peek().is_some_and(|b| b != delim) {
            self.pos += 1;
        }
        if self.peek().is_none() {
            return Err(self.error(20, start, None));
        }
        let name = self.pattern[name_start..self.pos].to_vec();
        self.pos += 1;
        let index = self.new_capture();
        self.names.push((name, index));
        Ok(index)
    }

    fn non_capture_group(
        &mut self,
        reset: Option<usize>,
        options: Options,
        changed: bool,
    ) -> ParseResult<NodeId> {
        let body = self.expression(reset)?;
        self.close_group()?;
        let body = self.restore_scope(body, options, changed);
        Ok(self.node(Node::Group { body }))
    }

    fn assertion_group(
        &mut self,
        start: usize,
        kind: u8,
        behind: bool,
        options: Options,
        changed: bool,
    ) -> ParseResult<NodeId> {
        let body = self.expression(None)?;
        self.close_group()?;
        if !behind && matches!(self.nodes[body], Node::Empty) && kind != b'!' {
            return Err(self.error(20, start, Some(b"Invalid or empty zero width assertion.")));
        }
        let body = self.restore_scope(body, options, changed);
        Ok(self.node(if kind == b'>' {
            Node::Atomic { body }
        } else {
            Node::Assert {
                body,
                positive: kind == b'=',
                behind: behind.then_some(usize::MAX),
            }
        }))
    }

    fn option_group(
        &mut self,
        start: usize,
        ext: u8,
        old_options: Options,
        old_case_change: bool,
    ) -> ParseResult<NodeId> {
        // The retained parser consumes P before falling through to option
        // parsing when this is not the (?P>name) extension.
        if ext == b'P' {
            self.pos += 1;
            if self.peek().is_none() {
                return Err(self.error(20, start, None));
            }
        }
        let mut options = self.options;
        self.parse_options(start, &mut options)?;
        let changed = options.icase != old_options.icase;
        match self.peek() {
            Some(b')') => {
                self.expression(None)?;
                self.close_group()?;
                self.options = options;
                self.has_case_change = old_case_change || changed;
                let body = self.node(if changed {
                    Node::ToggleCase(options.icase)
                } else {
                    Node::Empty
                });
                Ok(self.node(Node::Group { body }))
            }
            Some(b':') => {
                self.pos += 1;
                self.options = options;
                self.has_case_change = changed;
                let body = self.expression(None)?;
                self.close_group()?;
                let body = if changed {
                    let toggle = self.node(Node::ToggleCase(options.icase));
                    self.node(Node::Sequence(vec![toggle, body]))
                } else {
                    body
                };
                let body = self.restore_scope(body, old_options, old_case_change);
                Ok(self.node(Node::Group { body }))
            }
            _ => Err(self.error(20, start, None)),
        }
    }

    fn restore_scope(&mut self, body: NodeId, options: Options, changed: bool) -> NodeId {
        let body = if self.has_case_change {
            let toggle = self.node(Node::ToggleCase(options.icase));
            self.node(Node::Sequence(vec![body, toggle]))
        } else {
            body
        };
        self.options = options;
        self.has_case_change = changed;
        body
    }

    fn new_capture(&mut self) -> usize {
        self.mark_count += 1;
        if self.groups.len() <= self.mark_count {
            self.groups.resize(self.mark_count + 1, None);
        }
        self.mark_count
    }
    fn close_group(&mut self) -> ParseResult<()> {
        if self.peek() != Some(b')') {
            return Err(self.error(8, self.pattern.len(), None));
        }
        self.pos += 1;
        Ok(())
    }
    fn parse_options(&mut self, start: usize, options: &mut Options) -> ParseResult<()> {
        let mut enabled = true;
        loop {
            match self.peek() {
                Some(b'i') => options.icase = enabled,
                Some(b'm') => options.multiline = enabled,
                Some(b's') => options.dot_newline = enabled,
                Some(b'x') => options.extended = enabled,
                Some(b'-') if enabled => {
                    enabled = false;
                    self.pos += 1;
                    if self.peek().is_none() {
                        return Err(self.error(8, start, None));
                    }
                    continue;
                }
                _ => break,
            }
            self.pos += 1;
            if self.peek().is_none() {
                return Err(self.error(8, start, None));
            }
        }
        Ok(())
    }

    fn recurse_group(&mut self, start: usize, kind: u8) -> ParseResult<Option<NodeId>> {
        let target = match kind {
            b'R' => {
                self.pos += 1;
                Reference::Number(0)
            }
            b'&' => {
                self.pos += 1;
                let begin = self.pos;
                while self.peek().is_some_and(|b| b != b')') {
                    self.pos += 1;
                }
                Reference::Name(self.pattern[begin..self.pos].to_vec())
            }
            b'+' | b'-' => {
                self.pos += 1;
                let n = self.number(10, self.pattern.len());
                if n <= 0 {
                    if kind == b'-' {
                        return Err(self.error(20, start, None));
                    }
                    return Err(self.error(
                        20,
                        start,
                        Some(b"An invalid or unterminated recursive sub-expression."),
                    ));
                }
                let index = if kind == b'+' {
                    self.mark_count
                        .checked_add(usize::try_from(n).expect("validated nonnegative integer"))
                } else {
                    (self.mark_count + 1)
                        .checked_sub(usize::try_from(n).expect("validated nonnegative integer"))
                };
                let Some(index) = index.filter(|&v| v > 0) else {
                    return Err(self.error(
                        20,
                        start,
                        Some(b"An invalid or unterminated recursive sub-expression."),
                    ));
                };
                Reference::Number(index)
            }
            _ => {
                let n = self.number(10, self.pattern.len());
                if n < 0 {
                    return Err(self.error(20,start,Some(b"The recursive sub-expression refers to an invalid marking group, or is unterminated.")));
                }
                Reference::Number(usize::try_from(n).expect("validated nonnegative integer"))
            }
        };
        if kind != b'-' && self.peek() != Some(b')') {
            let msg = if kind.is_ascii_digit() {
                Some(&b"The recursive sub-expression refers to an invalid marking group, or is unterminated."[..])
            } else if matches!(kind, b'+' | b'-') {
                Some(&b"An invalid or unterminated recursive sub-expression."[..])
            } else {
                None
            };
            return Err(self.error(20, start, msg));
        }
        let old_options = self.options;
        let old_change = self.has_case_change;
        self.has_case_change = false;
        let recurse = self.node(Node::Recurse(target));
        let toggle = self.node(Node::ToggleCase(self.options.icase));
        let tail = self.expression(None)?;
        self.close_group()?;
        let body = self.node(Node::Sequence(vec![recurse, toggle, tail]));
        let body = self.restore_scope(body, old_options, old_change);
        Ok(Some(self.node(Node::Group { body })))
    }

    fn conditional(&mut self, start: usize) -> ParseResult<NodeId> {
        let condition_start = self.pos;
        self.pos += 1;
        if self.peek().is_none() {
            return Err(self.error(20, condition_start, None));
        }
        let condition = if self.peek() == Some(b'?') {
            let assertion_kind = self.pattern.get(self.pos + 1).copied();
            let valid = matches!(assertion_kind, Some(b'=' | b'!'))
                || (assertion_kind == Some(b'<')
                    && self
                        .pattern
                        .get(self.pos + 2)
                        .is_some_and(|b| matches!(b, b'=' | b'!')));
            if !valid {
                return Err(self.error(20, condition_start, None));
            }
            self.pos = condition_start;
            // The source parses the assertion as the first state of the
            // conditional body's parse_all call, so both scopes count while
            // the assertion is being read.
            self.depth += 1;
            if self.depth > 400 {
                return Err(self.error(18, self.pos, Some(b"Exceeded nested brace limit.")));
            }
            let assertion = self.group()?.expect("validated assertion condition");
            self.depth -= 1;
            Condition::Assertion(assertion)
        } else {
            self.condition_token(condition_start)?
        };
        let prefix = if let Condition::Assertion(assertion) = condition {
            assertion
        } else {
            self.node(Node::ConditionTest(condition.clone()))
        };
        let body = self.expression_from(None, Some(prefix))?;
        self.close_group()?;
        self.finish_condition(start, condition, body, prefix)
    }

    fn condition_token(&mut self, condition_start: usize) -> ParseResult<Condition> {
        let cond = match self.peek() {
            Some(b'R') => {
                self.pos += 1;
                if self.peek() == Some(b'&') {
                    self.pos += 1;
                    let name_start = self.pos;
                    while self.peek().is_some_and(|b| b != b')') {
                        self.pos += 1;
                    }
                    Condition::Recursion(Some(Reference::Name(
                        self.pattern[name_start..self.pos].to_vec(),
                    )))
                } else {
                    let n = self.number(10, self.pattern.len());
                    Condition::Recursion(if n > 0 {
                        Some(Reference::Number(
                            usize::try_from(n).expect("validated nonnegative integer"),
                        ))
                    } else {
                        None
                    })
                }
            }
            Some(b'\'' | b'<') => {
                self.pos += 1;
                let name_start = self.pos;
                while self.peek().is_some_and(|b| !matches!(b, b'\'' | b'>')) {
                    self.pos += 1;
                }
                if self.peek().is_none() {
                    return Err(self.error(20, condition_start, None));
                }
                let name = self.pattern[name_start..self.pos].to_vec();
                self.pos += 1;
                if self.peek().is_none() {
                    return Err(self.error(
                        20,
                        condition_start,
                        Some(b"Unterminated named capture."),
                    ));
                }
                Condition::Capture(Reference::Name(name))
            }
            Some(b'D') if self.pattern.get(self.pos..self.pos + 6) == Some(b"DEFINE") => {
                self.pos += 6;
                Condition::Define
            }
            _ => {
                let n = self.number(10, self.pattern.len());
                if n <= 0 {
                    return Err(self.error(20, condition_start, None));
                }
                Condition::Capture(Reference::Number(
                    usize::try_from(n).expect("validated nonnegative integer"),
                ))
            }
        };
        if self.peek() != Some(b')') {
            return Err(self.error(20, condition_start, None));
        }
        self.pos += 1;
        if self.peek().is_none() {
            return Err(self.error(20, condition_start, None));
        }
        Ok(cond)
    }

    fn finish_condition(
        &mut self,
        start: usize,
        mut condition: Condition,
        body: NodeId,
        prefix: NodeId,
    ) -> ParseResult<NodeId> {
        let error_pos = self.pattern[..self.pos]
            .iter()
            .rposition(|&b| b == b'(')
            .unwrap_or(start);
        let explicit_else = matches!(&self.nodes[body], Node::Alternation(_));
        let (first, no) = if let Node::Alternation(branches) = &self.nodes[body] {
            if branches.len() > 2 {
                return Err(self.error(2, error_pos, Some(b"More than one alternation operator | was encountered inside a conditional expression.")));
            }
            (branches[0], branches[1])
        } else {
            (body, self.node(Node::Empty))
        };
        let (first_condition, yes) = if let Node::Sequence(items) = &self.nodes[first] {
            let first = items[0];
            let rest = items[1..].to_vec();
            (first, self.sequence(rest))
        } else {
            (first, self.node(Node::Empty))
        };
        if first_condition == prefix {
            if matches!(condition, Condition::Define) && explicit_else {
                return Err(self.error(
                    2,
                    error_pos,
                    Some(b"Alternation operators are not allowed inside a DEFINE block."),
                ));
            }
        } else if let Node::Repeat {
            body,
            min,
            max,
            greedy,
            possessive: true,
        } = self.nodes[first_condition]
        {
            // A possessive condition is retained by the source: its inserted
            // atomic startmark satisfies the condition-state validation. The
            // matcher enters that wrapper's body as a positive assertion.
            let repeat = self.node(Node::Repeat {
                body,
                min,
                max,
                greedy,
                possessive: false,
            });
            condition = Condition::Assertion(self.node(Node::Atomic { body: repeat }));
        } else {
            return Err(self.error(
                13,
                error_pos,
                Some(b"A repetition operator cannot be applied to a zero-width assertion."),
            ));
        }
        Ok(self.node(Node::Conditional {
            condition,
            yes,
            no,
            explicit_else,
        }))
    }

    // Keep the source parser dispatch together so error ordering remains visible.
    #[allow(clippy::too_many_lines)]
    fn escape(&mut self) -> ParseResult<NodeId> {
        let start = self.pos;
        self.pos += 1;
        let Some(byte) = self.peek() else {
            return Err(self.error(5, self.pos, Some(b"Incomplete escape sequence found.")));
        };
        let anchor = match byte {
            b'<' => Some(Anchor::WordStart),
            b'>' => Some(Anchor::WordEnd),
            b'A' | b'`' => Some(Anchor::BufferStart),
            b'z' | b'\'' => Some(Anchor::BufferEnd),
            b'Z' => Some(Anchor::SoftEnd),
            b'b' => Some(Anchor::WordBoundary),
            b'B' => Some(Anchor::NotWordBoundary),
            b'G' => Some(Anchor::Continue),
            _ => None,
        };
        if let Some(anchor) = anchor {
            self.pos += 1;
            return Ok(self.node(Node::Anchor(anchor)));
        }
        match byte {
            b'1'..=b'9' => {
                self.pos += 1;
                let index = usize::from(byte - b'0');
                self.max_backref = self.max_backref.max(index);
                Ok(self.node(Node::Backref {
                    target: Reference::Number(index),
                    icase: self.options.icase,
                }))
            }
            b'g' | b'k' => self.extended_backref(),
            b'K' => {
                self.pos += 1;
                Ok(self.node(Node::ResetStart))
            }
            b'C' => {
                self.pos += 1;
                Ok(self.node(Node::Dot {
                    newline: self.options.dot_newline,
                }))
            }
            b'X' => {
                self.pos += 1;
                Ok(self.node(Node::Combining))
            }
            b'Q' => {
                self.pos += 1;
                let begin = self.pos;
                while self.pos < self.pattern.len() {
                    if self.pattern[self.pos] == b'\\' {
                        if self.pos + 1 == self.pattern.len() {
                            return Err(self.error(
                                5,
                                self.pattern.len(),
                                Some(b"Unterminated \\Q...\\E sequence."),
                            ));
                        }
                        if self.pattern[self.pos + 1] == b'E' {
                            let bytes = self.pattern[begin..self.pos].to_vec();
                            self.pos += 2;
                            return Ok(self.node(Node::Literal {
                                bytes,
                                icase: self.options.icase,
                            }));
                        }
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                Ok(self.node(Node::Literal {
                    bytes: self.pattern[begin..self.pos].to_vec(),
                    icase: self.options.icase,
                }))
            }
            b'R' => {
                // The retained byte expansion runs parse_all, an option group,
                // and an atomic group. Its own source buffer appears in a
                // nesting-limit diagnostic, rather than the caller's pattern.
                if self.depth >= 398 {
                    const EXPANSION: &[u8] = b"(?-x:(?>\r\n?|[\n\x0b\x0c\x85]))";
                    let position = match self.depth {
                        400 => 0,
                        399 => 5,
                        _ => 8,
                    };
                    return Err(Box::new(Error::compile(
                        18,
                        position,
                        EXPANSION,
                        Some(b"Exceeded nested brace limit."),
                        None,
                    )));
                }
                self.pos += 1;
                let cr = self.literal(b'\r');
                let lf = self.literal(b'\n');
                let repeat = self.node(Node::Repeat {
                    body: lf,
                    min: 0,
                    max: 1,
                    greedy: true,
                    possessive: false,
                });
                let seq = self.node(Node::Sequence(vec![cr, repeat]));
                let mut set = ByteSet::new(self.options.icase);
                for c in [b'\n', 11, 12, 0x85] {
                    set.map[usize::from(c)] = true;
                }
                let set = self.node(Node::Set(set));
                let alt = self.node(Node::Alternation(vec![seq, set]));
                let atomic = self.node(Node::Atomic { body: alt });
                Ok(self.node(Node::Group { body: atomic }))
            }
            b'p' | b'P' => {
                self.pos += 1;
                if self.peek().is_none() {
                    return Err(self.error(
                        5,
                        self.pos,
                        Some(b"Incomplete property escape found."),
                    ));
                }
                let name = if self.peek() == Some(b'{') {
                    self.pos += 1;
                    let begin = self.pos;
                    while self.peek().is_some_and(|b| b != b'}') {
                        self.pos += 1;
                    }
                    if self.peek().is_none() {
                        return Err(self.error(
                            5,
                            self.pos,
                            Some(b"Closing } missing from property escape sequence."),
                        ));
                    }
                    let name = self.pattern[begin..self.pos].to_vec();
                    self.pos += 1;
                    name
                } else {
                    let name = vec![self.pattern[self.pos]];
                    self.pos += 1;
                    name
                };
                if self.rules.is_class(0, &name).is_none() {
                    return Err(self.error(4,self.pos,Some(b"Escape sequence was neither a valid property nor a valid character class name.")));
                }
                Ok(self.class_node(&name, byte == b'P'))
            }
            _ => {
                if !matches!(
                    byte,
                    b'a' | b'e' | b'f' | b'n' | b'r' | b't' | b'c' | b'x' | b'N' | b'0'
                ) && self.rules.is_class(0, &[byte]).is_some()
                {
                    self.pos += 1;
                    return Ok(self.class_node(&[byte], byte.is_ascii_uppercase()));
                }
                let value = self.unescape(start)?;
                Ok(self.literal(value))
            }
        }
    }

    fn class_node(&mut self, name: &[u8], negated: bool) -> NodeId {
        let mut set = ByteSet::new(self.options.icase);
        set.negated = negated;
        for byte in 0_u8..=255 {
            set.map[usize::from(byte)] = self.class_value(byte, name);
        }
        self.node(Node::Set(set))
    }
    fn class_value(&self, byte: u8, name: &[u8]) -> bool {
        let value = self.rules.is_class(byte, name).unwrap_or(false);
        let lower = name.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
        value
            || (self.options.icase
                && matches!(&lower[..], b"lower" | b"upper" | b"l" | b"u")
                && self.rules.is_class(byte, b"alpha").unwrap_or(false))
    }

    fn extended_backref(&mut self) -> ParseResult<NodeId> {
        self.pos += 1;
        let incomplete = b"Incomplete \\g escape found.";
        if self.peek().is_none() {
            return Err(self.error(5, self.pos, Some(incomplete)));
        }
        let close = match self.peek() {
            Some(b'{') => Some(b'}'),
            Some(b'<') => Some(b'>'),
            Some(b'\'') => Some(b'\''),
            _ => None,
        };
        if close.is_some() {
            self.pos += 1;
            if self.peek().is_none() {
                return Err(self.error(5, self.pos, Some(incomplete)));
            }
        }
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
            if self.peek().is_none() {
                return Err(self.error(5, self.pos, Some(incomplete)));
            }
        }
        let begin = self.pos;
        let n = self.number(10, self.pattern.len());
        let target = if n < 0 {
            let Some(close) = close else {
                return Err(self.error(6, self.pos, None));
            };
            self.pos = begin;
            while self.peek().is_some_and(|b| b != close) {
                self.pos += 1;
            }
            let name = self.pattern[begin..self.pos].to_vec();
            if !self
                .names
                .iter()
                .any(|(candidate, _)| name_hash(candidate) == name_hash(&name))
            {
                return Err(self.error(6, self.pos, None));
            }
            Reference::Name(name)
        } else {
            let index = if negative {
                (self.mark_count + 1)
                    .saturating_sub(usize::try_from(n).expect("validated nonnegative integer"))
            } else {
                usize::try_from(n).expect("validated nonnegative integer")
            };
            if index == 0 {
                return Err(self.error(6, begin, None));
            }
            self.max_backref = self.max_backref.max(index);
            Reference::Number(index)
        };
        if let Some(close) = close {
            if self.peek() != Some(close) {
                return Err(self.error(5, self.pos, Some(incomplete)));
            }
            self.pos += 1;
        }
        Ok(self.node(Node::Backref {
            target,
            icase: self.options.icase,
        }))
    }

    // Keep the source parser dispatch together so error ordering remains visible.
    #[allow(clippy::too_many_lines)]
    fn unescape(&mut self, start: usize) -> ParseResult<u8> {
        let Some(byte) = self.peek() else {
            return Err(self.error(
                5,
                self.pos,
                Some(b"Escape sequence terminated prematurely."),
            ));
        };
        let result = match byte {
            b'a' => 7,
            b'e' => 27,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            b'b' => 8,
            b'c' => {
                self.pos += 1;
                let Some(value) = self.peek() else {
                    return Err(self.error(
                        5,
                        start,
                        Some(b"ASCII escape sequence terminated prematurely."),
                    ));
                };
                value % 32
            }
            b'x' => {
                self.pos += 1;
                if self.peek().is_none() {
                    return Err(self.error(
                        5,
                        start,
                        Some(b"Hexadecimal escape sequence terminated prematurely."),
                    ));
                }
                if self.peek() == Some(b'{') {
                    self.pos += 1;
                    if self.peek().is_none() {
                        return Err(self.error(
                            5,
                            start,
                            Some(b"Missing } in hexadecimal escape sequence."),
                        ));
                    }
                    let n = self.number(16, self.pattern.len());
                    if n < 0 || n > Self::char_max() || self.peek() != Some(b'}') {
                        return Err(self.error(
                            10,
                            start,
                            Some(b"Hexadecimal escape sequence was invalid."),
                        ));
                    }
                    self.pos += 1;
                    return Ok(u8::try_from(n).expect("validated byte value"));
                }
                let n = self.number(16, self.pos + 2);
                if !(0..=255).contains(&n) {
                    return Err(self.error(
                        5,
                        start,
                        Some(b"Escape sequence did not encode a valid character."),
                    ));
                }
                return Ok(u8::try_from(n).expect("validated byte value"));
            }
            b'0'..=b'9' => {
                if byte != b'0' {
                    return Err(self.error(5, start, Some(b"Invalid octal escape sequence.")));
                }
                let n = self.number(8, self.pos + 4);
                if n < 0 || n > Self::char_max() {
                    return Err(self.error(5, start, Some(b"Octal escape sequence is invalid.")));
                }
                return Ok(u8::try_from(n).expect("validated byte value"));
            }
            b'N' => {
                self.pos += 1;
                if self.peek() != Some(b'{') {
                    return Err(self.error(5, start, None));
                }
                self.pos += 1;
                let begin = self.pos;
                while self.peek().is_some_and(|b| b != b'}') {
                    self.pos += 1;
                }
                if self.peek().is_none() {
                    return Err(self.error(5, start, None));
                }
                let name = &self.pattern[begin..self.pos];
                self.pos += 1;
                let value = self
                    .rules
                    .collating_name(name)
                    .ok_or_else(|| self.error(3, start, None))?;
                if value.len() != 1 {
                    return Err(self.error(5, start, None));
                }
                return Ok(value[0]);
            }
            _ => byte,
        };
        self.pos += 1;
        Ok(result)
    }

    // Keep the source parser dispatch together so error ordering remains visible.
    #[allow(clippy::too_many_lines)]
    fn set(&mut self) -> ParseResult<NodeId> {
        const INCOMPLETE:&[u8]=b"Character set declaration starting with [ terminated prematurely - either no ] was found or the set had no content.";
        const CLASS_INCOMPLETE:&[u8]=b"Character class declaration starting with [ terminated prematurely - either no ] was found or the set had no content.";
        self.pos += 1;
        if self.peek().is_none() {
            return Err(self.error(7, self.pos, Some(INCOMPLETE)));
        }
        let mut set = ByteSet::new(self.options.icase);
        let mut positive_classes = Vec::new();
        let mut negative_classes = Vec::new();
        let mut invalid_range = false;
        let mut singles: Vec<Vec<u8>> = Vec::new();
        if self.peek() == Some(b'^') {
            set.negated = true;
            self.pos += 1;
        }
        let first = self.pos;
        let mut content = false;
        while let Some(byte) = self.peek() {
            if byte == b'[' && self.pos + 1 == self.pattern.len() {
                return Err(self.error(7, self.pos + 1, Some(CLASS_INCOMPLETE)));
            }
            if byte == b']' && self.pos != first {
                self.pos += 1;
                if invalid_range {
                    return Err(self.error(4, self.pos, None));
                }
                set.long = singles.iter().any(|s| s.len() > 1);
                if set.long {
                    singles.sort_by_key(|s| {
                        s.iter()
                            .map(|&b| {
                                if Self::char_max() == 255 {
                                    i16::from(b)
                                } else {
                                    i16::from(b.cast_signed())
                                }
                            })
                            .collect::<Vec<_>>()
                    });
                    singles.dedup();
                    set.sequences = singles.iter().map(|s| self.translate(s)).collect();
                } else {
                    for single in &singles {
                        let value = if self.options.icase {
                            self.rules.lower(single[0])
                        } else {
                            single[0]
                        };
                        for b in 0_u8..=255 {
                            let actual = if self.options.icase {
                                self.rules.lower(b)
                            } else {
                                b
                            };
                            if actual == value {
                                set.map[usize::from(b)] = true;
                            }
                        }
                    }
                    for (low, high) in &set.ranges {
                        for b in 0_u8..=255 {
                            let value = vec![b];
                            if *low <= value && value <= *high {
                                set.map[usize::from(b)] = true;
                            }
                        }
                    }
                    for primary in &set.equivalents {
                        for b in 0_u8..=255 {
                            if self.rules.transform_primary(&[b]) == *primary {
                                set.map[usize::from(b)] = true;
                            }
                        }
                    }
                    set.ranges.clear();
                    set.equivalents.clear();
                }
                for b in 0_u8..=255 {
                    let positive = positive_classes
                        .iter()
                        .any(|name: &Vec<u8>| self.class_value(b, name));
                    let negative = !negative_classes.is_empty()
                        && !negative_classes
                            .iter()
                            .any(|name: &Vec<u8>| self.class_value(b, name));
                    set.map[usize::from(b)] |= positive || negative;
                }
                return Ok(self.node(Node::Set(set)));
            }
            if self.pattern.get(self.pos..self.pos + 2) == Some(b"[:")
                || self.pattern.get(self.pos..self.pos + 2) == Some(b"[=")
            {
                let kind = self.pattern[self.pos + 1];
                self.pos += 2;
                if self.peek().is_none() {
                    return Err(self.error(7, self.pos, Some(CLASS_INCOMPLETE)));
                }
                let mut begin = self.pos;
                self.pos += 1;
                while self.peek().is_some_and(|b| b != kind) {
                    self.pos += 1;
                }
                let end = self.pos;
                if self.peek().is_none() {
                    return Err(self.error(7, self.pos, Some(CLASS_INCOMPLETE)));
                }
                self.pos += 1;
                if self.peek() != Some(b']') {
                    return Err(self.error(7, self.pos, Some(CLASS_INCOMPLETE)));
                }
                self.pos += 1;
                if kind == b':' {
                    let negated = self.pattern.get(begin) == Some(&b'^');
                    if negated {
                        begin += 1;
                    }
                    let name = self.pattern[begin..end].to_vec();
                    if self.rules.is_class(0, &name).is_none() {
                        if !content && matches!(&name[..], b"<" | b">") && self.peek() == Some(b']')
                        {
                            self.pos += 1;
                            return Ok(self.node(Node::Anchor(if name == b"<" {
                                Anchor::WordStart
                            } else {
                                Anchor::WordEnd
                            })));
                        }
                        return Err(self.error(4, begin, None));
                    }
                    if negated {
                        negative_classes.push(name);
                    } else {
                        positive_classes.push(name);
                    }
                } else {
                    let name = &self.pattern[begin..end];
                    let value = self
                        .rules
                        .collating_name(name)
                        .filter(|v| !v.is_empty() && v.len() <= 2)
                        .ok_or_else(|| self.error(3, begin, None))?;
                    let primary = self.rules.transform_primary(&value);
                    if primary.is_empty() {
                        invalid_range = true;
                    } else {
                        set.equivalents.push(primary);
                    }
                    if value.len() > 1 {
                        singles.push(value);
                    }
                }
                content = true;
                continue;
            }
            if byte == b'\\'
                && let Some(&escaped) = self.pattern.get(self.pos + 1)
                && !matches!(
                    escaped,
                    b'a' | b'e'
                        | b'f'
                        | b'n'
                        | b'r'
                        | b't'
                        | b'b'
                        | b'c'
                        | b'x'
                        | b'N'
                        | b'v'
                        | b'0'..=b'9'
                )
                && self.rules.is_class(0, &[escaped]).is_some()
            {
                if escaped.is_ascii_uppercase() {
                    negative_classes.push(vec![escaped]);
                } else {
                    positive_classes.push(vec![escaped]);
                }
                self.pos += 2;
                content = true;
                continue;
            }
            let start = self.set_literal(content)?;
            if self.peek().is_none() {
                return Err(self.error(7, self.pos, None));
            }
            if self.peek() == Some(b'-') && self.pattern.get(self.pos + 1) != Some(&b']') {
                self.pos += 1;
                if self.peek().is_none() {
                    return Err(self.error(7, self.pos, None));
                }
                let end = self.set_literal(content)?;
                let low = self.translate(&start);
                let high = self.translate(&end);
                if low > high {
                    invalid_range = true;
                } else {
                    set.ranges.push((low, high));
                }
                if start.len() > 1 {
                    singles.push(start);
                }
                if end.len() > 1 {
                    singles.push(end);
                }
                if self.peek() == Some(b'-') && self.pattern.get(self.pos + 1) != Some(&b']') {
                    self.pos += 1;
                    return Err(self.error(
                        if self.peek().is_none() { 7 } else { 11 },
                        self.pos,
                        None,
                    ));
                }
            } else {
                singles.push(start);
            }
            content = true;
        }
        // parse_set returning false is propagated by parse_all: at the root it
        // uses the unexpected-closing-parenthesis diagnostic, even at EOF.
        Err(self.error(
            8,
            self.pos,
            if self.depth == 1 {
                Some(b"Found a closing ) with no corresponding opening parenthesis.")
            } else {
                None
            },
        ))
    }
    fn translate(&self, bytes: &[u8]) -> Vec<u8> {
        bytes
            .iter()
            .map(|&b| {
                if self.options.icase {
                    self.rules.lower(b)
                } else {
                    b
                }
            })
            .collect()
    }
    fn set_literal(&mut self, content: bool) -> ParseResult<Vec<u8>> {
        let start = self.pos;
        let Some(byte) = self.peek() else {
            return Err(self.error(7, self.pos, None));
        };
        match byte {
            b'-' if content && self.pattern.get(self.pos + 1) != Some(&b']') => {
                self.pos += 1;
                Err(self.error(11, self.pos, None))
            }
            b'\\' => {
                self.pos += 1;
                Ok(vec![self.unescape(start)?])
            }
            b'[' if self.pos + 1 == self.pattern.len() => {
                self.pos += 1;
                Err(self.error(3, self.pos, None))
            }
            b'[' if self.pattern.get(self.pos + 1) == Some(&b'.') => {
                self.pos += 2;
                if self.peek().is_none() {
                    return Err(self.error(3, self.pos, None));
                }
                let begin = self.pos;
                self.pos += 1;
                while self.peek().is_some_and(|b| b != b'.') {
                    self.pos += 1;
                }
                if self.peek().is_none() {
                    return Err(self.error(3, begin, None));
                }
                let name = &self.pattern[begin..self.pos];
                self.pos += 1;
                if self.peek() != Some(b']') {
                    return Err(self.error(3, begin, None));
                }
                self.pos += 1;
                self.rules
                    .collating_name(name)
                    .filter(|v| !v.is_empty() && v.len() <= 2)
                    .ok_or_else(|| self.error(3, begin, None))
            }
            _ => {
                self.pos += 1;
                Ok(vec![byte])
            }
        }
    }

    fn fixed_width(&self, id: NodeId) -> Option<usize> {
        self.backstep_shape(id).map(|(width, _)| width)
    }

    // calculate_backstep keeps an alternative pending until it reaches the
    // enclosing assertion's end, even across later sequential alternatives.
    // Track that source stack high-water alongside the common path length.
    // Its check is `size > 1024` before push, so 1025 pending entries fit.
    fn backstep_shape(&self, id: NodeId) -> Option<(usize, usize)> {
        let shape = match &self.nodes[id] {
            Node::Empty
            | Node::Anchor(_)
            | Node::Assert { .. }
            | Node::ResetStart
            | Node::ToggleCase(_)
            | Node::Verb(_)
            | Node::Recurse(_)
            | Node::ConditionTest(_) => (0, 0),
            Node::Literal { bytes, .. } => (bytes.len(), 0),
            Node::Dot { .. } => (1, 0),
            Node::Set(set) if set.sequences.is_empty() => (1, 0),
            Node::Group { body } | Node::Capture { body, .. } | Node::Atomic { body } => {
                self.backstep_shape(*body)?
            }
            Node::Sequence(items) => {
                items
                    .iter()
                    .try_fold((0_usize, 0_usize), |(width, pending), &item| {
                        let (next_width, next_pending) = self.backstep_shape(item)?;
                        Some((
                            width.checked_add(next_width)?,
                            pending.checked_add(next_pending)?,
                        ))
                    })?
            }
            Node::Alternation(items) => self.backstep_alternatives(items)?,
            Node::Conditional {
                condition, yes, no, ..
            } => {
                let (width, pending) = self.backstep_alternatives(&[*yes, *no])?;
                if let Condition::Assertion(condition) = condition {
                    let (condition_width, condition_pending) = self.backstep_shape(*condition)?;
                    if condition_width != 0 {
                        return None;
                    }
                    (width, pending.checked_add(condition_pending)?)
                } else {
                    (width, pending)
                }
            }
            Node::Repeat { body, min, max, .. }
                if min == max
                    && i32::try_from(*min).is_ok()
                    && (matches!(&self.nodes[*body], Node::Literal { bytes, .. } if bytes.len() == 1)
                        || matches!(&self.nodes[*body], Node::Dot { .. })
                        || matches!(&self.nodes[*body], Node::Set(set) if set.sequences.is_empty())) =>
            {
                (*min, 0)
            }
            _ => return None,
        };
        (i32::try_from(shape.0).is_ok() && shape.1 <= 1025).then_some(shape)
    }

    fn backstep_alternatives(&self, items: &[NodeId]) -> Option<(usize, usize)> {
        let mut width = None;
        let mut pending = 0;
        for (index, &item) in items.iter().enumerate() {
            let (item_width, item_pending) = self.backstep_shape(item)?;
            if width.is_some_and(|width| width != item_width) {
                return None;
            }
            width = Some(item_width);
            pending = pending.max(item_pending + usize::from(index + 1 != items.len()));
        }
        Some((width?, pending))
    }

    fn reference_id(&self, target: &Reference) -> Option<usize> {
        match target {
            Reference::Number(id) => Some(*id).filter(|&id| id < self.groups.len()),
            Reference::Name(name) => self
                .names
                .iter()
                .find(|(candidate, _)| name_hash(candidate) == name_hash(name))
                .map(|(_, index)| *index),
        }
    }
    fn finish(&mut self) -> ParseResult<()> {
        let has_recursions = self
            .nodes
            .iter()
            .any(|node| matches!(node, Node::Recurse(_)));
        for node in &self.nodes {
            if let Node::Recurse(target) = node
                && self
                    .reference_id(target)
                    .and_then(|id| self.groups.get(id).copied().flatten())
                    .is_none()
            {
                return Err(Box::new(Error::compile_raw(2,0,b"Encountered a forward reference to a recursive sub-expression that does not exist.")));
            }
            if let Node::Conditional {
                condition: Condition::Recursion(Some(Reference::Name(name))),
                ..
            }
            | Node::ConditionTest(Condition::Recursion(Some(Reference::Name(name)))) = node
                && has_recursions
                && self.reference_id(&Reference::Name(name.clone())).is_none()
            {
                return Err(Box::new(Error::compile_raw(2,0,b"Encountered a forward reference to a marked sub-expression that does not exist.")));
            }
        }
        for id in 0..self.nodes.len() {
            if let Node::Assert {
                body,
                behind: Some(_),
                ..
            } = &self.nodes[id]
            {
                let width = self.fixed_width(*body).ok_or_else(|| {
                    Error::compile_raw(
                        2,
                        0,
                        b"Invalid lookbehind assertion encountered in the regular expression.",
                    )
                })?;
                if let Node::Assert { behind, .. } = &mut self.nodes[id] {
                    *behind = Some(width);
                }
            }
        }
        Ok(())
    }
}
