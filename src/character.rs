// Ported from the retained Boost.Regex character traits.
// Copyright John Maddock and contributors. Distributed under BSL-1.0.

//! Platform character rules used by compilation, matching, and formatting.

use std::fmt;
#[cfg(unix)]
use std::sync::Arc;

#[derive(Clone, Debug)]
enum Platform {
    #[cfg(unix)]
    Classic,
    #[cfg(unix)]
    Named(Arc<unix::Locale>),
    #[cfg(windows)]
    Windows(u32),
}

#[derive(Clone, Copy, Debug)]
enum SortSyntax {
    C,
    Unknown,
    Fixed(usize),
    Delimited(u8),
}

/// An immutable snapshot of the reference's byte classification and case maps.
///
/// Unix defaults to the classic C++ locale, independently of environment variables.
/// Windows uses the current user's default LCID, as the retained Win32 traits do.
#[derive(Clone, Debug)]
pub struct CharacterRules {
    lower: [u8; 256],
    upper: [u8; 256],
    classes: [u16; 256],
    platform: Platform,
    sort: SortSyntax,
}

/// A requested native locale could not be constructed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleError(pub String);

impl fmt::Display for LocaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LocaleError {}

#[cfg(unix)]
const ALPHA: u16 = 1 << 0;
#[cfg(unix)]
const CONTROL: u16 = 1 << 1;
#[cfg(unix)]
const DIGIT: u16 = 1 << 2;
#[cfg(unix)]
const GRAPH: u16 = 1 << 3;
#[cfg(unix)]
const LOWER: u16 = 1 << 4;
#[cfg(unix)]
const PRINT: u16 = 1 << 5;
#[cfg(unix)]
const PUNCT: u16 = 1 << 6;
#[cfg(unix)]
const SPACE: u16 = 1 << 7;
#[cfg(unix)]
const UPPER: u16 = 1 << 8;
#[cfg(unix)]
const XDIGIT: u16 = 1 << 9;

impl Default for CharacterRules {
    fn default() -> Self {
        #[cfg(unix)]
        {
            Self::classic()
        }
        #[cfg(windows)]
        {
            // SAFETY: GetUserDefaultLCID has no pointer arguments or preconditions.
            Self::windows(windows::default_lcid())
        }
    }
}

impl CharacterRules {
    #[cfg(unix)]
    fn classic() -> Self {
        let mut result = Self {
            lower: [0; 256],
            upper: [0; 256],
            classes: [0; 256],
            platform: Platform::Classic,
            sort: SortSyntax::Unknown,
        };
        for byte in u8::MIN..=u8::MAX {
            let index = usize::from(byte);
            result.lower[index] = byte.to_ascii_lowercase();
            result.upper[index] = byte.to_ascii_uppercase();
            let mut mask = 0;
            for (yes, bit) in [
                (byte.is_ascii_alphabetic(), ALPHA),
                (byte.is_ascii_control(), CONTROL),
                (byte.is_ascii_digit(), DIGIT),
                (byte.is_ascii_graphic(), GRAPH),
                (byte.is_ascii_lowercase(), LOWER),
                (byte.is_ascii_graphic() || byte == b' ', PRINT),
                (byte.is_ascii_punctuation(), PUNCT),
                (matches!(byte, b' ' | b'\t'..=b'\r'), SPACE),
                (byte.is_ascii_uppercase(), UPPER),
                (byte.is_ascii_hexdigit(), XDIGIT),
            ] {
                if yes {
                    mask |= bit;
                }
            }
            result.classes[index] = mask;
        }
        result
    }

    /// Construct character rules for a named native Unix locale.
    ///
    /// This does not change the process or thread locale. The locale is shared
    /// immutably by cloned rules and released after the final clone is dropped.
    ///
    /// # Errors
    /// Returns an error if the locale name contains NUL or is not installed.
    #[cfg(unix)]
    pub fn with_locale(name: &str) -> Result<Self, LocaleError> {
        if matches!(name, "C" | "POSIX") {
            return Ok(Self::classic());
        }
        let locale = Arc::new(unix::Locale::new(name)?);
        let mut result = Self::classic();
        for byte in u8::MIN..=u8::MAX {
            let index = usize::from(byte);
            let (lower, upper) = locale.case_map(byte);
            result.lower[index] = lower;
            result.upper[index] = upper;
            // libc++'s ctype_byname<char> retains ctype<char>'s classic table
            // and ASCII gate. Only its case maps use the named native locale.
            #[cfg(not(target_os = "macos"))]
            {
                result.classes[index] = locale.classes(byte);
            }
        }
        result.platform = Platform::Named(locale);
        result.sort = result.find_sort_syntax();
        Ok(result)
    }

