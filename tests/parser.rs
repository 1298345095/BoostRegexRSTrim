use boost_regex_rs_trim::{Phase, Regex};

#[test]
fn retained_group_forms_compile_with_reference_numbering() {
    let cases: &[(&[u8], usize)] = &[
        (b"", 0),
        (b"(a)(?:b)(c)", 2),
        (b"(?|(a)|(b)(c))(d)", 3),
        (b"(?<x>a)\\k<x>", 1),
        (b"(?'x'a)\\g{x}", 1),
        (b"(?(DEFINE)(?<x>a))(?&x)", 1),
        (b"(a)(?+1)(b)", 2),
        // These unusual forms are deliberately retained by the source parser.
        (b"(a)(?-1a)", 1),
        (b"(?Pi)a", 0),
        (b"(?P:)", 0),
    ];
    for &(pattern, count) in cases {
        assert_eq!(
            Regex::new(pattern).unwrap().mark_count(),
            count,
            "{pattern:?}"
        );
    }
}

#[test]
fn incomplete_and_unknown_extensions_keep_their_raw_diagnostics() {
    let error = Regex::new(b"\0\xff(?)").unwrap_err();
    assert_eq!(error.phase, Phase::Compile);
    assert_eq!((error.code, error.position), (Some(20), Some(2)));
    assert_eq!(error.raw_message(), b"Invalid or unterminated Perl (?...) sequence.  The error occurred while parsing the regular expression: '\0\xff>>>HERE>>>(?)'.");
    assert!(error.display_text().contains("\\x00\\xFF"));
}

#[test]
fn lookbehind_acceptance_follows_the_retained_creator_states() {
    for pattern in [b"(?<=a{2})".as_slice(), b"(?<=a|b)", b"(?<=)", b"(?<!)"] {
        assert!(Regex::new(pattern).is_ok(), "{pattern:?}");
    }
    for pattern in [
        b"(?<=(ab){2})".as_slice(),
        b"(?<=a|bb)",
        b"(?<=\\X)",
        b"(?<=a+)",
    ] {
        let error = Regex::new(pattern).unwrap_err();
        assert_eq!(
            (error.code, error.position),
            (Some(2), Some(0)),
            "{pattern:?}"
        );
        assert_eq!(
            error.raw_message(),
            b"Invalid lookbehind assertion encountered in the regular expression."
        );
    }
}

#[test]
fn parser_diagnostics_precede_creator_validation() {
    let error = Regex::new(b"(?<=a+)((").unwrap_err();
    assert_eq!((error.code, error.position), (Some(8), Some(9)));
    assert!(
        error
            .raw_message()
            .starts_with(b"Unmatched marking parenthesis")
    );
}

#[test]
fn creator_startmap_recursion_uses_source_reachability() {
    for pattern in [b"(?R)".as_slice(), b"x(?<a>(?&a)|b)"] {
        let error = Regex::new(pattern).unwrap_err();
        assert_eq!((error.code, error.position), (Some(2), Some(0)));
        assert_eq!(error.raw_message(), b"Encountered an infinite recursion.");
    }
    for pattern in [b"(?=a)(?R)".as_slice(), b"x(?<a>(?&a))", b"(a(?1)?b)"] {
        assert!(Regex::new(pattern).is_ok(), "{pattern:?}");
    }
}

#[test]
fn creator_depth_counts_reachable_word_assertions() {
    let accepted = [b"\\<".repeat(100), b"a".to_vec()].concat();
    assert!(Regex::new(&accepted).is_ok());
    let rejected = [b"\\<".repeat(101), b"a".to_vec()].concat();
    let error = Regex::new(&rejected).unwrap_err();
    assert_eq!((error.code, error.position), (Some(18), Some(0)));
    assert_eq!(error.raw_message(), b"Expression complexity exceeded.");
    // The source's map stops at an initial literal, so these are not traversed.
    assert!(Regex::new(&[b"a".to_vec(), b"\\<".repeat(101)].concat()).is_ok());
}

#[test]
fn empty_quoting_does_not_create_a_repeatable_state() {
    assert!(
        Regex::new(b"\\Q\\E")
            .unwrap()
            .full_match(b"")
            .unwrap()
            .is_some()
    );
    let error = Regex::new(b"\\Q\\E*").unwrap_err();
    assert_eq!((error.code, error.position), (Some(13), Some(5)));
    assert!(
        Regex::new(b"a\\Q\\E*")
            .unwrap()
            .full_match(b"aaa")
            .unwrap()
            .is_some()
    );
}

#[test]
fn parser_nesting_limit_preserves_position_and_raw_message() {
    let accepted = [b"(".repeat(399), b"a".to_vec(), b")".repeat(399)].concat();
    assert_eq!(Regex::new(&accepted).unwrap().mark_count(), 399);
    let rejected = [b"(".repeat(400), b"a".to_vec(), b")".repeat(400)].concat();
    let error = Regex::new(&rejected).unwrap_err();
    assert_eq!((error.code, error.position), (Some(18), Some(400)));
    assert!(
        error
            .raw_message()
            .starts_with(b"Exceeded nested brace limit.")
    );
}

