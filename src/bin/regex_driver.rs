//! Explicit test/measurement driver. Cargo never builds this for a library consumer.

use boost_regex_rs_trim::{CharacterRules, Error, Limits, Match, Phase, Regex};
use serde_json::{Value, json};
use std::io::{self, BufRead};

mod measurement {
    // The only unsafe operations in the driver call operating-system APIs.
    #![allow(unsafe_code)]

    #[cfg(all(unix, feature = "measurements"))]
    pub fn peak_rss_bytes() -> std::io::Result<u64> {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: libc supplies the platform's exact rusage layout. The output
        // pointer is aligned, writable, and lives for the complete system call.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: The integer-only buffer was zero-initialized, and a successful
        // getrusage call filled its fields. No reference into it outlives usage.
        let usage = unsafe { usage.assume_init() };
        let value = u64::try_from(usage.ru_maxrss)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        #[cfg(target_vendor = "apple")]
        {
            Ok(value)
        }
        #[cfg(not(target_vendor = "apple"))]
        {
            Ok(value.saturating_mul(1024))
        }
    }

    #[cfg(all(windows, feature = "measurements"))]
    pub fn peak_rss_bytes() -> std::io::Result<u64> {
        use windows_sys::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        // SAFETY: PROCESS_MEMORY_COUNTERS contains only integer fields, so all
        // zero bits represent valid values, matching the old Default struct.
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>())
            .expect("Windows memory counter structure fits in DWORD");
        // SAFETY: GetCurrentProcess returns a pseudo-handle requiring no close;
        // counters is writable and uses the native ABI layout and exact size.
        let success =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, size) };
        if success == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(counters.PeakWorkingSetSize as u64)
    }

    #[cfg(not(feature = "measurements"))]
    pub fn peak_rss_bytes() -> std::io::Result<u64> {
        Err(std::io::Error::other(
            "Build with --release --features measurements to collect performance measurements",
        ))
    }
}

fn hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn unhex(value: &str) -> Result<Vec<u8>, String> {
    // Keep the original protocol errors instead of exposing hex crate errors.
    if !value.len().is_multiple_of(2) {
        return Err("hex field must have even length".into());
    }
    hex::decode(value).map_err(|_| "invalid hex field".to_owned())
}

fn match_json(value: &Match) -> Value {
    let span = value.span();
    let captures = value
        .captures
        .iter()
        .map(|range| range.as_ref().map(|range| [range.start, range.end]))
        .collect::<Vec<_>>();
    json!({
        "span": [span.start, span.end],
        "captures": captures
    })
}

fn error_json(error: &Error) -> Value {
    let phase = match error.phase {
        Phase::Compile => "compile",
        Phase::Execute => "execute",
        Phase::CallerLimit => "limit",
        Phase::CharacterRules => "locale",
    };
    json!({
        "ok": false,
        "phase": phase,
        "code": error.code,
        "position": error.position,
        "message_hex": hex(error.raw_message())
    })
}

fn request(line: &str, rules: &CharacterRules) -> Result<Value, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() != 4 {
        return Err("request must contain four tab-separated fields".into());
    }
    let pattern = unhex(fields[1])?;
    let input = unhex(fields[2])?;
    let template = unhex(fields[3])?;
    let regex = match Regex::with_character_rules(&pattern, Limits::UNLIMITED, rules.clone()) {
        Ok(regex) => regex,
        Err(error) => return Ok(error_json(&error)),
    };
    let result = match fields[0] {
        #[cfg(feature = "diagnostics")]
        "accounting_search" | "accounting_full" => {
            return Ok(accounting_result(
                &regex,
                &input,
                fields[0] == "accounting_full",
            ));
        }
        "compile" => Ok(json!({
            "ok": true,
            "mark_count": regex.mark_count()
        })),
        "search" | "full" => {
            let result = if fields[0] == "full" {
                regex.full_match(&input)
            } else {
                regex.captures(&input)
            };
            result.map(|matched| {
                json!({
                    "ok": true,
                    "match": matched.as_ref().map(match_json)
                })
            })
        }
        "find_all" => regex.find_all(&input).map(|matches| {
            json!({
                "ok": true,
                "matches": matches.iter().map(match_json).collect::<Vec<_>>()
            })
        }),
        "replace" => regex.replace(&input, &template).map(|output| {
            json!({
                "ok": true,
                "output_hex": hex(&output)
            })
        }),
        _ => return Err("unknown operation".into()),
    };
    Ok(result.unwrap_or_else(|error| error_json(&error)))
}

fn mix(checksum: &mut u64, value: u64) {
    *checksum = checksum.wrapping_mul(1_099_511_628_211) ^ value;
}