    /// Construct the retained Win32 byte rules for a specific LCID.
    #[cfg(windows)]
    #[must_use]
    pub fn with_lcid(lcid: u32) -> Self {
        Self::windows(lcid)
    }

    #[cfg(windows)]
    #[allow(
        unsafe_code,
        reason = "Native Win32 calls preserve the retained byte traits."
    )]
    fn windows(lcid: u32) -> Self {
        let mut result = Self {
            lower: [0; 256],
            upper: [0; 256],
            classes: [0; 256],
            platform: Platform::Windows(lcid),
            sort: SortSyntax::Unknown,
        };
        let bytes: [u8; 256] = std::array::from_fn(|i: usize| i.to_le_bytes()[0]);
        // SAFETY: Both arrays contain 256 initialized bytes and the explicit
        // lengths match the allocated readable and writable regions.
        let written = unsafe {
            windows::LCMapStringA(
                lcid,
                windows::LOWERCASE,
                bytes.as_ptr().cast(),
                256,
                result.lower.as_mut_ptr().cast(),
                256,
            )
        };
        // The bulk lower map, including its DBCS-dependent tail policy, is
        // intentionally preserved from w32_regex_traits_char_layer::init.
        let mapped = usize::try_from(written).unwrap_or(0);
        if mapped < bytes.len() {
            result.lower[mapped..].copy_from_slice(&bytes[mapped..]);
        }
        for byte in u8::MIN..=u8::MAX {
            let index = usize::from(byte);
            let mut mask = 0;
            // SAFETY: The source is exactly one initialized byte; the output
            // points to an initialized WORD. Per-byte failure yields zero.
            let classified = unsafe {
                windows::GetStringTypeExA(lcid, 1, (&raw const byte).cast(), 1, &raw mut mask)
            };
            result.classes[index] = if classified == 0 { 0 } else { mask };
            let mut mapped = byte;
            // SAFETY: The source and destination each hold one byte, exactly
            // matching the native call used by w32_toupper.
            let changed = unsafe {
                windows::LCMapStringA(
                    lcid,
                    windows::UPPERCASE,
                    (&raw const byte).cast(),
                    1,
                    (&raw mut mapped).cast(),
                    1,
                )
            };
            result.upper[index] = if changed == 0 { byte } else { mapped };
        }
        result.sort = result.find_sort_syntax();
        result
    }

    #[must_use]
    pub fn lower(&self, byte: u8) -> u8 {
        self.lower[usize::from(byte)]
    }

    #[must_use]
    pub fn upper(&self, byte: u8) -> u8 {
        self.upper[usize::from(byte)]
    }

    /// Test a retained class name, returning `None` for an unknown name.
    #[must_use]
    pub fn is_class(&self, byte: u8, name: &[u8]) -> Option<bool> {
        let lowered: Vec<_> = name.iter().map(|&c| self.lower(c)).collect();
        let mask = self.classes[usize::from(byte)];
        #[cfg(unix)]
        let result = match lowered.as_slice() {
            b"alnum" => mask & (ALPHA | DIGIT) != 0,
            b"alpha" => mask & ALPHA != 0,
            b"blank" => mask & SPACE != 0 && !is_separator(byte),
            b"cntrl" => mask & CONTROL != 0,
            b"d" | b"digit" => mask & DIGIT != 0,
            b"graph" => mask & GRAPH != 0,
            b"h" => mask & SPACE != 0 && !is_vertical(byte),
            b"l" | b"lower" => mask & LOWER != 0,
            b"print" => mask & PRINT != 0,
            b"punct" => mask & PUNCT != 0,
            b"s" | b"space" => mask & SPACE != 0,
            b"u" | b"upper" => mask & UPPER != 0,
            b"unicode" => false,
            b"v" => is_vertical(byte),
            b"w" | b"word" => mask & (ALPHA | DIGIT) != 0 || byte == b'_',
            b"xdigit" => mask & XDIGIT != 0,
            _ => return None,
        };
        #[cfg(windows)]
        let result = match lowered.as_slice() {
            b"alnum" => mask & 0x104 != 0,
            b"alpha" => mask & 0x100 != 0,
            b"blank" => mask & 0x40 != 0,
            b"cntrl" => mask & 0x20 != 0,
            b"d" | b"digit" => mask & 4 != 0,
            b"graph" => mask & (!(0x20 | 8 | 0x40) & 0x1ff) != 0 || byte == b'_',
            b"h" => mask & 8 != 0 && !is_vertical(byte),
            b"l" | b"lower" => mask & 2 != 0,
            b"print" => mask & (!0x20 & 0x1ff) != 0 || byte == b'_',
            b"punct" => mask & 0x10 != 0,
            b"s" | b"space" => mask & 8 != 0,
            b"u" | b"upper" => mask & 1 != 0,
            b"unicode" => false,
            b"v" => is_vertical(byte),
            b"w" | b"word" => mask & 0x104 != 0 || byte == b'_',
            b"xdigit" => mask & 0x80 != 0,
            _ => return None,
        };
        Some(result)
    }

    /// Resolve the reference's POSIX names and retained two-byte collating names.
    #[must_use]
    pub fn collating_name(&self, name: &[u8]) -> Option<Vec<u8>> {
        if let Some((byte, _)) = (0u8..=127)
            .zip(COLLATING_NAMES.iter())
            .find(|(_, entry)| **entry == name)
        {
            return Some(vec![byte]);
        }
        if MULTI_COLLATING_NAMES.contains(&name) || name.len() == 1 {
            return Some(name.to_vec());
        }
        None
    }

    /// A full collation key with the same NUL-avoidance encoding as the reference.
    #[must_use]
    pub fn transform(&self, bytes: &[u8]) -> Vec<u8> {
        let mut raw = self.raw_transform(bytes);
        while raw.last() == Some(&0) {
            raw.pop();
        }
        #[cfg(unix)]
        {
            let mut result = Vec::with_capacity(raw.len() * 2);
            for byte in raw {
                if byte == u8::MAX {
                    result.extend_from_slice(&[u8::MAX, b'b']);
                } else {
                    result.extend_from_slice(&[byte + 1, b'a']);
                }
            }
            result
        }
        #[cfg(windows)]
        raw
    }

    /// A primary collation key, using the retained sort-key format heuristic.
    #[must_use]
    pub fn transform_primary(&self, bytes: &[u8]) -> Vec<u8> {
        let mut result = match self.sort {
            SortSyntax::C | SortSyntax::Unknown => {
                let lower: Vec<_> = bytes.iter().map(|&c| self.lower(c)).collect();
                self.raw_transform(&lower)
            }
            SortSyntax::Fixed(length) => {
                let mut key = self.raw_transform(bytes);
                key.truncate(length);
                key
            }
            SortSyntax::Delimited(delimiter) => {
                let mut key = self.raw_transform(bytes);
                if let Some(index) = key.iter().position(|&c| c == delimiter) {
                    key.truncate(index);
                }
                key
            }
        };
        #[cfg(unix)]
        while result.last() == Some(&0) {
            result.pop();
        }
        if result.is_empty() {
            result.push(0);
        }
        result
    }

    fn raw_transform(&self, bytes: &[u8]) -> Vec<u8> {
        match &self.platform {
            #[cfg(unix)]
            Platform::Classic => bytes.to_vec(),
            #[cfg(unix)]
            Platform::Named(locale) => locale.transform(bytes),
            #[cfg(windows)]
            Platform::Windows(lcid) => windows::transform(*lcid, bytes),
        }
    }

    #[allow(
        clippy::naive_bytecount,
        reason = "These are three tiny native sort keys; no helper dependency is needed."
    )]
    fn find_sort_syntax(&self) -> SortSyntax {
        let lower = self.transform(b"a");
        if lower == b"a" {
            return SortSyntax::C;
        }
        let upper = self.transform(b"A");
        let semicolon = self.transform(b";");
        let mut position = 0;
        while position <= lower.len()
            && position <= upper.len()
            && lower.get(position).unwrap_or(&0) == upper.get(position).unwrap_or(&0)
        {
            position += 1;
        }
        let Some(last_common) = position.checked_sub(1) else {
            return SortSyntax::Unknown;
        };
        let delimiter = *lower.get(last_common).unwrap_or(&0);
        let count = |key: &[u8]| key.iter().filter(|&&c| c == delimiter).count();
        if last_common != 0 && count(&lower) == count(&upper) && count(&lower) == count(&semicolon)
        {
            SortSyntax::Delimited(delimiter)
        } else if lower.len() == upper.len() && lower.len() == semicolon.len() {
            SortSyntax::Fixed(last_common + 1)
        } else {
            SortSyntax::Unknown
        }
    }

    /// Parse the retained traits' integer format and update the position on success.
    /// A negative result denotes a failed or negative extraction, matching `toi`.
    #[allow(
        clippy::unused_self,
        reason = "Keep the numeric operation on the shared platform-traits interface."
    )]
    pub(crate) fn toi(&self, bytes: &[u8], position: &mut usize, end: usize, radix: u32) -> i64 {
        #[cfg(unix)]
        {
            let start = *position;
            let mut cursor = start;
            let end = end.min(bytes.len());
            // The reference creates a fresh stream in the global locale. Named
            // trait locales do not change that stream; the supported process
            // conditions retain the standard ASCII number alphabet.
            while cursor < end && matches!(bytes[cursor], b' ' | b'\t'..=b'\r') {
                cursor += 1;
            }
            let negative = bytes.get(cursor) == Some(&b'-');
            if cursor < end && matches!(bytes[cursor], b'+' | b'-') {
                cursor += 1;
            }
            if radix == 16
                && cursor + 1 < end
                && bytes[cursor] == b'0'
                && matches!(bytes[cursor + 1], b'x' | b'X')
            {
                cursor += 2;
            }
            let first_digit = cursor;
            let mut value = 0u64;
            let maximum = if negative {
                1u64 << 63
            } else {
                i64::MAX as u64
            };
            while cursor < end {
                let Some(digit) = digit(bytes[cursor], radix) else {
                    break;
                };
                let Some(next) = value
                    .checked_mul(u64::from(radix))
                    .and_then(|v| v.checked_add(u64::from(digit)))
                else {
                    return -1;
                };
                if next > maximum {
                    return -1;
                }
                value = next;
                cursor += 1;
            }
            if cursor == first_digit {
                return -1;
            }
            *position = cursor;
            if negative {
                i64::from_ne_bytes(value.to_ne_bytes()).wrapping_neg()
            } else {
                i64::from_ne_bytes(value.to_ne_bytes())
            }
        }
        #[cfg(windows)]
        {
            let end = end.min(bytes.len());
            let mut result = 0i64;
            let mut have_digit = false;
            while *position < end {
                let Some(value) = digit(bytes[*position], radix) else {
                    break;
                };
                // The source advances first and checks its threshold after
                // arithmetic. Preserve MSVC's two's-complement result at that
                // boundary without a Rust debug-overflow panic.
                result = result
                    .wrapping_mul(i64::from(radix))
                    .wrapping_add(i64::from(value));
                *position += 1;
                have_digit = true;
                if result > i64::MAX / i64::from(radix) {
                    return -1;
                }
            }
            if have_digit { result } else { -1 }
        }
    }
}

