// Post-parse AST validation ported from the retained Boost.Regex creator.
// Keep the reference's error ordering and fixed-width accounting behavior.

use crate::ast::{Condition, Node, NodeId, Program, Reference, name_hash};
use crate::error::Error;

pub(crate) fn validate(program: &mut Program) -> Result<(), Error> {
    program.validate_inner().map_err(|error| *error)
}

impl Program {
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
    fn validate_inner(&mut self) -> Result<(), Box<Error>> {
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
