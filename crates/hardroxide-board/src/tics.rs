//! Register files exported by TI's TICS Pro: one 24-bit write per line, such as `R0 (INIT)\t0x000090`, in the order to
//! send them.

/// A line that is not blank and holds no 24-bit hexadecimal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("line {line} of the register file has no 24-bit value")]
pub struct BadLine {
    /// Counted from 1.
    pub line: usize,
}

/// The values of `text`, in order.
pub fn registers(text: &str) -> impl Iterator<Item = Result<u32, BadLine>> + '_ {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            l.split_whitespace()
                .last()
                .and_then(|w| w.strip_prefix("0x"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .filter(|&v| v <= 0xFF_FFFF)
                .ok_or(BadLine { line: i + 1 })
        })
}

/// Collect the values of `text` into `buffer`, returning the part used; fails on a bad line, or one past the buffer.
pub fn parse<'a>(text: &str, buffer: &'a mut [u32]) -> Result<&'a [u32], BadLine> {
    let mut n = 0;
    for value in registers(text) {
        *buffer.get_mut(n).ok_or(BadLine { line: n + 1 })? = value?;
        n += 1;
    }
    Ok(&buffer[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_give_their_last_value() {
        let text = "R0 (INIT)\t0x000090\r\nR0\t0x000010\n\nR2\t0x000200\n";
        let mut buffer = [0; 4];
        assert_eq!(parse(text, &mut buffer).unwrap(), [0x90, 0x10, 0x200]);
    }

    #[test]
    fn bad_and_surplus_lines_are_named() {
        let mut buffer = [0; 1];
        assert_eq!(
            parse("R0\t0x1000000", &mut buffer),
            Err(BadLine { line: 1 })
        );
        assert_eq!(parse("hello", &mut buffer), Err(BadLine { line: 1 }));
        assert_eq!(parse("a 0x1\nb 0x2", &mut buffer), Err(BadLine { line: 2 }));
    }
}