fn mix_match(checksum: &mut u64, matched: Option<&Match>) {
    mix(checksum, u64::from(matched.is_some()));
    if let Some(matched) = matched {
        mix(checksum, matched.captures.len() as u64);
        for capture in &matched.captures {
            mix(checksum, u64::from(capture.is_some()));
            if let Some(range) = capture {
                mix(checksum, range.start as u64);
                mix(checksum, range.end as u64);
            }
        }
    }
}

fn benchmark(measured: bool, fields: &[String], rules: &CharacterRules) -> Result<Value, String> {
    if fields.len() != 5 {
        return Err(
            "Expected benchmark OPERATION PATTERN_HEX INPUT_HEX TEMPLATE_HEX ITERATIONS".into(),
        );
    }
    if measured && !cfg!(feature = "measurements") {
        return Err(
            "Build with --release --features measurements to collect performance measurements"
                .into(),
        );
    }
    let operation = fields[0].as_str();
    if !matches!(operation, "compile" | "search" | "full" | "replace") {
        return Err("Unknown benchmark operation".into());
    }
    let pattern = unhex(&fields[1])?;
    let input = unhex(&fields[2])?;
    let template = unhex(&fields[3])?;
    let iterations = fields[4]
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    if iterations == 0 {
        return Err("Benchmark iterations must be a positive integer".into());
    }
    let compiled = if operation == "compile" {
        None
    } else {
        match Regex::with_character_rules(&pattern, Limits::UNLIMITED, rules.clone()) {
            Ok(regex) => Some(regex),
            Err(error) => return Ok(error_json(&error)),
        }
    };
    let mut checksum = 14_695_981_039_346_656_037_u64;
    // Check mode does not call Instant::now or read process memory.
    let started = measured.then(std::time::Instant::now);
    for _ in 0..iterations {
        if operation == "compile" {
            match Regex::with_character_rules(&pattern, Limits::UNLIMITED, rules.clone()) {
                Ok(regex) => {
                    mix(&mut checksum, regex.mark_count() as u64);
                    mix(&mut checksum, pattern.len() as u64);
                }
                Err(error) => return Ok(error_json(&error)),
            }
        } else {
            let regex = compiled
                .as_ref()
                .expect("non-compile operations compile before the loop");
            if operation == "replace" {
                match regex.replace(&input, &template) {
                    Ok(output) => {
                        mix(&mut checksum, output.len() as u64);
                        for byte in output {
                            mix(&mut checksum, u64::from(byte));
                        }
                    }
                    Err(error) => return Ok(error_json(&error)),
                }
            } else {
                let result = if operation == "full" {
                    regex.full_match(&input)
                } else {
                    regex.captures(&input)
                };
                match result {
                    Ok(matched) => mix_match(&mut checksum, matched.as_ref()),
                    Err(error) => return Ok(error_json(&error)),
                }
            }
        }
    }
    // End the operation interval before JSON formatting or reading memory stats.
    let elapsed = started.map(|start| start.elapsed().as_nanos());
    let mut response = json!({
        "ok": true,
        "operation": operation,
        "iterations": iterations,
        "checksum": checksum,
        "measured": measured
    });
    if let Some(elapsed) = elapsed {
        let peak = measurement::peak_rss_bytes().map_err(|error| error.to_string())?;
        response["elapsed_ns"] = json!(elapsed);
        response["peak_rss_bytes"] = json!(peak);
    }
    Ok(response)
}

#[cfg(feature = "diagnostics")]
fn accounting(fields: &[String], rules: &CharacterRules) -> Result<Value, String> {
    if fields.len() != 3 || !matches!(fields[0].as_str(), "search" | "full") {
        return Err("Expected --accounting search|full PATTERN_HEX INPUT_HEX".into());
    }
    let pattern = unhex(&fields[1])?;
    let input = unhex(&fields[2])?;
    let regex = match Regex::with_character_rules(&pattern, Limits::UNLIMITED, rules.clone()) {
        Ok(regex) => regex,
        Err(error) => return Ok(error_json(&error)),
    };
    Ok(accounting_result(&regex, &input, fields[0] == "full"))
}

