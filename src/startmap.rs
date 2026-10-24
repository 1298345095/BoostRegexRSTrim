// The reference creator's start-map walk is part of its computation contract.
// Ported from basic_regex_creator.hpp; distributed under BSL-1.0.

use crate::Error;
use crate::ast::{Anchor, Condition, Reference, Verb, name_hash};
use crate::character::CharacterRules;
use crate::engine::{Atom, First, Instruction, Kind, atom_match, set_match};

#[derive(Clone)]
struct Map {
    first: First,
    initialized: bool,
}

impl Map {
    fn empty() -> Self {
        Self {
            first: First::empty(),
            initialized: false,
        }
    }
    fn all(nullable: bool) -> Self {
        Self {
            first: First {
                bytes: [true; 256],
                nullable,
            },
            initialized: true,
        }
    }
    fn union(&mut self, other: &Self) {
        self.first.union(&other.first);
        self.initialized |= other.initialized;
    }
}

/// The compiler emits backwards. Reconstruct the source's lexical state order,
/// stopping branch traversal at the explicit jumps and assertion end markers.
pub(crate) fn lexical_order(code: &[Instruction], entry: usize) -> Vec<usize> {
    let mut order = Vec::with_capacity(code.len());
    let mut seen = vec![false; code.len()];
    let mut pending = vec![entry];
    while let Some(pc) = pending.pop() {
        if seen[pc] {
            continue;
        }
        seen[pc] = true;
        order.push(pc);
        let instruction = &code[pc];
        match &instruction.kind {
            Kind::Match | Kind::Jump | Kind::ScopeEnd => {}
            Kind::Alt(other) => {
                pending.push(*other);
                pending.push(instruction.next);
            }
            Kind::Repeat { body, .. } | Kind::Assert { body, .. } | Kind::Atomic(body) => {
                pending.push(instruction.next);
                pending.push(*body);
            }
            Kind::Conditional { yes, no, .. } => {
                pending.push(instruction.next);
                pending.push(*no);
                pending.push(*yes);
            }
            _ => pending.push(instruction.next),
        }
    }
    // Assertion conditions are stored out of line by the Rust compiler.
    // They remain reachable creator states even when not taken during matching.
    for (pc, instruction) in code.iter().enumerate().rev() {
        if !seen[pc] && !matches!(instruction.kind, Kind::Match) {
            order.push(pc);
        }
    }
    order
}

pub(crate) fn build(
    code: &[Instruction],
    groups: &[usize],
    names: &[(Vec<u8>, usize)],
    rules: &CharacterRules,
) -> Result<Vec<First>, Error> {
    let order = lexical_order(code, groups[0]);
    let mut repeats = vec![None; code.len()];
    let mut count = 0;
    for &pc in &order {
        if matches!(
            code[pc].kind,
            Kind::Repeat { .. } | Kind::SingleRepeat { .. }
        ) {
            repeats[pc] = Some(count);
            count += 1;
        }
    }
    let mut builder = Builder {
        code,
        groups,
        names,
        rules,
        order: &order,
        repeats,
        cache: vec![None; code.len()],
    };
    let mut first = vec![First::any(); code.len()];
    for &pc in order.iter().rev() {
        let instruction = &code[pc];
        let (take_start, skip_start) = match &instruction.kind {
            Kind::Alt(other) => (Some(instruction.next), *other),
            Kind::Repeat { body, .. } => (Some(*body), instruction.next),
            Kind::SingleRepeat { .. } => (None, instruction.next),
            Kind::Conditional { yes, no, .. } => (Some(*yes), *no),
            _ => continue,
        };
        builder.cache[pc] = Some(Map::empty());
        let take = match &instruction.kind {
            Kind::SingleRepeat { atom, .. } => builder.atom(atom),
            Kind::Conditional {
                condition: Condition::Assertion(_),
                ..
            } => Map::all(true),
            _ => builder.root(take_start.expect("choice body"), Some(pc), Map::empty())?,
        };
        builder.cache[pc] = Some(take.clone());
        let skip = builder.root(skip_start, Some(pc), take.clone())?;
        if let Some(take_start) = take_start {
            first[take_start] = take.first.clone();
        }
        first[skip_start] = skip.first;
    }
    first[groups[0]] = builder.root(groups[0], None, Map::empty())?.first;
    Ok(first)
}

// The reference writes into each choice's map as it walks. A nullable walk
// can revisit that same choice before its map has finished: the partial
// mask_init bit and nullable bits are therefore observable matching behavior.
struct Output {
    map: Map,
    cache_slot: Option<usize>,
    other_branch: Map,
}

struct Builder<'a> {
    code: &'a [Instruction],
    groups: &'a [usize],
    names: &'a [(Vec<u8>, usize)],
    rules: &'a CharacterRules,
    order: &'a [usize],
    repeats: Vec<Option<usize>>,
    cache: Vec<Option<Map>>,
}

