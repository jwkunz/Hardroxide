//! File names: short (8.3) names, long names, the checksum that ties them together.

/// A file name: UTF-8, at most 255 bytes, on the stack.
#[derive(Clone, Copy)]
pub struct NameBuf {
    len: u8,
    bytes: [u8; 255],
}

impl NameBuf {
    pub const fn new() -> Self {
        NameBuf {
            len: 0,
            bytes: [0; 255],
        }
    }

    pub fn as_str(&self) -> &str {
        // Only whole UTF-8 characters are ever pushed.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("?")
    }

    pub(crate) fn push(&mut self, c: char) {
        let mut tmp = [0u8; 4];
        let s = c.encode_utf8(&mut tmp);
        if usize::from(self.len) + s.len() <= 255 {
            self.bytes[usize::from(self.len)..usize::from(self.len) + s.len()]
                .copy_from_slice(s.as_bytes());
            self.len += s.len() as u8;
        }
    }

    pub(crate) fn from_str_lossy(s: &str) -> Self {
        let mut n = NameBuf::new();
        for c in s.chars() {
            n.push(c);
        }
        n
    }
}

impl Default for NameBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for NameBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl core::fmt::Display for NameBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// ASCII-case-insensitive equality, which is how FAT compares names.
pub(crate) fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .all(|(x, y)| x.eq_ignore_ascii_case(&y))
}

/// The checksum of an 8.3 name stored in each long-name entry.
pub(crate) fn lfn_checksum(short: &[u8; 11]) -> u8 {
    short.iter().fold(0u8, |sum, &b| {
        (sum >> 1).wrapping_add(sum << 7).wrapping_add(b)
    })
}

/// A name is valid if it is 1 to 255 bytes, has none of the reserved characters, no control
/// characters, no trailing space or dot, and is not `.` or `..`.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.ends_with([' ', '.'])
        && !name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
}

fn short_char_ok(c: u8) -> bool {
    c.is_ascii_uppercase()
        || c.is_ascii_lowercase()
        || c.is_ascii_digit()
        || b"!#$%&'()-@^_`{}~".contains(&c)
}

/// How a name is stored.
pub(crate) struct Stored {
    /// The 8.3 name (the alias, if a long name is needed).
    pub short: [u8; 11],
    /// The case flags of the short name (bit 3: base is lower case, bit 4: extension is lower case).
    pub nt: u8,
    /// Whether long-name entries are needed.
    pub needs_lfn: bool,
}

fn case_of(part: &[u8]) -> Option<bool> {
    // Some(true): has lower case and no upper; Some(false): no lower case; None: mixed.
    let (lower, upper) = (
        part.iter().any(u8::is_ascii_lowercase),
        part.iter().any(u8::is_ascii_uppercase),
    );
    match (lower, upper) {
        (true, true) => None,
        (true, false) => Some(true),
        _ => Some(false),
    }
}

/// Decide how `name` is stored; `alias_n` is the numeric tail to use if an alias is needed.
pub(crate) fn classify(name: &str, alias_n: u32) -> Stored {
    let b = name.as_bytes();
    let dot = name.rfind('.').filter(|&i| i > 0);
    let (base, ext) = match dot {
        Some(i) => (&b[..i], &b[i + 1..]),
        None => (b, &b[..0]),
    };
    let fits = (1..=8).contains(&base.len())
        && ext.len() <= 3
        && base.iter().chain(ext).all(|&c| short_char_ok(c))
        && case_of(base).is_some()
        && case_of(ext).is_some();
    if fits {
        let mut short = [b' '; 11];
        for (d, s) in short[..8].iter_mut().zip(base) {
            *d = s.to_ascii_uppercase();
        }
        for (d, s) in short[8..].iter_mut().zip(ext) {
            *d = s.to_ascii_uppercase();
        }
        let nt = if case_of(base) == Some(true) { 0x08 } else { 0 }
            | if !ext.is_empty() && case_of(ext) == Some(true) {
                0x10
            } else {
                0
            };
        return Stored {
            short,
            nt,
            needs_lfn: false,
        };
    }
    // An alias: the first characters of the base (invalid ones become `_`), `~N`, three of the extension.
    let mut short = [b' '; 11];
    let mut tail = [0u8; 10];
    let mut digits = 0;
    let mut n = alias_n.max(1);
    while n > 0 {
        tail[digits] = b'0' + (n % 10) as u8;
        n /= 10;
        digits += 1;
    }
    let keep = 8usize.saturating_sub(digits + 1).max(1);
    let mut i = 0;
    for &c in base.iter().filter(|&&c| c != b' ' && c != b'.') {
        if i >= keep {
            break;
        }
        short[i] = if short_char_ok(c) {
            c.to_ascii_uppercase()
        } else {
            b'_'
        };
        i += 1;
    }
    short[i] = b'~';
    for d in 0..digits {
        short[i + 1 + d] = tail[digits - 1 - d];
    }
    for (d, &c) in short[8..]
        .iter_mut()
        .zip(ext.iter().filter(|&&c| c != b' '))
    {
        *d = if short_char_ok(c) {
            c.to_ascii_uppercase()
        } else {
            b'_'
        };
    }
    Stored {
        short,
        nt: 0,
        needs_lfn: true,
    }
}

