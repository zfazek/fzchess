//! Pure utility functions ported from the C++ `Util` class.
//!
//! The move encoding is a faithful port of `src/Util.cpp` so that move integers
//! are bit-for-bit compatible with the C++ engine. This matters because the
//! golden regression baseline compares coordinate-notation moves (e.g. "e2e4"),
//! and keeping the internal encoding identical makes cross-checking the two
//! engines straightforward during the port.
//!
//! Move encoding (16-bit int), matching the C++ layout:
//!   bits for `from` square: x_from*32 + y_from*4, placed in the high byte
//!   bits for `to`   square: x_to*32   + y_to*4,   placed in the low byte
//!   promotion flags are OR'd into the low bits of each byte:
//!     high byte: +2 = queen, +1 = rook
//!     low  byte: +2 = bishop, +1 = knight

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch (matches C++ `Util::get_ms`).
pub fn get_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as u64
}

/// Parse a coordinate-notation move ("e2e4", "e7e8q") into the engine's packed
/// integer encoding. Faithful port of C++ `Util::str2move`.
///
/// Accepts a 4- or 5-char move. The 5th char (promotion piece) is optional.
pub fn str2move(move_old: &str) -> i32 {
    let b = move_old.as_bytes();
    let x_from = (b[0] - b'a') as i32;
    let y_from = (b[1] - b'1') as i32;
    let x_to = (b[2] - b'a') as i32;
    let y_to = (b[3] - b'1') as i32;
    let mut move_hi = x_from * 32 + y_from * 4;
    let mut move_lo = x_to * 32 + y_to * 4;
    // Promotion piece, if present.
    if b.len() > 4 {
        match b[4] {
            b'q' | b'Q' => move_hi += 2,
            b'r' | b'R' => move_hi += 1,
            b'b' | b'B' => move_lo += 2,
            b'n' | b'N' => move_lo += 1,
            _ => {}
        }
    }
    move_hi * 256 + move_lo
}

/// Render a packed move integer as coordinate notation. Faithful port of C++
/// `Util::move2str`. Normal moves are 4 chars ("e2e4"), promotions 5 ("e7e8q").
///
/// Unlike the C++ version (which returns a pointer to a shared static buffer),
/// this returns an owned `String`, which is the idiomatic and thread-safe form.
pub fn move2str(mv: i32) -> String {
    let mut s = String::with_capacity(5);
    s.push((((mv & 0xe000) / 256 / 32) as u8 + b'a') as char);
    s.push((((mv & 0x1c00) / 256 / 4) as u8 + b'1') as char);
    s.push((((mv & 0x00e0) % 256 / 32) as u8 + b'a') as char);
    s.push((((mv & 0x001c) % 256 / 4) as u8 + b'1') as char);
    if (mv & 0x0303) > 0 {
        if (mv & 0x0200) == 0x0200 {
            s.push('q');
        } else if (mv & 0x0100) == 0x0100 {
            s.push('r');
        } else if (mv & 0x0002) == 0x0002 {
            s.push('b');
        } else if (mv & 0x0001) == 0x0001 {
            s.push('n');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_normal_moves() {
        for mv in ["e2e4", "e7e5", "g1f3", "a1h8", "h2h4", "e1g1"] {
            let encoded = str2move(mv);
            assert_eq!(move2str(encoded), mv, "round trip failed for {mv}");
        }
    }

    #[test]
    fn roundtrip_promotions() {
        // Lowercase promotion chars round-trip exactly.
        for mv in ["e7e8q", "a7a8r", "b7b8b", "c7c8n"] {
            let encoded = str2move(mv);
            assert_eq!(move2str(encoded), mv, "round trip failed for {mv}");
        }
    }

    #[test]
    fn uppercase_promotion_parses_same_as_lowercase() {
        assert_eq!(str2move("e7e8Q"), str2move("e7e8q"));
        assert_eq!(str2move("a7a8N"), str2move("a7a8n"));
    }

    #[test]
    fn known_encoding_values() {
        // Cross-checked against the C++ bit layout:
        // e2e4: from e2, to e4.
        //   x_from=4,y_from=1 -> hi = 4*32 + 1*4 = 132
        //   x_to=4,  y_to=3   -> lo = 4*32 + 3*4 = 140
        //   move = 132*256 + 140 = 33932
        assert_eq!(str2move("e2e4"), 33932);
    }
}
