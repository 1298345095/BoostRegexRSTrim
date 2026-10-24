#![cfg(feature = "diagnostics")]

//! Counts and boundaries recorded from the retained C++ `--accounting` driver.
//! These assertions cover logical computation budgets, not execution speed.

use boost_regex_rs_trim::{Diagnostics, Phase, Regex};

fn assert_counts(observed: &Diagnostics, states: usize, maximum: usize) {
    assert_eq!(
        (
            observed.state_count,
            observed.max_state_count,
            observed.blocks_remaining_after_cleanup,
        ),
        (states, maximum, 1024),
    );
}

fn assert_execution_error(observed: &Diagnostics, code: u32) {
    let error = observed.result.as_ref().unwrap_err();
    assert_eq!(error.phase, Phase::Execute);
    assert_eq!(error.code, Some(code));
    assert_eq!(error.position, Some(0));
}

#[test]
fn alternation_dispatches_before_the_first_branch_start() {
    let regex = Regex::new(b"a|b").unwrap();
    let empty = regex.diagnose(b"", true);
    assert!(empty.result.as_ref().unwrap().is_none());
    assert_counts(&empty, 1, 100_009);

    for full in [false, true] {
        let second = regex.diagnose(b"b", full);
        assert_eq!(
            second.result.as_ref().unwrap().as_ref().unwrap().span(),
            0..1
        );
        assert_counts(&second, 3, 100_009);
    }
}

#[test]
fn full_match_enters_the_machine_when_the_search_map_rejects() {
    let regex = Regex::new(b"a").unwrap();
    for input in [b"".as_slice(), b"b"] {
        let full = regex.diagnose(input, true);
        assert!(full.result.as_ref().unwrap().is_none());
        assert_counts(&full, 2, 100_001);
        let search = regex.diagnose(input, false);
        assert!(search.result.as_ref().unwrap().is_none());
        assert_counts(&search, 0, 100_001);
    }
}

#[test]
fn anchor_restart_modes_attempt_the_initial_position() {
    for (pattern, states, maximum) in [
        (b"^a".as_slice(), 3, 100_004),
        (b"\\Aa", 3, 100_009),
        (b"\\<a", 2, 100_009),
        (b"\\G.", 3, 100_009),
    ] {
        let observed = Regex::new(pattern).unwrap().diagnose(b"", false);
        assert!(observed.result.as_ref().unwrap().is_none());
        assert_counts(&observed, states, maximum);
    }
}

#[test]
fn digraph_sets_keep_the_generic_repeat_path() {
    let regex = Regex::new(b"[[.ae.]]*").unwrap();
    let partial = regex.diagnose(b"a", false);
    assert_eq!(
        partial.result.as_ref().unwrap().as_ref().unwrap().span(),
        0..0
    );
    assert_counts(&partial, 4, 100_081);
    let repeated = regex.diagnose(b"aeae", true);
    assert_eq!(
        repeated.result.as_ref().unwrap().as_ref().unwrap().span(),
        0..4
    );
    assert_counts(&repeated, 9, 100_324);
}

#[test]
fn conditional_direct_handlers_and_explicit_else_have_source_counts() {
    // An explicit empty else still adds the source's successful-yes Jump.
    for (pattern, states, maximum) in [
        (b"(a)?(?(1)b)".as_slice(), 11, 100_242),
        (b"(a)?(?(1)b|)", 12, 100_288),
        (b"(a)?(?(1)b|c)", 12, 100_338),
    ] {
        let observed = Regex::new(pattern).unwrap().diagnose(b"ab", true);
        assert!(observed.result.as_ref().unwrap().is_some());
        assert_counts(&observed, states, maximum);
    }
    let rejected = Regex::new(b"(a)?(?(1)b|c)").unwrap().diagnose(b"b", true);
    assert!(rejected.result.as_ref().unwrap().is_none());
    assert_counts(&rejected, 4, 100_169);

    for (pattern, states, maximum) in [
        (b"(?(?=a)a)".as_slice(), 7, 100_081),
        (b"(?(?=a)a|)", 8, 100_100),
        (b"(?(?=a)a|b)", 8, 100_121),
    ] {
        let observed = Regex::new(pattern).unwrap().diagnose(b"a", true);
        assert!(observed.result.as_ref().unwrap().is_some());
        assert_counts(&observed, states, maximum);
    }
    let negated = Regex::new(b"(?(?!a)b|a)").unwrap().diagnose(b"", true);
    assert!(negated.result.as_ref().unwrap().is_none());
    assert_counts(&negated, 4, 100_121);
}

