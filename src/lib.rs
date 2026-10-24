//! Byte-oriented Rust port of the repository-local trimmed Boost.Regex engine.
//!
//! Patterns, inputs, templates, and raw errors are bytes. All positions are byte
//! offsets. A compiled expression is immutable and can be shared across threads.
//!
//! ```
//! use boost_regex_rs_trim::{Limits, Regex};
//! let regex = Regex::compile(b"(?<word>[a-z]+)", Limits::UNLIMITED)?;
//! assert_eq!(regex.find(b"123 abc")?, Some(4..7));
//! assert_eq!(regex.replace(b"abc", b"$+{word}!")?, b"abc!");
//! # Ok::<(), boost_regex_rs_trim::Error>(())
//! ```

mod ast;
pub mod character;
mod engine;
pub mod error;
mod format;
mod parser;
mod startmap;

pub use character::{CharacterRules, LocaleError};
pub use error::{Error, LimitKind, Phase};
use std::ops::Range;
use std::sync::Arc;

/// Caller policy. These limits do not replace the reference's computation limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub pattern: usize,
    pub input: usize,
    pub template: usize,
    pub output: usize,
    pub matches: usize,
}

impl Limits {
    /// No caller policy restriction; engine computation limits still apply.
    pub const UNLIMITED: Self = Self {
        pattern: usize::MAX,
        input: usize::MAX,
        template: usize::MAX,
        output: usize::MAX,
        matches: usize::MAX,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

/// One match and its numbered captures. Capture zero is the whole match.
/// `None` denotes a nonparticipating capture; `Some(n..n)` is an empty capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Match {
    pub captures: Vec<Option<Range<usize>>>,
    /// Last group closed by the reference, used by its replacement formatter.
    pub last_closed: Option<usize>,
    // ACCEPT can leave an opened group unmatched with nonempty stored bounds.
    // The source formatter reads those bounds even when public participation is false.
    pub(crate) formatting_captures: Vec<Range<usize>>,
}

/// Opt-in reference-accounting evidence for compatibility tests.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug)]
pub struct Diagnostics {
    pub result: Result<Option<Match>, Error>,
    pub state_count: usize,
    pub max_state_count: usize,
    pub blocks_remaining_after_cleanup: usize,
}

impl Match {
    /// The whole-match range.
    ///
    /// # Panics
    /// Panics if a caller removes capture zero from the public capture vector.
    /// Every match returned by `Regex` contains capture zero.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.captures[0]
            .clone()
            .expect("successful match has capture zero")
    }

    /// A participating numbered capture, if present.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Range<usize>> {
        self.captures.get(index).cloned().flatten()
    }
}

#[derive(Debug)]
struct Compiled {
    engine: engine::Engine,
    names: Vec<(Vec<u8>, usize)>,
    mark_count: usize,
    rules: CharacterRules,
}

/// An immutable compiled byte expression. Clone shares the compiled engine.
#[derive(Clone, Debug)]
pub struct Regex {
    compiled: Arc<Compiled>,
    limits: Limits,
}

impl Regex {
    /// Compile with no caller policy restrictions.
    ///
    /// # Errors
    /// Returns the reference-compatible compilation error for an invalid pattern.
    pub fn new(pattern: &[u8]) -> Result<Self, Error> {
        Self::compile(pattern, Limits::UNLIMITED)
    }

    /// Compile and apply the supplied caller limits to subsequent operations.
    ///
    /// # Errors
    /// Returns a pattern-limit or reference-compatible compilation error.
    pub fn compile(pattern: &[u8], limits: Limits) -> Result<Self, Error> {
        Self::with_character_rules(pattern, limits, CharacterRules::default())
    }

    /// Compile with an explicit snapshot of native character rules.
    ///
    /// # Errors
    /// Returns a pattern-limit or reference-compatible compilation error.
    pub fn with_character_rules(
        pattern: &[u8],
        limits: Limits,
        rules: CharacterRules,
    ) -> Result<Self, Error> {
        check_limit(LimitKind::Pattern, limits.pattern, pattern.len())?;
        let program = parser::parse(pattern, &rules)?;
        let engine = engine::Engine::new(&program, &rules)?;
        Ok(Self {
            compiled: Arc::new(Compiled {
                engine,
                names: program.names,
                mark_count: program.mark_count,
                rules,
            }),
            limits,
        })
    }

    /// Number of explicit capturing groups, excluding capture zero.
    #[must_use]
    pub fn mark_count(&self) -> usize {
        self.compiled.mark_count
    }

    /// The policy limits used by this expression.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Access the native character-rule snapshot.
    #[must_use]
    pub fn character_rules(&self) -> &CharacterRules {
        &self.compiled.rules
    }

    /// Return a participating named capture. Duplicate names follow reference order.
    #[must_use]
    pub fn named_capture(&self, matched: &Match, name: &[u8]) -> Option<Range<usize>> {
        let hash = ast::name_hash(name);
        self.compiled
            .names
            .iter()
            .filter(|(candidate, _)| ast::name_hash(candidate) == hash)
            .find_map(|(_, index)| matched.get(*index))
    }