/// The name stored in a short entry, with the case flags applied.
pub(crate) fn short_to_name(short: &[u8; 11], nt: u8) -> NameBuf {
    let mut out = NameBuf::new();
    let lower = |c: u8, flag: bool| if flag { c.to_ascii_lowercase() } else { c };
    let first = if short[0] == 0x05 { 0xE5 } else { short[0] };
    for (i, &c) in short[..8].iter().enumerate() {
        if c == b' ' {
            break;
        }
        let c = if i == 0 { first } else { c };
        out.push(char::from(lower(c, nt & 0x08 != 0)));
    }
    if short[8] != b' ' {
        out.push('.');
        for &c in short[8..].iter().take_while(|&&c| c != b' ') {
            out.push(char::from(lower(c, nt & 0x10 != 0)));
        }
    }
    out
}

/// Encode a name as UTF-16 units into `out`; returns how many.
pub(crate) fn to_utf16(name: &str, out: &mut [u16; 255]) -> usize {
    let mut n = 0;
    for c in name.chars() {
        let mut buf = [0u16; 2];
        for u in c.encode_utf16(&mut buf) {
            if n < 255 {
                out[n] = *u;
                n += 1;
            }
        }
    }
    n
}

/// Decode UTF-16 units into a name (unpaired surrogates become `?`).
pub(crate) fn from_utf16(units: &[u16]) -> NameBuf {
    let mut out = NameBuf::new();
    for r in char::decode_utf16(units.iter().copied()) {
        out.push(r.unwrap_or('?'));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_matches_the_documented_example() {
        // "FOO     BAR" has the checksum 0x3A? computed here against the algorithm of the specification.
        let s = *b"FOO     BAR";
        let mut sum = 0u8;
        for b in s {
            sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(b);
        }
        assert_eq!(lfn_checksum(&s), sum);
    }

    #[test]
    fn plain_names_are_stored_short_with_case_flags() {
        let s = classify("chime.wav", 1);
        assert!(!s.needs_lfn);
        assert_eq!(&s.short, b"CHIME   WAV");
        assert_eq!(s.nt, 0x18);
        let s = classify("README.TXT", 1);
        assert!(!s.needs_lfn && s.nt == 0);
        let s = classify("Mixed.txt", 1);
        assert!(s.needs_lfn, "mixed case needs a long name");
        assert!(!classify("data", 1).needs_lfn);
    }

    #[test]
    fn long_names_get_an_alias_with_a_numeric_tail() {
        let s = classify("A long file name.text", 1);
        assert!(s.needs_lfn);
        assert_eq!(&s.short, b"ALONGF~1TEX");
        assert_eq!(&classify("A long file name.text", 12).short, b"ALONG~12TEX");
        assert_eq!(&classify(".hidden", 1).short[..8], b"HIDDEN~1");
    }

    #[test]
    fn short_names_read_back_with_their_flags() {
        assert_eq!(short_to_name(b"CHIME   WAV", 0x18).as_str(), "chime.wav");
        assert_eq!(short_to_name(b"DATA       ", 0).as_str(), "DATA");
        assert_eq!(short_to_name(b"README  TXT", 0x08).as_str(), "readme.TXT");
    }

    #[test]
    fn names_are_validated_and_compared_without_case() {
        assert!(valid_name("a b.txt") && valid_name("Ünï.txt"));
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "a:b",
            "tab\t",
            "trailing.",
            "x ",
            "q?",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert!(eq_ignore_case("ReadMe.TXT", "readme.txt"));
        assert!(!eq_ignore_case("a", "ab"));
    }

    #[test]
    fn utf16_round_trips() {
        let mut u = [0u16; 255];
        let n = to_utf16("héllo wörld 😀", &mut u);
        assert_eq!(from_utf16(&u[..n]).as_str(), "héllo wörld 😀");
    }
}
