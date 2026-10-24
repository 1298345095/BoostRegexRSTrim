use boost_regex_rs_trim::{Error, LimitKind, Limits, Phase, Regex};

fn main() -> Result<(), Error> {
    let limits = Limits {
        pattern: 64,
        input: 128,
        template: 64,
        output: 8,
        matches: 2,
    };
    let regex = Regex::compile(b"[a-z]+", limits)?;
    let error = regex.replace(b"one two", b"<$&>").unwrap_err();
    assert_eq!(error.phase, Phase::CallerLimit);
    assert_eq!(error.limit_kind, Some(LimitKind::Output));
    println!("replacement returned no partial output: {error}");

    // A failed operation leaves the immutable expression ready for reuse.
    assert_eq!(regex.replace(b"one", b"<$&>")?, b"<one>");
    assert_eq!(regex.clone().find(b"123 two")?, Some(4..7));
    Ok(())
}