    /// Find the first whole-match byte span.
    ///
    /// # Errors
    /// Returns an input-limit or reference engine computation error.
    pub fn find(&self, input: &[u8]) -> Result<Option<Range<usize>>, Error> {
        Ok(self.captures(input)?.map(|matched| matched.span()))
    }

    /// Search for the first match and all captures.
    ///
    /// # Errors
    /// Returns an input-limit or reference engine computation error.
    pub fn captures(&self, input: &[u8]) -> Result<Option<Match>, Error> {
        self.check_input(input)?;
        self.compiled
            .engine
            .search(input, &self.compiled.rules, 0, false, false)
    }

    /// Whether a search succeeds.
    ///
    /// # Errors
    /// Returns an input-limit or reference engine computation error.
    pub fn is_match(&self, input: &[u8]) -> Result<bool, Error> {
        Ok(self.captures(input)?.is_some())
    }

    /// Match the complete input and return its captures.
    ///
    /// # Errors
    /// Returns an input-limit or reference engine computation error.
    pub fn full_match(&self, input: &[u8]) -> Result<Option<Match>, Error> {
        self.check_input(input)?;
        self.compiled
            .engine
            .search(input, &self.compiled.rules, 0, true, false)
    }

    /// Run a search or full match and retain the reference-accounting counters.
    /// The operation's success or failure is in `Diagnostics::result`.
    #[cfg(feature = "diagnostics")]
    #[must_use]
    pub fn diagnose(&self, input: &[u8], full: bool) -> Diagnostics {
        if let Err(error) = self.check_input(input) {
            return Diagnostics {
                result: Err(error),
                state_count: 0,
                max_state_count: 0,
                blocks_remaining_after_cleanup: 1024,
            };
        }
        let execution = self
            .compiled
            .engine
            .run(input, &self.compiled.rules, 0, full, false);
        Diagnostics {
            result: execution.result,
            state_count: execution.state_count,
            max_state_count: execution.max_state_count,
            blocks_remaining_after_cleanup: execution.blocks_remaining_after_cleanup,
        }
    }

    /// Collect all matches, including the reference's nonempty retry after an
    /// empty match. Failure returns no partial collection.
    ///
    /// # Errors
    /// Returns an input/match-count limit or reference engine computation error.
    pub fn find_all(&self, input: &[u8]) -> Result<Vec<Match>, Error> {
        self.check_input(input)?;
        let mut matches = Vec::new();
        let mut start = 0;
        let mut not_initial_null = false;
        while let Some(matched) = self.compiled.engine.search(
            input,
            &self.compiled.rules,
            start,
            false,
            not_initial_null,
        )? {
            check_limit(
                LimitKind::Matches,
                self.limits.matches,
                matches.len().saturating_add(1),
            )?;
            let span = matched.span();
            start = span.end;
            not_initial_null = span.start == span.end;
            matches.push(matched);
        }
        Ok(matches)
    }

    /// Globally replace matches using the retained Perl byte formatter.
    /// Failure discards the entire output buffer.
    ///
    /// # Errors
    /// Returns an input/template/output/match-count limit or engine error.
    /// Returns an execution error without a reference code if match-start
    /// resets produce a backwards range that the C++ copying operation cannot
    /// consume with defined behavior.
    pub fn replace(&self, input: &[u8], template: &[u8]) -> Result<Vec<u8>, Error> {
        check_limit(LimitKind::Template, self.limits.template, template.len())?;
        let matches = self.find_all(input)?;
        let mut output = Vec::new();
        let mut previous_end = 0;
        for matched in matches {
            let span = matched.span();
            if span.end < span.start || span.end > input.len() {
                return Err(Error::invalid_replacement_range());
            }
            let prefix = input
                .get(previous_end..span.start)
                .ok_or_else(Error::invalid_replacement_range)?;
            self.append_bytes(&mut output, prefix)?;
            format::append(
                &mut output,
                template,
                &format::FormatContext {
                    input,
                    captures: &matched.captures,
                    formatting_captures: &matched.formatting_captures,
                    names: &self.compiled.names,
                    prefix_start: previous_end,
                    last_closed: matched.last_closed,
                    rules: &self.compiled.rules,
                },
                self.limits.output,
            )?;
            previous_end = span.end;
        }
        self.append_bytes(&mut output, &input[previous_end..])?;
        Ok(output)
    }

    fn check_input(&self, input: &[u8]) -> Result<(), Error> {
        check_limit(LimitKind::Input, self.limits.input, input.len())
    }

    fn append_bytes(&self, output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Error> {
        check_limit(
            LimitKind::Output,
            self.limits.output,
            output.len().saturating_add(bytes.len()),
        )?;
        output.extend_from_slice(bytes);
        Ok(())
    }
}

fn check_limit(kind: LimitKind, maximum: usize, actual: usize) -> Result<(), Error> {
    if actual > maximum {
        Err(Error::limit(kind, maximum, actual))
    } else {
        Ok(())
    }
}
