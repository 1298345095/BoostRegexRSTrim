use boost_regex_rs_trim::{Limits, Regex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expression = Regex::compile(
        b"(?<word>[a-z]+)",
        Limits {
            pattern: 1024,
            input: 4096,
            template: 1024,
            output: 8192,
            matches: 100,
        },
    )?;
    let input = b"123 abc\x00def\xff";
    let matched = expression
        .captures(input)?
        .expect("the input contains a word");
    assert_eq!(matched.span(), 4..7);
    assert_eq!(expression.named_capture(&matched, b"word"), Some(4..7));
    assert_eq!(expression.find_all(input)?.len(), 2);
    assert_eq!(
        expression.replace(input, b"<$+{word}>")?,
        b"123 <abc>\x00<def>\xff"
    );
    assert!(expression.full_match(b"abc")?.is_some());
    let error = Regex::new(b"\x00\xff(").unwrap_err();
    assert!(error.raw_message().contains(&255));
    assert!(error.display_text().contains("\\xFF"));
    Ok(())
}
