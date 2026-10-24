use boost_regex_rs_trim::{Error, Regex};

fn main() -> Result<(), Error> {
    let regex = Regex::new(b"(?<word>[a-z]+)")?;
    let input = b"\xff\0abc 123";
    let Some(matched) = regex.captures(input)? else {
        return Ok(());
    };
    assert_eq!(matched.span(), 2..5);
    assert_eq!(regex.named_capture(&matched, b"word"), Some(2..5));
    let spans: Vec<_> = regex
        .find_all(b"one 123 two")?
        .iter()
        .map(boost_regex_rs_trim::Match::span)
        .collect();
    assert_eq!(spans, [0..3, 8..11]);
    assert!(regex.full_match(b"abc")?.is_some());
    let output = regex.replace(input, b"<$+{word}>")?;
    assert_eq!(output, b"\xff\0<abc> 123");
    println!("match: {:?}; output bytes: {output:02x?}", matched.span());
    let invalid = Regex::new(b"\xff\0[").unwrap_err();
    println!("raw diagnostic bytes: {:02x?}", invalid.raw_message());
    println!("display diagnostic: {}", invalid.display_text());
    Ok(())
}
