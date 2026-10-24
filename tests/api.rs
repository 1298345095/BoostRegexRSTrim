use boost_regex_rs_trim::{Error, LimitKind, Limits, Match, Phase, Regex};
use std::ops::Range;
use std::sync::Arc;

fn spans(matches: &[Match]) -> Vec<Range<usize>> {
    matches.iter().map(Match::span).collect()
}

fn assert_limit(error: &Error, kind: LimitKind, maximum: usize) {
    assert_eq!(error.phase, Phase::CallerLimit);
    assert_eq!(error.limit_kind, Some(kind));
    assert_eq!(error.limit, Some(maximum));
    assert!(error.actual.unwrap() > maximum);
    assert_eq!(error.code, None);
}

#[test]
fn byte_patterns_input_and_templates_never_require_utf8() {
    let expression = Regex::new(b"(\x00)(\xff)").unwrap();
    let input = b"x\x00\xffy";
    let matched = expression.captures(input).unwrap().unwrap();
    assert_eq!(matched.span(), 1..3);
    assert_eq!(matched.get(1), Some(1..2));
    assert_eq!(matched.get(2), Some(2..3));
    assert_eq!(expression.find(input).unwrap(), Some(1..3));
    assert!(expression.full_match(input).unwrap().is_none());
    assert!(expression.full_match(b"\x00\xff").unwrap().is_some());
    assert_eq!(
        expression.replace(input, b"\xfe$2$1\x00").unwrap(),
        b"x\xfe\xff\x00\x00y"
    );
}

#[test]
fn captures_distinguish_empty_unmatched_and_named_participation() {
    let expression = Regex::new(b"(?<first>a*)(b)?").unwrap();
    let matched = expression.full_match(b"").unwrap().unwrap();
    assert_eq!(matched.captures, vec![Some(0..0), Some(0..0), None]);
    assert_eq!(expression.named_capture(&matched, b"first"), Some(0..0));
    assert_eq!(expression.named_capture(&matched, b"missing"), None);

    let duplicates = Regex::new(b"(?<x>a)|(?<x>b)").unwrap();
    let matched = duplicates.full_match(b"b").unwrap().unwrap();
    assert_eq!(duplicates.named_capture(&matched, b"x"), Some(0..1));
    assert_eq!(duplicates.replace(b"ab", b"<$+{x}>").unwrap(), b"<a><b>");
}

#[test]
fn enumeration_retries_nonempty_at_the_same_empty_match_position() {
    let empty = Regex::new(b"").unwrap();
    assert_eq!(
        spans(&empty.find_all(b"ab").unwrap()),
        vec![0..0, 1..1, 2..2]
    );
    let alternative = Regex::new(b"|a").unwrap();
    assert_eq!(
        spans(&alternative.find_all(b"a").unwrap()),
        vec![0..0, 0..1, 1..1]
    );
    assert_eq!(alternative.replace(b"a", b"x").unwrap(), b"xxx");
    assert_eq!(
        spans(&Regex::new(b"a*").unwrap().find_all(b"a").unwrap()),
        vec![0..1, 1..1]
    );
}

#[test]
fn reversed_match_span_keeps_reference_advancement_with_a_caller_bound() {
    // The retained iterator treats only equal endpoints as empty. A reset in
    // lookahead can produce a reversed span and repeat without advancing, so
    // the caller bound makes this source behavior observable without hanging.
    let expression = Regex::compile(
        b"(?=a\\K)",
        Limits {
            matches: 2,
            ..Limits::UNLIMITED
        },
    )
    .unwrap();
    let matched = expression.captures(b"ab").unwrap().unwrap();
    assert_eq!((matched.span().start, matched.span().end), (1, 0));
    let error = expression.find_all(b"ab").unwrap_err();
    assert_limit(&error, LimitKind::Matches, 2);
    assert_eq!(error.actual, Some(3));
    assert_eq!(expression.captures(b"ab").unwrap(), Some(matched));
}

#[test]
fn pattern_limit_below_at_and_above_required_size() {
    for maximum in [2, 3, 4] {
        let result = Regex::compile(
            b"(a)",
            Limits {
                pattern: maximum,
                ..Limits::UNLIMITED
            },
        );
        if maximum < 3 {
            assert_limit(&result.unwrap_err(), LimitKind::Pattern, maximum);
        } else {
            assert_eq!(result.unwrap().find(b"a").unwrap(), Some(0..1));
        }
    }
    assert!(
        Regex::compile(
            b"",
            Limits {
                pattern: 0,
                ..Limits::UNLIMITED
            }
        )
        .is_ok()
    );
}