pub(crate) fn is_separator(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r' | b'\x0c')
}

fn is_vertical(byte: u8) -> bool {
    is_separator(byte) || byte == b'\x0b'
}

fn digit(byte: u8, radix: u32) -> Option<u32> {
    let value = match byte {
        b'0'..=b'9' => u32::from(byte - b'0'),
        b'a'..=b'f' => 10 + u32::from(byte - b'a'),
        b'A'..=b'F' => 10 + u32::from(byte - b'A'),
        _ => return None,
    };
    (value < radix).then_some(value)
}

#[cfg(unix)]
#[allow(unsafe_code)]
mod unix {
    use super::LocaleError;
    #[cfg(not(target_os = "macos"))]
    use super::{ALPHA, CONTROL, DIGIT, GRAPH, LOWER, PRINT, PUNCT, SPACE, UPPER, XDIGIT};
    use std::ffi::{CString, c_char, c_int, c_void};
    use std::ptr::NonNull;

    #[derive(Debug)]
    pub(super) struct Locale(NonNull<c_void>);

    // SAFETY: Locale is created privately by newlocale and is never modified.
    // The native *_l functions permit concurrent reads; Arc controls lifetime.
    unsafe impl Send for Locale {}
    // SAFETY: As above, shared references only perform immutable locale reads.
    unsafe impl Sync for Locale {}

