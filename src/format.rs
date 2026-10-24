// Ported from boost/regex/v5/regex_format.hpp.
// Copyright John Maddock and Eric Niebler. Distributed under BSL-1.0.

use crate::ast::name_hash;
use crate::character::CharacterRules;
use crate::error::{Error, LimitKind};
use std::ops::Range;

/// The match and original input ranges used by the retained default formatter.
pub(crate) struct FormatContext<'a> {
    pub input: &'a [u8],
    pub captures: &'a [Option<Range<usize>>],
    /// Stored bounds remain observable to numbered expansions after ACCEPT,
    /// even when the corresponding capture did not participate publicly.
    pub formatting_captures: &'a [Range<usize>],
    pub names: &'a [(Vec<u8>, usize)],
    pub prefix_start: usize,
    pub last_closed: Option<usize>,
    pub rules: &'a CharacterRules,
}

/// Append a single match's expansion. The public replacement operation owns the
/// buffer, and discards it if this function reports an output-limit failure.
pub(crate) fn append(
    output: &mut Vec<u8>,
    template: &[u8],
    context: &FormatContext<'_>,
    output_limit: usize,
) -> Result<(), Error> {
    Formatter {
        output,
        template,
        context,
        position: 0,
        state: State::Copy,
        restore_state: State::Copy,
        output_limit,
    }
    .run()
}

#[derive(Clone, Copy)]
enum State {
    Copy,
    NextLower,
    NextUpper,
    Lower,
    Upper,
}

struct Formatter<'a, 'b> {
    output: &'a mut Vec<u8>,
    template: &'a [u8],
    context: &'a FormatContext<'b>,
    position: usize,
    state: State,
    restore_state: State,
    output_limit: usize,
}

impl Formatter<'_, '_> {
    fn run(&mut self) -> Result<(), Error> {
        while let Some(&byte) = self.template.get(self.position) {
            match byte {
                b'\\' => self.escape()?,
                b'$' => self.perl()?,
                _ => {
                    self.put(byte)?;
                    self.position += 1;
                }
            }
        }
        Ok(())
    }

    fn put(&mut self, mut byte: u8) -> Result<(), Error> {
        if self.output.len() >= self.output_limit {
            return Err(Error::limit(
                LimitKind::Output,
                self.output_limit,
                self.output.len().saturating_add(1),
            ));
        }
        match self.state {
            State::Copy => {}
            State::Lower => byte = self.context.rules.lower(byte),
            State::Upper => byte = self.context.rules.upper(byte),
            State::NextLower => {
                byte = self.context.rules.lower(byte);
                self.state = self.restore_state;
            }
            State::NextUpper => {
                byte = self.context.rules.upper(byte);
                self.state = self.restore_state;
            }
        }
        self.output.push(byte);
        Ok(())
    }

    fn range(&mut self, range: Range<usize>) -> Result<(), Error> {
        let input = self.context.input;
        let bytes = input
            .get(range)
            .ok_or_else(Error::invalid_replacement_range)?;
        for &byte in bytes {
            self.put(byte)?;
        }
        Ok(())
    }

    fn capture(&mut self, index: usize) -> Result<(), Error> {
        if let Some(range) = self.context.formatting_captures.get(index) {
            self.range(range.clone())?;
        }
        Ok(())
    }

    fn prefix(&mut self) -> Result<(), Error> {
        let start = self.context.captures[0].as_ref().map_or(0, |r| r.start);
        self.range(self.context.prefix_start..start)
    }

    fn suffix(&mut self) -> Result<(), Error> {
        let end = self.context.captures[0].as_ref().map_or(0, |r| r.end);
        self.range(end..self.context.input.len())
    }

    fn named(&mut self, name: &[u8]) -> Result<(), Error> {
        let hash = name_hash(name);
        let group = self
            .context
            .names
            .iter()
            .filter(|(candidate, _)| name_hash(candidate) == hash)
            .map(|(_, index)| *index)
            .find(|&index| matches!(self.context.captures.get(index), Some(Some(_))));
        if let Some(group) = group {
            self.capture(group)?;
        }
        Ok(())
    }

    fn last_numbered(&mut self) -> Result<(), Error> {
        self.capture(self.context.captures.len().saturating_sub(1).max(1))
    }