#[test]
fn input_limit_below_at_and_above_required_size() {
    for maximum in [2, 3, 4] {
        let expression = Regex::compile(
            b"a+",
            Limits {
                input: maximum,
                ..Limits::UNLIMITED
            },
        )
        .unwrap();
        let result = expression.find(b"aaa");
        if maximum < 3 {
            assert_limit(&result.unwrap_err(), LimitKind::Input, maximum);
            assert_limit(
                &expression.find_all(b"aaa").unwrap_err(),
                LimitKind::Input,
                maximum,
            );
            assert_limit(
                &expression.replace(b"aaa", b"x").unwrap_err(),
                LimitKind::Input,
                maximum,
            );
        } else {
            assert_eq!(result.unwrap(), Some(0..3));
        }
        assert_eq!(expression.find(b"a").unwrap(), Some(0..1));
    }
}

#[test]
fn template_limit_below_at_and_above_required_size() {
    for maximum in [3, 4, 5] {
        let expression = Regex::compile(
            b"a",
            Limits {
                template: maximum,
                ..Limits::UNLIMITED
            },
        )
        .unwrap();
        let result = expression.replace(b"a", b"<$0>");
        if maximum < 4 {
            assert_limit(&result.unwrap_err(), LimitKind::Template, maximum);
        } else {
            assert_eq!(result.unwrap(), b"<a>");
        }
        assert_eq!(expression.replace(b"a", b"x").unwrap(), b"x");
    }
    let expression = Regex::compile(
        b"a",
        Limits {
            template: 0,
            ..Limits::UNLIMITED
        },
    )
    .unwrap();
    assert_eq!(expression.replace(b"a", b"").unwrap(), b"");
}

#[test]
fn output_limit_covers_expansions_and_unmatched_bytes_atomically() {
    for maximum in [4, 5, 6] {
        let expression = Regex::compile(
            b"(a)",
            Limits {
                output: maximum,
                ..Limits::UNLIMITED
            },
        )
        .unwrap();
        let result = expression.replace(b"xay", b"<$1>");
        if maximum < 5 {
            assert_limit(&result.unwrap_err(), LimitKind::Output, maximum);
        } else {
            assert_eq!(result.unwrap(), b"x<a>y");
        }
        assert_eq!(expression.replace(b"a", b"$0").unwrap(), b"a");
    }
    let expression = Regex::compile(
        b"a",
        Limits {
            output: 2,
            ..Limits::UNLIMITED
        },
    )
    .unwrap();
    assert_limit(
        &expression.replace(b"xyz", b"").unwrap_err(),
        LimitKind::Output,
        2,
    );
    assert_limit(
        &expression.replace(b"a", b"\\U$0$0$0").unwrap_err(),
        LimitKind::Output,
        2,
    );
    assert_eq!(expression.replace(b"a", b"\\U$0").unwrap(), b"A");
    let zero = Regex::compile(
        b"a",
        Limits {
            output: 0,
            ..Limits::UNLIMITED
        },
    )
    .unwrap();
    assert_eq!(zero.replace(b"a", b"").unwrap(), b"");
}

#[test]
fn match_count_limit_covers_collections_and_replacement_atomically() {
    for maximum in [1, 2, 3] {
        let expression = Regex::compile(
            b"a",
            Limits {
                matches: maximum,
                ..Limits::UNLIMITED
            },
        )
        .unwrap();
        let collected = expression.find_all(b"aa");
        let replaced = expression.replace(b"aa", b"x");
        if maximum < 2 {
            assert_limit(&collected.unwrap_err(), LimitKind::Matches, maximum);
            assert_limit(&replaced.unwrap_err(), LimitKind::Matches, maximum);
        } else {
            assert_eq!(spans(&collected.unwrap()), vec![0..1, 1..2]);
            assert_eq!(replaced.unwrap(), b"xx");
        }
        assert_eq!(spans(&expression.find_all(b"a").unwrap()), vec![0..1]);
    }
    let zero = Regex::compile(
        b"a",
        Limits {
            matches: 0,
            ..Limits::UNLIMITED
        },
    )
    .unwrap();
    assert_eq!(zero.find_all(b"b").unwrap(), [] as [Match; 0]);
    assert_eq!(zero.replace(b"b", b"x").unwrap(), b"b");
    assert_limit(&zero.find_all(b"a").unwrap_err(), LimitKind::Matches, 0);
    let empty = Regex::compile(
        b"",
        Limits {
            matches: 1,
            ..Limits::UNLIMITED
        },
    )
    .unwrap();
    assert_limit(&empty.find_all(b"a").unwrap_err(), LimitKind::Matches, 1);
    assert_eq!(spans(&empty.find_all(b"").unwrap()), vec![0..0]);
}