    unsafe extern "C" {
        fn newlocale(mask: c_int, name: *const c_char, base: *mut c_void) -> *mut c_void;
        #[cfg(not(target_os = "macos"))]
        fn freelocale(locale: *mut c_void);
        #[cfg(target_os = "macos")]
        fn freelocale(locale: *mut c_void) -> c_int;
        fn tolower_l(value: c_int, locale: *mut c_void) -> c_int;
        fn toupper_l(value: c_int, locale: *mut c_void) -> c_int;
        fn strxfrm_l(
            destination: *mut c_char,
            source: *const c_char,
            size: usize,
            locale: *mut c_void,
        ) -> usize;
    }

    #[cfg(not(target_os = "macos"))]
    unsafe extern "C" {
        fn isalpha_l(value: c_int, locale: *mut c_void) -> c_int;
        fn iscntrl_l(value: c_int, locale: *mut c_void) -> c_int;
        fn isdigit_l(value: c_int, locale: *mut c_void) -> c_int;
        fn isgraph_l(value: c_int, locale: *mut c_void) -> c_int;
        fn islower_l(value: c_int, locale: *mut c_void) -> c_int;
        fn isprint_l(value: c_int, locale: *mut c_void) -> c_int;
        fn ispunct_l(value: c_int, locale: *mut c_void) -> c_int;
        fn isspace_l(value: c_int, locale: *mut c_void) -> c_int;
        fn isupper_l(value: c_int, locale: *mut c_void) -> c_int;
        fn isxdigit_l(value: c_int, locale: *mut c_void) -> c_int;
    }

