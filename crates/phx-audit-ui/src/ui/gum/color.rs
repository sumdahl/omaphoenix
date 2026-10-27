//! Colour parsing and lipgloss's truecolor → 256 downsampling.

pub fn hex(c: &str) -> Option<(u8, u8, u8)> {
    let h = c.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// `termenv`'s truecolor → 256 mapping: the nearer of the 6×6×6 cube and the
/// grey ramp (distance in RGB; `termenv` uses `HSLuv`, same picks for themes).
pub fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    let v2ci = |v: u8| match v {
        0..48 => 0u8,
        48..115 => 1,
        _ => (v - 35) / 40,
    };
    let (ri, gi, bi) = (v2ci(r), v2ci(g), v2ci(b));
    let i2cv = [0u8, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
    let cv = |i: u8| i2cv.get(usize::from(i)).copied().unwrap_or(0);
    let avg = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let gray_idx = if avg > 238 {
        23
    } else {
        avg.saturating_sub(3) / 10
    };
    let gv = 8 + 10 * gray_idx;
    let dist = |a: (u16, u16, u16)| {
        let d = |x: u16, y: u8| (i32::from(x) - i32::from(y)).pow(2);
        d(a.0, r) + d(a.1, g) + d(a.2, b)
    };
    let cube = dist((u16::from(cv(ri)), u16::from(cv(gi)), u16::from(cv(bi))));
    let grey = dist((gv, gv, gv));
    if cube <= grey {
        16 + 36 * ri + 6 * gi + bi
    } else {
        232 + u8::try_from(gray_idx).unwrap_or(23)
    }
}

/// Removes ANSI escapes, as gum does with item labels.
pub fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            for c in it.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn downsamples_like_termenv() {
        assert_eq!(super::ansi256(0xf3, 0x8d, 0x70), 209, "accent");
        assert_eq!(super::ansi256(0x2c, 0x25, 0x25), 235, "background");
        assert_eq!(super::ansi256(0xe6, 0xd9, 0xdb), 253, "foreground");
        assert_eq!(super::ansi256(0x40, 0x3e, 0x41), 238, "selected");
        assert_eq!(super::ansi256(0x62, 0x62, 0x62), 241, "help key");
        assert_eq!(super::ansi256(0x4a, 0x4a, 0x4a), 239, "help text");
        assert_eq!(super::ansi256(0x3c, 0x3c, 0x3c), 237, "help separator");
    }
}
