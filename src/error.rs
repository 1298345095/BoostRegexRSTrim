//! Structured errors keep reference bytes separate from their display text.

use std::fmt;

/// The operation that produced an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Compile,
    Execute,
    CallerLimit,
    CharacterRules,
}

/// A caller-selected policy limit, distinct from engine computation limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Pattern,
    Input,
    Template,
    Output,
    Matches,
}

/// A compilation, execution, locale, or caller-policy error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub phase: Phase,
    /// Reference error number, when the reference reports a regex error.
    pub code: Option<u32>,
    /// Byte position reported by the reference.
    pub position: Option<usize>,
    pub limit_kind: Option<LimitKind>,
    pub limit: Option<usize>,
    pub actual: Option<usize>,
    raw_message: Vec<u8>,
}

impl Error {
    /// Returns the unmodified message, including NUL and non-UTF-8 bytes.
    #[must_use]
    pub fn raw_message(&self) -> &[u8] {
        &self.raw_message
    }

    /// Returns readable text without changing the raw message.
    #[must_use]
    pub fn display_text(&self) -> String {
        escape_bytes(&self.raw_message)
    }

    pub(crate) fn compile(
        code: u32,
        position: usize,
        pattern: &[u8],
        message: Option<&[u8]>,
        context_position: Option<usize>,
    ) -> Self {
        let mut raw = message.unwrap_or_else(|| default_message(code)).to_vec();
        if code != 17 {
            let start = match context_position {
                Some(start) if start != position => start,
                _ => position.saturating_sub(10),
            };
            let end = position.saturating_add(10).min(pattern.len());
            if start != 0 || end != pattern.len() {
                raw.extend_from_slice(
                    b"  The error occurred while parsing the regular expression fragment: '",
                );
            } else {
                raw.extend_from_slice(
                    b"  The error occurred while parsing the regular expression: '",
                );
            }
            if start != end {
                raw.extend_from_slice(&pattern[start..position]);
                raw.extend_from_slice(b">>>HERE>>>");
                raw.extend_from_slice(&pattern[position..end]);
            }
            raw.extend_from_slice(b"'.");
        }
        Self::compile_raw(code, position, &raw)
    }

    pub(crate) fn compile_raw(code: u32, position: usize, message: &[u8]) -> Self {
        Self::reference(Phase::Compile, code, position, message)
    }

    pub(crate) fn execution(code: u32) -> Self {
        Self::reference(Phase::Execute, code, 0, default_message(code))
    }

    pub(crate) fn invalid_replacement_range() -> Self {
        Self {
            phase: Phase::Execute,
            code: None,
            position: None,
            limit_kind: None,
            limit: None,
            actual: None,
            raw_message: b"Replacement requires forward, non-overlapping match ranges.".to_vec(),
        }
    }

    pub(crate) fn reference(phase: Phase, code: u32, position: usize, message: &[u8]) -> Self {
        Self {
            phase,
            code: Some(code),
            position: Some(position),
            limit_kind: None,
            limit: None,
            actual: None,
            raw_message: message.to_vec(),
        }
    }

    pub(crate) fn limit(kind: LimitKind, limit: usize, actual: usize) -> Self {
        Self {
            phase: Phase::CallerLimit,
            code: None,
            position: None,
            limit_kind: Some(kind),
            limit: Some(limit),
            actual: Some(actual),
            raw_message: format!("{kind:?} limit exceeded: limit {limit}, actual {actual}.")
                .into_bytes(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_text())
    }
}

impl std::error::Error for Error {}

/// Escapes invalid UTF-8 and control bytes while preserving valid readable text.
#[must_use]
// The expects below cover validated UTF-8 and infallible String writes; no input can trigger them.
#[allow(clippy::missing_panics_doc)]
pub fn escape_bytes(mut bytes: &[u8]) -> String {
    use fmt::Write;
    let mut result = String::new();
    while !bytes.is_empty() {
        let (valid, invalid) = match std::str::from_utf8(bytes) {
            Ok(_) => (bytes.len(), 0),
            Err(error) => (
                error.valid_up_to(),
                error
                    .error_len()
                    .unwrap_or(bytes.len() - error.valid_up_to()),
            ),
        };
        // `valid` is exactly the prefix validated by from_utf8.
        for ch in std::str::from_utf8(&bytes[..valid])
            .expect("validated UTF-8 prefix")
            .chars()
        {
            match ch {
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                c if c.is_control() => {
                    for byte in c.to_string().as_bytes() {
                        write!(result, "\\x{byte:02X}").expect("String writes cannot fail");
                    }
                }
                c => result.push(c),
            }
        }
        for byte in &bytes[valid..valid + invalid] {
            write!(result, "\\x{byte:02X}").expect("String writes cannot fail");
        }
        bytes = &bytes[valid + invalid..];
    }
    result
}

pub(crate) fn default_message(code: u32) -> &'static [u8] {
    match code {
        0 => b"Success",
        1 => b"No match",
        2 => b"Invalid regular expression.",
        3 => b"Invalid collation character.",
        4 => b"Invalid character class name, collating name, or character range.",
        5 => b"Invalid or unterminated escape sequence.",
        6 => b"Invalid back reference: specified capturing group does not exist.",
        7 => b"Unmatched [ or [^ in character class declaration.",
        8 => b"Unmatched marking parenthesis ( or \\(.",
        9 => b"Unmatched quantified repeat operator { or \\{.",
        10 => b"Invalid content of repeat range.",
        11 => b"Invalid range end in character class",
        12 => b"Out of memory.",
        13 => b"Invalid preceding regular expression prior to repetition operator.",
        14 => b"Premature end of regular expression",
        15 => b"Regular expression is too large.",
        16 => b"Unmatched ) or \\)",
        17 => b"Empty regular expression.",
        18 => b"The complexity of matching the regular expression exceeded predefined bounds.  Try refactoring the regular expression to make each choice made by the state machine unambiguous.  This exception is thrown to prevent \"eternal\" matches that take an indefinite period time to locate.",
        19 => b"Ran out of stack space trying to match the regular expression.",
        20 => b"Invalid or unterminated Perl (?...) sequence.",
        _ => b"Unknown error.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_and_display_representations_are_independent() {
        let error = Error::compile_raw(2, 1, b"bad\0\xff\xc3\xa9\\\n");
        assert_eq!(error.raw_message(), b"bad\0\xff\xc3\xa9\\\n");
        assert_eq!(error.display_text(), "bad\\x00\\xFFé\\\\\\n");
        assert_eq!(error.raw_message(), b"bad\0\xff\xc3\xa9\\\n");
    }

    #[test]
    fn context_preserves_nul_and_invalid_bytes() {
        let error = Error::compile(5, 2, b"\0\xff\\", None, None);
        let fragment = b"\0\xff>>>HERE>>>\\";
        assert!(
            error
                .raw_message()
                .windows(fragment.len())
                .any(|w| w == fragment)
        );
        assert_eq!(error.position, Some(2));
    }
}