    impl Locale {
        pub(super) fn new(name: &str) -> Result<Self, LocaleError> {
            let name_c = CString::new(name)
                .map_err(|_| LocaleError("Locale names cannot contain NUL".into()))?;
            #[cfg(target_os = "macos")]
            let mask = (1 << 0) | (1 << 1); // Darwin LC_COLLATE_MASK and LC_CTYPE_MASK.
            #[cfg(not(target_os = "macos"))]
            let mask = (1 << 0) | (1 << 3); // glibc LC_CTYPE and LC_COLLATE.
            // SAFETY: name_c is NUL terminated and remains alive for the call;
            // null base requests a new owned locale, with only supported masks.
            let locale = unsafe { newlocale(mask, name_c.as_ptr(), std::ptr::null_mut()) };
            NonNull::new(locale)
                .map(Self)
                .ok_or_else(|| LocaleError(format!("Native locale is unavailable: {name}")))
        }

        pub(super) fn case_map(&self, byte: u8) -> (u8, u8) {
            let value = c_int::from(byte);
            let locale = self.0.as_ptr();
            // SAFETY: value is in the unsigned-char domain, the locale is live
            // and immutable, and each call accepts exactly these arguments.
            unsafe {
                (
                    tolower_l(value, locale).to_le_bytes()[0],
                    toupper_l(value, locale).to_le_bytes()[0],
                )
            }
        }

        #[cfg(not(target_os = "macos"))]
        pub(super) fn classes(&self, byte: u8) -> u16 {
            let value = c_int::from(byte);
            let locale = self.0.as_ptr();
            // SAFETY: value is in the unsigned-char domain, the locale is live
            // and immutable, and each call accepts exactly these arguments.
            unsafe {
                let mut classes = 0;
                for (yes, bit) in [
                    (isalpha_l(value, locale), ALPHA),
                    (iscntrl_l(value, locale), CONTROL),
                    (isdigit_l(value, locale), DIGIT),
                    (isgraph_l(value, locale), GRAPH),
                    (islower_l(value, locale), LOWER),
                    (isprint_l(value, locale), PRINT),
                    (ispunct_l(value, locale), PUNCT),
                    (isspace_l(value, locale), SPACE),
                    (isupper_l(value, locale), UPPER),
                    (isxdigit_l(value, locale), XDIGIT),
                ] {
                    if yes != 0 {
                        classes |= bit;
                    }
                }
                classes
            }
        }