#[test]
fn raw_error_bytes_are_distinct_from_display_text() {
    let error = Regex::new(b"\x00\xff(").unwrap_err();
    assert_eq!(error.phase, Phase::Compile);
    assert_eq!(error.code, Some(8));
    assert!(error.raw_message().contains(&0));
    assert!(error.raw_message().contains(&255));
    let raw = error.raw_message().to_vec();
    assert!(error.display_text().contains("\\x00\\xFF"));
    assert_eq!(error.raw_message(), raw);
}

#[test]
fn accept_preserves_raw_numbered_expansions_of_unmatched_groups() {
    let expression = Regex::new(b"(*ACCEPT)(?<skipped>a)").unwrap();
    let matched = expression.captures(b"ab").unwrap().unwrap();
    assert_eq!(matched.get(1), None);
    assert_eq!(expression.named_capture(&matched, b"skipped"), None);
    for template in [
        &b"<$1>"[..],
        &b"<${1}>"[..],
        &b"<\\1>"[..],
        &b"<$+>"[..],
        &b"<$LAST_PAREN_MATCH>"[..],
    ] {
        assert_eq!(expression.replace(b"ab", template).unwrap(), b"<ab>a<b>b<>");
    }
    assert_eq!(
        expression.replace(b"ab", b"<$+{skipped}>").unwrap(),
        b"<>a<>b<>"
    );
    // A failed nonempty retry closes a group before the next empty match.
    // The current raw bounds are used through the retained last-closed index.
    assert_eq!(expression.replace(b"ab", b"<$^N>").unwrap(), b"<>a<b>b<>");
    assert_eq!(
        expression
            .replace(b"aa", b"<$LAST_SUBMATCH_RESULT>")
            .unwrap(),
        b"<>a<a>a<>"
    );
    assert_eq!(
        Regex::new(b"(a)(*ACCEPT)(?<x>b)")
            .unwrap()
            .replace(b"abc", b"<$1|$2|$+|$+{x}|$^N>")
            .unwrap(),
        b"<a|bc|bc||a>bc",
    );
}

#[test]
fn nested_atomic_computation_failure_does_not_poison_reuse() {
    // The reference's match_all_states native call depth includes the initial
    // invocation. The 80th nested atomic group therefore fails with code 18.
    let mut pattern = b"(?:".to_vec();
    for _ in 0..80 {
        pattern.extend_from_slice(b"(?>");
    }
    pattern.push(b'a');
    pattern.extend(std::iter::repeat_n(b')', 80));
    pattern.extend_from_slice(b"|b)");
    let expression = Regex::new(&pattern).unwrap();
    for _ in 0..3 {
        let error = expression.find(b"a").unwrap_err();
        assert_eq!(error.phase, Phase::Execute);
        assert_eq!(error.code, Some(18));
        assert_eq!(expression.find(b"b").unwrap(), Some(0..1));
        assert_eq!(expression.replace(b"b", b"ok").unwrap(), b"ok");
    }
}

#[test]
fn compiled_expressions_share_without_execution_state_leaks() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Regex>();
    let recursive = Arc::new(Regex::new(b"(?<pair>a(?&pair)?b)").unwrap());
    let assertion = Arc::new(
        Regex::compile(
            b"(?=(a+))a+",
            Limits {
                output: 3,
                ..Limits::UNLIMITED
            },
        )
        .unwrap(),
    );
    let expected = recursive.full_match(b"aaabbb").unwrap().unwrap();
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let recursive = Arc::clone(&recursive);
            let assertion = Arc::clone(&assertion);
            let expected = expected.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    assert!(recursive.full_match(b"aaabb").unwrap().is_none());
                    assert_eq!(
                        recursive.full_match(b"aaabbb").unwrap(),
                        Some(expected.clone())
                    );
                    assert_eq!(
                        recursive.replace(b"zaaabbby", b"<$+{pair}>").unwrap(),
                        b"z<aaabbb>y"
                    );
                    assert_limit(
                        &assertion.replace(b"aa", b"$1$1").unwrap_err(),
                        LimitKind::Output,
                        3,
                    );
                    assert_eq!(assertion.replace(b"a", b"$1$1").unwrap(), b"aa");
                    assert_eq!(
                        assertion.captures(b"a").unwrap().unwrap().get(1),
                        Some(0..1)
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn backwards_replacement_prefix_returns_an_error_without_panicking() {
    let regex = Regex::new(b"a|(?<=\\Ka)b").unwrap();
    let matches = regex.find_all(b"ab").unwrap();
    assert_eq!(
        matches
            .iter()
            .map(boost_regex_rs_trim::Match::span)
            .collect::<Vec<_>>(),
        [0..1, 0..2]
    );
    let error = regex.replace(b"ab", b"X").unwrap_err();
    assert_eq!(error.phase, Phase::Execute);
    assert_eq!(error.code, None);
    assert_eq!(error.position, None);
    assert_eq!(regex.replace(b"a", b"X").unwrap(), b"X");
}