    fn number(&mut self, end: usize, radix: u32) -> i32 {
        // The reference's formatter helper explicitly casts intmax_t to int.
        let bytes = self
            .context
            .rules
            .toi(self.template, &mut self.position, end, radix)
            .to_le_bytes();
        i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn perl(&mut self) -> Result<(), Error> {
        self.position += 1;
        let Some(&byte) = self.template.get(self.position) else {
            return self.put(b'$');
        };
        let save_position = self.position;
        match byte {
            b'&' => {
                self.position += 1;
                self.capture(0)
            }
            b'`' => {
                self.position += 1;
                self.prefix()
            }
            b'\'' => {
                self.position += 1;
                self.suffix()
            }
            b'$' => {
                self.position += 1;
                self.put(b'$')
            }
            b'+' => {
                self.position += 1;
                if self.template.get(self.position) == Some(&b'{') {
                    let opening = self.position;
                    self.position += 1;
                    let start = self.position;
                    while self.position < self.template.len()
                        && self.template[self.position] != b'}'
                    {
                        self.position += 1;
                    }
                    if self.position < self.template.len() {
                        self.named(&self.template[start..self.position])?;
                        self.position += 1;
                        return Ok(());
                    }
                    self.position = opening;
                }
                self.last_numbered()
            }
            _ => {
                let have_brace = byte == b'{';
                if have_brace {
                    self.position += 1;
                }
                let value = self.number(self.template.len(), 10);
                if value < 0 || (have_brace && self.template.get(self.position) != Some(&b'}')) {
                    if !self.verb(have_brace)? {
                        self.position = save_position;
                        self.put(b'$')?;
                    }
                    return Ok(());
                }
                self.capture(value.unsigned_abs() as usize)?;
                if have_brace {
                    self.position += 1;
                }
                Ok(())
            }
        }
    }

    fn verb(&mut self, have_brace: bool) -> Result<bool, Error> {
        if self.position == self.template.len() {
            return Ok(false);
        }
        if have_brace && self.template[self.position] == b'^' {
            self.position += 1;
        }
        for (name, operation) in [
            (&b"MATCH"[..], 0),
            (&b"PREMATCH"[..], 1),
            (&b"POSTMATCH"[..], 2),
            (&b"LAST_PAREN_MATCH"[..], 3),
            (&b"LAST_SUBMATCH_RESULT"[..], 4),
            (&b"^N"[..], 4),
        ] {
            if !self.template[self.position..].starts_with(name) {
                continue;
            }
            self.position += name.len();
            if have_brace {
                if self.template.get(self.position) != Some(&b'}') {
                    self.position -= name.len();
                    return Ok(false);
                }
                self.position += 1;
            }
            match operation {
                0 => self.capture(0)?,
                1 => self.prefix()?,
                2 => self.suffix()?,
                3 => self.last_numbered()?,
                _ => {
                    if let Some(index) = self.context.last_closed {
                        self.capture(index)?;
                    }
                }
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn escape(&mut self) -> Result<(), Error> {
        let escape_start = self.position;
        self.position += 1;
        let Some(&byte) = self.template.get(self.position) else {
            return self.put(b'\\');
        };
        let literal = match byte {
            b'a' => Some(7),
            b'f' => Some(12),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            b'v' => Some(11),
            b'e' => Some(27),
            _ => None,
        };
        if let Some(literal) = literal {
            self.position += 1;
            return self.put(literal);
        }
        match byte {
            b'x' => {
                self.position += 1;
                if self.position == self.template.len() {
                    return self.put(b'x');
                }
                if self.template[self.position] == b'{' {
                    self.position += 1;
                    let value = self.number(self.template.len(), 16);
                    if value < 0 {
                        self.put(b'x')?;
                        return self.put(b'{');
                    }
                    if self.template.get(self.position) != Some(&b'}') {
                        self.position = escape_start + 2;
                        return self.put(b'x');
                    }
                    self.position += 1;
                    return self.put(value.to_le_bytes()[0]);
                }
                let value = self.number((self.position + 2).min(self.template.len()), 16);
                if value < 0 {
                    // The reference rewinds one byte even when a signed
                    // negative number consumed two bytes before the cast.
                    self.position -= 1;
                    let literal = self.template[self.position];
                    self.position += 1;
                    self.put(literal)
                } else {
                    self.put(value.to_le_bytes()[0])
                }
            }
            b'c' => {
                self.position += 1;
                let Some(&literal) = self.template.get(self.position) else {
                    return self.put(b'c');
                };
                self.position += 1;
                // char signedness differs between supported target ABIs.
                self.put((std::ffi::c_char::from_ne_bytes([literal]) % 32).to_ne_bytes()[0])
            }
            b'l' | b'u' => {
                self.restore_state = self.state;
                self.state = if byte == b'l' {
                    State::NextLower
                } else {
                    State::NextUpper
                };
                self.position += 1;
                Ok(())
            }
            b'L' | b'U' | b'E' => {
                self.state = match byte {
                    b'L' => State::Lower,
                    b'U' => State::Upper,
                    _ => State::Copy,
                };
                self.position += 1;
                Ok(())
            }
            _ => {
                let value = self.number(self.position + 1, 10);
                if value > 0 {
                    return self.capture(value.unsigned_abs() as usize);
                }
                if value == 0 {
                    self.position -= 1;
                    let value = self.number((self.position + 4).min(self.template.len()), 8);
                    return self.put(value.to_le_bytes()[0]);
                }
                let literal = self.template[self.position];
                self.position += 1;
                self.put(literal)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(template: &[u8]) -> Vec<u8> {
        let rules = CharacterRules::default();
        let context = FormatContext {
            input: b"preAbpost",
            captures: &[Some(3..5), Some(3..4), Some(4..5), None],
            formatting_captures: &[3..5, 3..4, 4..5, 9..9],
            names: &[(b"letter".to_vec(), 1)],
            prefix_start: 0,
            last_closed: Some(2),
            rules: &rules,
        };
        let mut output = Vec::new();
        append(&mut output, template, &context, usize::MAX).unwrap();
        output
    }

    #[test]
    fn byte_expansions_and_unmatched_captures() {
        assert_eq!(
            expand(b"\0\xff$0:$1:$2:$3:$4:$+{letter}"),
            b"\0\xffAb:A:b:::A"
        );
        assert_eq!(expand(b"$`|$&|$'|$$|$+"), b"pre|Ab|post|$|");
        assert_eq!(
            expand(b"$MATCH|${PREMATCH}|${^POSTMATCH}|$LAST_PAREN_MATCH|$LAST_SUBMATCH_RESULT|$^N"),
            b"Ab|pre|post||b|b"
        );
        assert_eq!(expand(b"${^N}|${^^N}"), b"${^N}|b");
    }

    #[test]
    fn incomplete_forms_retain_reference_literals() {
        for (template, expected) in [
            (&b"$"[..], &b"$"[..]),
            (&b"${1"[..], &b"${1"[..]),
            (&b"${missing}"[..], &b"${missing}"[..]),
            (&b"$+{missing"[..], &b"{missing"[..]),
            (&b"\\"[..], &b"\\"[..]),
            (&b"\\x"[..], &b"x"[..]),
            (&b"\\x{41"[..], &b"x{41"[..]),
            (&b"\\x{zz}"[..], &b"x{zz}"[..]),
        ] {
            assert_eq!(expand(template), expected);
        }
        assert_eq!(expand(b"$+{missing}|\\q|\\01234"), b"|q|S4");
    }

    #[test]
    fn stateful_case_conversion_restores_previous_mode() {
        assert_eq!(expand(b"\\U$0\\E-\\L$0\\E-\\l$0-\\u$2"), b"AB-ab-ab-B");
        assert_eq!(expand(b"\\U\\lAB\\E-\\L\\uAB\\E"), b"aB-Ab");
        assert_eq!(expand(b"\\l\\uABC"), b"Abc");
        assert_eq!(
            expand(b"\\a\\f\\n\\r\\t\\v\\e\\x41\\x{ff}\\cA"),
            b"\x07\x0c\n\r\t\x0b\x1bA\xff\x01"
        );
    }

    #[test]
    fn expansions_check_output_limit_at_each_emitted_byte() {
        let rules = CharacterRules::default();
        let whole = 0..2;
        let context = FormatContext {
            input: b"ab",
            captures: &[Some(0..2)],
            formatting_captures: std::slice::from_ref(&whole),
            names: &[],
            prefix_start: 0,
            last_closed: None,
            rules: &rules,
        };
        let mut output = Vec::new();
        let error = append(&mut output, b"\\U$0$0", &context, 3).unwrap_err();
        assert_eq!(error.limit_kind, Some(LimitKind::Output));
        assert_eq!(error.limit, Some(3));
        assert_eq!(error.actual, Some(4));
    }
}