#[test]
fn accepted_deep_groups_fit_a_small_native_stack() {
    // Exercise the native one-MiB stack size without changing the parser's
    // reference limit or the normal test thread stack. This catches recursive
    // debug-frame growth in both parsing and instruction lowering.
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            for (prefix, suffix) in [
                (b"(".as_slice(), b")".as_slice()),
                (b"(?i:", b")"),
                (b"(?=", b")"),
            ] {
                let pattern = [prefix.repeat(399), b"a".to_vec(), suffix.repeat(399)].concat();
                assert!(Regex::new(&pattern).is_ok(), "{prefix:?}");
            }
            let accepted = [b"(?(?=".repeat(199), b"a".to_vec(), b")a)".repeat(199)].concat();
            assert!(Regex::new(&accepted).is_ok());
            let rejected = [b"(?(?=".repeat(200), b"a".to_vec(), b")a)".repeat(200)].concat();
            let error = Regex::new(&rejected).unwrap_err();
            assert_eq!((error.code, error.position), (Some(18), Some(1000)));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn line_end_repeat_maps_publish_partial_nullable_results() {
    // The source initializes the repeat's byte map before walking LineEnd's
    // nullable continuation. Revisiting that partial map must not make its
    // take branch nullable.
    for pattern in [
        b"(?:$)+?".as_slice(),
        b"(?:$)++",
        b"((?>(?:(?=a))*)(?:$){1,3})",
    ] {
        let regex = Regex::new(pattern).unwrap();
        assert!(regex.captures(b"").unwrap().is_none(), "{pattern:?}");
        assert!(regex.full_match(b"").unwrap().is_none(), "{pattern:?}");
    }
    assert!(
        Regex::new(b"(?:$)*")
            .unwrap()
            .full_match(b"")
            .unwrap()
            .is_some()
    );
}

#[test]
fn incomplete_range_endpoint_uses_collation_diagnostic() {
    let error = Regex::new(b"P19+(9=[--[").unwrap_err();
    assert_eq!((error.code, error.position), (Some(3), Some(11)));
    assert!(
        error
            .raw_message()
            .starts_with(b"Invalid collation character.")
    );
}

#[test]
fn lookbehind_tracks_conditional_paths_and_pending_alternative_limit() {
    for pattern in [
        b"(?<=(?(1)a|b))".as_slice(),
        b"(?<=(?(1)))",
        b"(?<=(?(?=a)b|c))",
    ] {
        assert!(Regex::new(pattern).is_ok(), "{pattern:?}");
    }
    let pattern = [b"(?<=".to_vec(), b"(?:a|a)".repeat(1026), b")".to_vec()].concat();
    let error = Regex::new(&pattern).unwrap_err();
    assert_eq!((error.code, error.position), (Some(2), Some(0)));
    assert_eq!(
        error.raw_message(),
        b"Invalid lookbehind assertion encountered in the regular expression."
    );
}

#[test]
fn line_break_expansion_keeps_its_source_depth_and_error_context() {
    let accepted = [b"(".repeat(396), b"\\R".to_vec(), b")".repeat(396)].concat();
    assert!(Regex::new(&accepted).is_ok());
    for (groups, position) in [(397, 8), (398, 5), (399, 0)] {
        let pattern = [b"(".repeat(groups), b"\\R".to_vec(), b")".repeat(groups)].concat();
        let error = Regex::new(&pattern).unwrap_err();
        assert_eq!((error.code, error.position), (Some(18), Some(position)));
        assert!(error.raw_message().windows(5).any(|part| part == b"(?-x:"));
    }
}

#[test]
fn condition_repetition_keeps_retained_possessive_behavior() {
    for pattern in [b"(?(?=a)*b)".as_slice(), b"(?(1)+b)"] {
        let error = Regex::new(pattern).unwrap_err();
        assert_eq!((error.code, error.position), (Some(13), Some(2)));
        assert!(
            error
                .raw_message()
                .starts_with(b"A repetition operator cannot be applied to a zero-width assertion.")
        );
    }
    for (pattern, input) in [
        (b"(?(1)*+b)".as_slice(), b"b".as_slice()),
        (b"(?(1)++b|c)", b"c"),
        (b"(?(?=a)++a|b)", b"a"),
        (b"(?(?=a)++a|b)", b"b"),
        (b"(?(DEFINE)*+b|c)", b"b"),
        (b"(?(DEFINE)++b|c)", b"c"),
    ] {
        assert!(
            Regex::new(pattern)
                .unwrap()
                .full_match(input)
                .unwrap()
                .is_some(),
            "{pattern:?}"
        );
    }
}