        pub(super) fn transform(&self, bytes: &[u8]) -> Vec<u8> {
            let mut result = Vec::new();
            // libc++'s named collate facet passes the entire range as a C
            // string, so an embedded NUL terminates it on macOS. In contrast,
            // libstdc++ transforms each range separately and rejoins with NUL.
            #[cfg(target_os = "macos")]
            let bytes = &bytes[..bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len())];
            // libstdc++ transforms each embedded-NUL-delimited range and retains
            // the separators in the resulting collation key.
            for (index, part) in bytes.split(|&c| c == 0).enumerate() {
                if index != 0 {
                    result.push(0);
                }
                let mut source = part.to_vec();
                source.push(0);
                // SAFETY: source is NUL terminated; a zero-sized destination is
                // ignored by strxfrm_l. The locale outlives both calls.
                let size = unsafe {
                    strxfrm_l(
                        std::ptr::null_mut(),
                        source.as_ptr().cast(),
                        0,
                        self.0.as_ptr(),
                    )
                };
                let Some(capacity) = size.checked_add(1) else {
                    return Vec::new();
                };
                let mut key = vec![0u8; capacity];
                // SAFETY: key has capacity bytes, exactly as passed; source and
                // the immutable native locale are valid for the entire call.
                let written = unsafe {
                    strxfrm_l(
                        key.as_mut_ptr().cast(),
                        source.as_ptr().cast(),
                        capacity,
                        self.0.as_ptr(),
                    )
                };
                if written >= capacity {
                    return Vec::new();
                }
                key.truncate(written);
                result.extend(key);
            }
            result
        }
    }

    impl Drop for Locale {
        fn drop(&mut self) {
            // SAFETY: This is the unique final owner of a locale returned by
            // newlocale; it has not been freed or used as a mutable base.
            unsafe { freelocale(self.0.as_ptr()) };
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use std::ffi::{c_char, c_int};

    pub(super) const LOWERCASE: u32 = 0x100;
    pub(super) const UPPERCASE: u32 = 0x200;
    const SORTKEY: u32 = 0x400;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(super) fn GetUserDefaultLCID() -> u32;
        pub(super) fn LCMapStringA(
            locale: u32,
            flags: u32,
            source: *const c_char,
            source_size: c_int,
            destination: *mut c_char,
            destination_size: c_int,
        ) -> c_int;
        pub(super) fn GetStringTypeExA(
            locale: u32,
            info: u32,
            source: *const c_char,
            size: c_int,
            types: *mut u16,
        ) -> c_int;
    }

    pub(super) fn default_lcid() -> u32 {
        // SAFETY: GetUserDefaultLCID has no preconditions or pointer arguments.
        unsafe { GetUserDefaultLCID() }
    }

    pub(super) fn transform(lcid: u32, bytes: &[u8]) -> Vec<u8> {
        let Ok(length) = c_int::try_from(bytes.len()) else {
            return bytes.to_vec();
        };
        // SAFETY: The explicit source range is initialized; a null destination
        // requests only the size. Empty input is allowed to fail, as in C++.
        let size = unsafe {
            LCMapStringA(
                lcid,
                SORTKEY,
                bytes.as_ptr().cast(),
                length,
                std::ptr::null_mut(),
                0,
            )
        };
        if size <= 0 {
            return bytes.to_vec();
        }
        let mut output = vec![0u8; usize::try_from(size).expect("positive native buffer size") + 1];
        // SAFETY: The writable output and readable input ranges match the
        // supplied lengths. LCMapStringA writes no more than output.len().
        let written = unsafe {
            LCMapStringA(
                lcid,
                SORTKEY,
                bytes.as_ptr().cast(),
                length,
                output.as_mut_ptr().cast(),
                size + 1,
            )
        };
        if usize::try_from(written).unwrap_or(usize::MAX) > output.len() {
            return bytes.to_vec();
        }
        while output.last() == Some(&0) {
            output.pop();
        }
        output
    }
}