#[cfg(feature = "diagnostics")]
fn accounting_result(regex: &Regex, input: &[u8], full: bool) -> Value {
    let diagnostic = regex.diagnose(input, full);
    let mut result = match diagnostic.result {
        Ok(matched) => json!({
            "ok": true,
            "match": matched.as_ref().map(match_json)
        }),
        Err(error) => error_json(&error),
    };
    result["accounting"] = json!({
        "state_count": diagnostic.state_count,
        "max_state_count": diagnostic.max_state_count,
        "blocks_remaining_after_cleanup": diagnostic.blocks_remaining_after_cleanup
    });
    result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rules = CharacterRules::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            #[cfg(unix)]
            "--locale" => {
                rules = CharacterRules::with_locale(&args.next().ok_or("missing locale")?)?;
            }
            #[cfg(windows)]
            "--lcid" => {
                let value = args.next().ok_or("missing LCID")?;
                let lcid = if let Some(hex) = value.strip_prefix("0x") {
                    u32::from_str_radix(hex, 16)?
                } else {
                    value.parse()?
                };
                rules = CharacterRules::with_lcid(lcid);
            }
            "--environment" => {
                println!(
                    "{}",
                    json!({
                        "engine": "rust",
                        "os": std::env::consts::OS,
                        "arch": std::env::consts::ARCH,
                        "debug_assertions": cfg!(debug_assertions),
                        "measurements": cfg!(feature = "measurements"),
                        "diagnostics": cfg!(feature = "diagnostics")
                    })
                );
                return Ok(());
            }
            "--benchmark" | "--benchmark-check" => {
                let fields = args.collect::<Vec<_>>();
                println!("{}", benchmark(arg == "--benchmark", &fields, &rules)?);
                return Ok(());
            }
            "--benchmark-stdin" | "--benchmark-check-stdin" => {
                let mut line = String::new();
                if io::stdin().lock().read_line(&mut line)? == 0 {
                    return Err("Expected a benchmark workload on standard input".into());
                }
                let fields = line
                    .trim_end_matches(['\r', '\n'])
                    .split('\t')
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                println!(
                    "{}",
                    benchmark(arg == "--benchmark-stdin", &fields, &rules)?
                );
                return Ok(());
            }
            #[cfg(feature = "diagnostics")]
            "--accounting" => {
                let fields = args.collect::<Vec<_>>();
                println!("{}", accounting(&fields, &rules)?);
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    for line in io::stdin().lock().lines() {
        match request(line?.trim_end_matches('\r'), &rules) {
            Ok(output) => println!("{output}"),
            Err(message) => println!(
                "{}",
                json!({
                    "ok": false,
                    "phase": "protocol",
                    "code": null,
                    "position": null,
                    "message_hex": hex(message.as_bytes())
                })
            ),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_codec_and_protocol_errors_remain_stable() {
        assert_eq!(hex(b"\x00\xffA"), "00ff41");
        assert_eq!(unhex("00Ff41").unwrap(), b"\x00\xffA");
        assert_eq!(unhex("123").unwrap_err(), "hex field must have even length");
        assert_eq!(unhex("gg").unwrap_err(), "invalid hex field");
    }

    #[test]
    fn exact_wire_json_key_order_and_nulls() {
        let rules = CharacterRules::default();
        assert_eq!(
            request("compile\t286129\t\t", &rules).unwrap().to_string(),
            r#"{"ok":true,"mark_count":1}"#
        );
        assert_eq!(
            request("search\t61\t7861\t", &rules).unwrap().to_string(),
            r#"{"ok":true,"match":{"span":[1,2],"captures":[[1,2]]}}"#
        );
        assert_eq!(
            request("full\t61\t78\t", &rules).unwrap().to_string(),
            r#"{"ok":true,"match":null}"#
        );
        assert_eq!(
            request("replace\t61\t7861\tff", &rules)
                .unwrap()
                .to_string(),
            r#"{"ok":true,"output_hex":"78ff"}"#
        );
        assert_eq!(
            json!({
                "ok": false,
                "phase": "protocol",
                "code": null,
                "position": null,
                "message_hex": hex(b"invalid hex field")
            })
            .to_string(),
            r#"{"ok":false,"phase":"protocol","code":null,"position":null,"message_hex":"696e76616c696420686578206669656c64"}"#
        );
        let invalid = request("compile\t28\t\t", &rules).unwrap().to_string();
        assert!(
            invalid.starts_with(
                r#"{"ok":false,"phase":"compile","code":8,"position":1,"message_hex":""#
            )
        );
    }

    #[cfg(feature = "diagnostics")]
    #[test]
    fn accounting_fields_follow_the_unmodified_result() {
        let value = request("accounting_search\t61\t61\t", &CharacterRules::default()).unwrap();
        let wire = value.to_string();
        assert!(
            wire.starts_with(
                r#"{"ok":true,"match":{"span":[0,1],"captures":[[0,1]]},"accounting":{"#
            )
        );
        assert!(wire.contains(r#""state_count":"#));
        assert!(wire.contains(r#""max_state_count":"#));
        assert!(wire.contains(r#""blocks_remaining_after_cleanup":"#));
    }
}