#[test]
fn control_verbs_stop_at_source_assertion_and_atomic_boundaries() {
    for (pattern, states, maximum, start, end) in [
        (b"(?=a(*COMMIT)b)|a".as_slice(), 8, 100_289, 0, 1),
        (b"(?=a(*PRUNE)b)|a", 8, 100_256, 0, 1),
        (b"(?=a(*SKIP)b)|a", 8, 100_225, 0, 1),
        (b"(?!a(*THEN)b)|a", 12, 100_225, 1, 1),
        (b"(?>a(*THEN)b)|a", 8, 100_225, 0, 1),
        (b"(?=a*(*COMMIT)b)|a", 9, 100_324, 0, 1),
        (b"(?(?=a(*COMMIT)b)c|a)", 8, 100_441, 0, 1),
    ] {
        let observed = Regex::new(pattern).unwrap().diagnose(b"a", false);
        assert_eq!(
            observed.result.as_ref().unwrap().as_ref().unwrap().span(),
            start..end,
        );
        assert_counts(&observed, states, maximum);
    }
    let captures = Regex::new(b"(?=(a)(*COMMIT)b)c|ac")
        .unwrap()
        .captures(b"ac")
        .unwrap()
        .unwrap();
    assert_eq!(captures.span(), 0..2);
    assert_eq!(captures.get(1), None);
}

#[test]
fn explicit_recursion_uses_source_return_dispatch_and_separate_depth_budget() {
    let simple = Regex::new(b"(a)(?1)b").unwrap().diagnose(b"aab", true);
    assert!(simple.result.as_ref().unwrap().is_some());
    assert_counts(&simple, 12, 100_192);

    let mut input = vec![b'a'; 100];
    input.extend_from_slice(&[b'b'; 100]);
    let deep = Regex::new(b"(?<x>a(?&x)?b)")
        .unwrap()
        .diagnose(&input, true);
    assert_eq!(
        deep.result.as_ref().unwrap().as_ref().unwrap().span(),
        0..200
    );
    assert_counts(&deep, 997, 140_000);
}

#[test]
fn nested_matcher_depth_preserves_success_and_failure_boundary() {
    for (depth, states, maximum) in [
        (78, 159, 197_969),
        (79, 161, 200_489),
        (80, 81, 203_041),
        (81, 81, 205_625),
    ] {
        let mut pattern = b"(?>".repeat(depth);
        pattern.push(b'a');
        pattern.extend(std::iter::repeat_n(b')', depth));
        let observed = Regex::new(&pattern).unwrap().diagnose(b"a", true);
        if depth < 80 {
            assert!(observed.result.as_ref().unwrap().is_some());
        } else {
            assert_execution_error(&observed, 18);
        }
        assert_counts(&observed, states, maximum);
    }
}

#[test]
fn complexity_failure_counts_the_complete_failed_dispatch() {
    let regex = Regex::new(b"(a+)+$").unwrap();
    for (length, states, maximum) in [
        (14, 73_726, 100_540),
        (15, 100_582, 100_576),
        (16, 100_618, 100_612),
    ] {
        let mut input = vec![b'a'; length];
        input.push(b'b');
        let observed = regex.diagnose(&input, true);
        if length == 14 {
            assert!(observed.result.as_ref().unwrap().is_none());
        } else {
            assert_execution_error(&observed, 18);
        }
        assert_counts(&observed, states, maximum);
    }
    let recovered = regex.diagnose(b"a", true);
    assert!(recovered.result.as_ref().unwrap().is_some());
    assert_counts(&recovered, 11, 100_036);
}

#[test]
fn backtracking_blocks_restore_after_exhaustion() {
    let regex = Regex::new(b"(a)*").unwrap();
    for (length, states) in [(36_900, 184_503), (36_901, 184_507), (36_902, 184_507)] {
        let observed = regex.diagnose(&vec![b'a'; length], true);
        if length == 36_900 {
            assert_eq!(
                observed.result.as_ref().unwrap().as_ref().unwrap().span(),
                0..length
            );
        } else {
            assert_execution_error(&observed, 19);
        }
        assert_counts(&observed, states, 100_000_000);
    }
    let recovered = regex.diagnose(b"a", true);
    assert!(recovered.result.as_ref().unwrap().is_some());
    assert_counts(&recovered, 8, 100_016);
}

#[test]
fn threshold_cap_applies_only_to_the_input_square_term() {
    for (pattern_length, input_length, maximum) in [
        (1, 9_994, 99_980_036),
        (1, 9_995, 100_000_000),
        (101, 10_000, 102_110_000),
    ] {
        let regex = Regex::new(&vec![b'b'; pattern_length]).unwrap();
        let observed = regex.diagnose(&vec![b'a'; input_length], true);
        assert!(observed.result.as_ref().unwrap().is_none());
        assert_counts(&observed, 2, maximum);
    }
}