const COLLATING_NAMES: &[&[u8]] = &[
    b"NUL",
    b"SOH",
    b"STX",
    b"ETX",
    b"EOT",
    b"ENQ",
    b"ACK",
    b"alert",
    b"backspace",
    b"tab",
    b"newline",
    b"vertical-tab",
    b"form-feed",
    b"carriage-return",
    b"SO",
    b"SI",
    b"DLE",
    b"DC1",
    b"DC2",
    b"DC3",
    b"DC4",
    b"NAK",
    b"SYN",
    b"ETB",
    b"CAN",
    b"EM",
    b"SUB",
    b"ESC",
    b"IS4",
    b"IS3",
    b"IS2",
    b"IS1",
    b"space",
    b"exclamation-mark",
    b"quotation-mark",
    b"number-sign",
    b"dollar-sign",
    b"percent-sign",
    b"ampersand",
    b"apostrophe",
    b"left-parenthesis",
    b"right-parenthesis",
    b"asterisk",
    b"plus-sign",
    b"comma",
    b"hyphen",
    b"period",
    b"slash",
    b"zero",
    b"one",
    b"two",
    b"three",
    b"four",
    b"five",
    b"six",
    b"seven",
    b"eight",
    b"nine",
    b"colon",
    b"semicolon",
    b"less-than-sign",
    b"equals-sign",
    b"greater-than-sign",
    b"question-mark",
    b"commercial-at",
    b"A",
    b"B",
    b"C",
    b"D",
    b"E",
    b"F",
    b"G",
    b"H",
    b"I",
    b"J",
    b"K",
    b"L",
    b"M",
    b"N",
    b"O",
    b"P",
    b"Q",
    b"R",
    b"S",
    b"T",
    b"U",
    b"V",
    b"W",
    b"X",
    b"Y",
    b"Z",
    b"left-square-bracket",
    b"backslash",
    b"right-square-bracket",
    b"circumflex",
    b"underscore",
    b"grave-accent",
    b"a",
    b"b",
    b"c",
    b"d",
    b"e",
    b"f",
    b"g",
    b"h",
    b"i",
    b"j",
    b"k",
    b"l",
    b"m",
    b"n",
    b"o",
    b"p",
    b"q",
    b"r",
    b"s",
    b"t",
    b"u",
    b"v",
    b"w",
    b"x",
    b"y",
    b"z",
    b"left-curly-bracket",
    b"vertical-line",
    b"right-curly-bracket",
    b"tilde",
    b"DEL",
];
const MULTI_COLLATING_NAMES: &[&[u8]] = &[
    b"ae", b"Ae", b"AE", b"ch", b"Ch", b"CH", b"ll", b"Ll", b"LL", b"ss", b"Ss", b"SS", b"nj",
    b"Nj", b"NJ", b"dz", b"Dz", b"DZ", b"lj", b"Lj", b"LJ",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn native_numeric_boundary_is_explicit_and_does_not_panic() {
        let rules = CharacterRules::default();
        let bytes = b"9223372036854775808";
        let mut position = 0;
        assert_eq!(rules.toi(bytes, &mut position, bytes.len(), 10), i64::MIN);
        assert_eq!(position, bytes.len());
        let regex = crate::Regex::new(b"a").unwrap();
        assert_eq!(regex.replace(b"a", b"$9223372036854775808").unwrap(), b"a");
    }

    #[test]
    fn retained_classes_and_collating_names() {
        let rules = CharacterRules::default();
        assert_eq!(rules.is_class(b'_', b"word"), Some(true));
        assert_eq!(rules.is_class(b'0', b"DIGIT"), Some(true));
        assert_eq!(rules.is_class(0xff, b"unicode"), Some(false));
        assert_eq!(rules.is_class(b'a', b"unknown"), None);
        assert_eq!(rules.collating_name(b"NUL"), Some(vec![0]));
        assert_eq!(rules.collating_name(b"DEL"), Some(vec![127]));
        assert_eq!(rules.collating_name(b"ch"), Some(b"ch".to_vec()));
        assert_eq!(rules.collating_name(b"\xff"), Some(vec![255]));
    }

    #[cfg(unix)]
    #[test]
    fn classic_rules_keep_byte_semantics() {
        let rules = CharacterRules::default();
        assert_eq!(rules.is_class(b'\x0b', b"blank"), Some(true));
        assert_eq!(rules.is_class(b'\x0b', b"h"), Some(false));
        assert_eq!(rules.is_class(b'\x0c', b"blank"), Some(false));
        assert_eq!(
            rules.transform(&[0, b'A', 255]),
            [1, b'a', b'B', b'a', 255, b'b']
        );
        assert_eq!(rules.transform_primary(b"A"), b"a");
        for byte in 128..=255 {
            assert_eq!(rules.lower(byte), byte);
            assert_eq!(rules.is_class(byte, b"alpha"), Some(false));
        }
    }

    #[cfg(unix)]
    #[test]
    fn named_locale_does_not_mutate_other_rules() {
        let rules = CharacterRules::with_locale("C").unwrap();
        assert_eq!(rules.lower(b'A'), b'a');
        assert!(CharacterRules::with_locale("nonexistent_boost_regex_locale").is_err());
        assert!(CharacterRules::with_locale("C\0UTF8").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn named_libcpp_locale_keeps_classic_classes_and_native_case_maps() {
        let rules = CharacterRules::with_locale("en_US.UTF-8").unwrap();
        let classic = CharacterRules::default();
        let classes: [&[u8]; 21] = [
            b"alnum", b"alpha", b"blank", b"cntrl", b"d", b"digit", b"graph", b"h", b"l", b"lower",
            b"print", b"punct", b"s", b"space", b"u", b"upper", b"unicode", b"v", b"w", b"word",
            b"xdigit",
        ];
        for byte in u8::MIN..=u8::MAX {
            for class in classes {
                assert_eq!(
                    rules.is_class(byte, class),
                    classic.is_class(byte, class),
                    "byte {byte:#04x}, class {class:?}"
                );
            }
        }
        // Native Darwin case conversion still maps Latin-1 code points even
        // though the C++ char facet does not classify their individual bytes.
        assert_eq!(rules.lower(0xc0), 0xe0);
        assert_eq!(rules.upper(0xe0), 0xc0);
        assert_eq!(classic.lower(0xc0), 0xc0);
        assert_eq!(classic.upper(0xe0), 0xe0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn named_libcpp_word_boundaries_ignore_high_bytes() {
        let rules = CharacterRules::with_locale("en_US.UTF-8").unwrap();
        let input = b"\xaaA\xe0_ \xa0";
        for (pattern, positions) in [
            (&b"\\b"[..], &b"\x01\x02\x03\x04"[..]),
            (&b"\\B"[..], &b"\x00\x05\x06"[..]),
            (&b"\\<"[..], &b"\x01\x03"[..]),
            (&b"\\>"[..], &b"\x02\x04"[..]),
        ] {
            let regex = crate::Regex::with_character_rules(
                pattern,
                crate::Limits::default(),
                rules.clone(),
            )
            .unwrap();
            let spans: Vec<_> = regex
                .find_all(input)
                .unwrap()
                .iter()
                .map(crate::Match::span)
                .collect();
            let expected: Vec<_> = positions
                .iter()
                .map(|&position| usize::from(position)..usize::from(position))
                .collect();
            assert_eq!(spans, expected, "pattern {pattern:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn stream_numeric_extraction_preserves_consumption() {
        let rules = CharacterRules::default();
        for (bytes, radix, expected, consumed) in [
            (&b"  +42z"[..], 10, 42, 5),
            (&b"-42z"[..], 10, -42, 3),
            (&b"0xffz"[..], 16, 255, 4),
            (&b"0x"[..], 16, -1, 0),
            (&b"9999999999999999999999"[..], 10, -1, 0),
        ] {
            let mut position = 0;
            assert_eq!(
                rules.toi(bytes, &mut position, bytes.len(), radix),
                expected
            );
            assert_eq!(position, consumed);
        }
    }
}