impl Builder<'_> {
    fn root(
        &mut self,
        pc: usize,
        cache_slot: Option<usize>,
        other_branch: Map,
    ) -> Result<Map, Error> {
        let mut output = Output {
            map: Map::empty(),
            cache_slot,
            other_branch,
        };
        self.walk(pc, 0, vec![0; self.groups.len()], &mut 0, &mut output, true)?;
        Ok(output.map)
    }

    fn publish(&mut self, output: &Output) {
        if let Some(slot) = output.cache_slot {
            let mut combined = output.map.clone();
            combined.union(&output.other_branch);
            self.cache[slot] = Some(combined);
        }
    }

    fn add(&mut self, output: &mut Output, map: &Map, with_bytes: bool) {
        if with_bytes {
            output.map.union(map);
        } else {
            output.map.first.nullable |= map.first.nullable;
        }
        self.publish(output);
    }

    fn resolve(&self, target: &Reference) -> Option<usize> {
        match target {
            Reference::Number(index) => (*index < self.groups.len()).then_some(*index),
            Reference::Name(name) => self
                .names
                .iter()
                .find(|(candidate, _)| name_hash(candidate) == name_hash(name))
                .map(|(_, index)| *index),
        }
    }

    fn atom(&self, atom: &Atom) -> Map {
        if matches!(atom, Atom::Dot(_)) || matches!(atom,Atom::Set(set) if set.long) {
            return Map::all(false);
        }
        let mut result = Map::empty();
        result.initialized = true;
        for byte in 0_u8..=255 {
            result.first.bytes[usize::from(byte)] =
                atom_match(atom, &[byte], 0, self.rules).is_some();
        }
        result
    }

    // This intentionally follows the retained dispatch, including conservative
    // maps for assertions, verbs, and byte \X. A more precise analysis would
    // change which states the matcher visits and therefore its budget behavior.
    #[allow(clippy::too_many_lines)]
    fn walk(
        &mut self,
        mut pc: usize,
        mut depth: usize,
        mut checks: Vec<u8>,
        bad: &mut u64,
        output: &mut Output,
        with_bytes: bool,
    ) -> Result<(), Error> {
        if depth > 100 {
            return Err(Error::compile_raw(
                18,
                0,
                b"Expression complexity exceeded.",
            ));
        }
        let mut recursion: Option<(usize, usize)> = None;
        let mut after_jump = false;
        let code = self.code;
        loop {
            let instruction = &code[pc];
            match &instruction.kind {
                Kind::Literal(bytes, icase) => {
                    if let Some(&literal) = bytes.first() {
                        if with_bytes {
                            output.map.initialized = true;
                            for byte in 0_u8..=255 {
                                let actual = if *icase { self.rules.lower(byte) } else { byte };
                                let expected = if *icase {
                                    self.rules.lower(literal)
                                } else {
                                    literal
                                };
                                output.map.first.bytes[usize::from(byte)] |= actual == expected;
                            }
                            self.publish(output);
                        }
                        return Ok(());
                    }
                }
                Kind::Dot(_) => {
                    self.add(output, &Map::all(false), with_bytes);
                    return Ok(());
                }
                Kind::Set(set) => {
                    if with_bytes {
                        if set.long {
                            self.add(output, &Map::all(false), true);
                        } else {
                            output.map.initialized = true;
                            for byte in 0_u8..=255 {
                                output.map.first.bytes[usize::from(byte)] |=
                                    set_match(set, &[byte], 0, self.rules).is_some();
                            }
                            self.publish(output);
                        }
                    }
                    return Ok(());
                }
                Kind::Match
                | Kind::ScopeEnd
                | Kind::Assert { .. }
                | Kind::Verb(Verb::Accept)
                | Kind::Backref(..) => {
                    self.add(output, &Map::all(true), with_bytes);
                    return Ok(());
                }
                Kind::Anchor(Anchor::BufferEnd) => {
                    output.map.first.nullable = true;
                    self.publish(output);
                    return Ok(());
                }
                Kind::Anchor(Anchor::SoftEnd) => {
                    if with_bytes {
                        output.map.initialized = true;
                        output.map.first.bytes[usize::from(b'\n')] = true;
                        output.map.first.bytes[usize::from(b'\r')] = true;
                    }
                    output.map.first.nullable = true;
                    self.publish(output);
                    return Ok(());
                }
                Kind::Anchor(Anchor::LineEnd) => {
                    if with_bytes {
                        output.map.initialized = true;
                        for byte in [b'\n', b'\r', 12, 0x85] {
                            output.map.first.bytes[usize::from(byte)] = true;
                        }
                        self.publish(output);
                    }
                    self.walk(instruction.next, depth + 1, checks, bad, output, false)?;
                    return Ok(());
                }
                Kind::Anchor(Anchor::WordStart | Anchor::WordEnd) => {
                    self.walk(instruction.next, depth + 1, checks, bad, output, with_bytes)?;
                    if with_bytes {
                        let require_word =
                            matches!(instruction.kind, Kind::Anchor(Anchor::WordStart));
                        for byte in 0_u8..=255 {
                            if self.rules.is_class(byte, b"w").unwrap_or(false) != require_word {
                                output.map.first.bytes[usize::from(byte)] = false;
                            }
                        }
                        output.map.initialized = true;
                        self.publish(output);
                    }
                    return Ok(());
                }
                Kind::Jump => {
                    pc = instruction.next;
                    after_jump = true;
                    continue;
                }
                Kind::Atomic(body) => {
                    pc = *body;
                    after_jump = false;
                    continue;
                }
                Kind::Recurse(target) => {
                    let Some(index) = self.resolve(target) else {
                        return Err(Error::compile_raw(2, 0, b"Encountered a forward reference to a recursive sub-expression that does not exist."));
                    };
                    if checks[index] & 1 != 0 {
                        return Err(Error::compile_raw(
                            2,
                            0,
                            b"Encountered an infinite recursion.",
                        ));
                    }
                    checks[index] |= 1;
                    if recursion.is_none() {
                        recursion = Some((index, instruction.next));
                        pc = self.groups[index];
                        after_jump = false;
                        continue;
                    }
                    self.add(output, &Map::all(true), with_bytes);
                    return Ok(());
                }
                Kind::End(index) if *index != 0 => {
                    if recursion.is_some_and(|(id, _)| id == *index) {
                        pc = recursion.take().expect("matching recursive return").1;
                        after_jump = false;
                        continue;
                    }
                    if checks[*index] & 2 == 0
                        && let Some(call) = self.order.iter().find(|&&call| matches!(&code[call].kind, Kind::Recurse(target) if self.resolve(target) == Some(*index)))
                    {
                        checks[*index] |= 2;
                        depth += 1;
                        self.walk(code[*call].next, depth, checks.clone(), bad, output, with_bytes)?;
                    }
                }
                Kind::Alt(_)
                | Kind::Repeat { .. }
                | Kind::SingleRepeat { .. }
                | Kind::Conditional { .. } => {
                    if let Some(cached) = self.cache[pc]
                        .as_ref()
                        .filter(|map| map.initialized)
                        .cloned()
                    {
                        self.add(output, &cached, with_bytes);
                        return Ok(());
                    }
                    if let Some(id) = self.repeats[pc] {
                        if id >= 64 || *bad & (1_u64 << id) != 0 {
                            self.add(output, &Map::all(true), with_bytes);
                            return Ok(());
                        }
                        *bad |= 1_u64 << id;
                    }
                    match &instruction.kind {
                        Kind::Alt(other) => {
                            depth += 1;
                            self.walk(
                                instruction.next,
                                depth,
                                checks.clone(),
                                bad,
                                output,
                                with_bytes,
                            )?;
                            depth += 1;
                            self.walk(*other, depth, checks, bad, output, with_bytes)?;
                        }
                        Kind::Repeat { body, min, .. } => {
                            depth += 1;
                            self.walk(*body, depth, checks.clone(), bad, output, with_bytes)?;
                            if *min == 0 || after_jump {
                                depth += 1;
                                self.walk(
                                    instruction.next,
                                    depth,
                                    checks,
                                    bad,
                                    output,
                                    with_bytes,
                                )?;
                            }
                        }
                        Kind::SingleRepeat { atom, min, .. } => {
                            let first = self.atom(atom);
                            self.add(output, &first, with_bytes);
                            if *min == 0 || after_jump {
                                depth += 2;
                                self.walk(
                                    instruction.next,
                                    depth,
                                    checks,
                                    bad,
                                    output,
                                    with_bytes,
                                )?;
                            }
                        }
                        Kind::Conditional { condition, yes, no } => {
                            depth += 1;
                            if matches!(condition, Condition::Assertion(_)) {
                                self.add(output, &Map::all(true), with_bytes);
                            } else {
                                self.walk(*yes, depth, checks.clone(), bad, output, with_bytes)?;
                            }
                            depth += 1;
                            self.walk(*no, depth, checks, bad, output, with_bytes)?;
                        }
                        _ => unreachable!("choice dispatch"),
                    }
                    return Ok(());
                }
                Kind::Verb(Verb::Commit | Verb::Prune | Verb::Skip) if with_bytes => {
                    output.map.first.bytes.fill(true);
                    output.map.initialized = true;
                    self.publish(output);
                }
                _ => {}
            }
            pc = instruction.next;
            after_jump = false;
        }
    }
}
