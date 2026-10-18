//! LZO1X decompression (the compression Nadeo uses for the GameBox body).
//!
//! Ported from the reference `lzo1x_decompress` in the LZO library
//! (https://www.oberhumer.com/opensource/lzo). The decoder stops on the
//! end-of-stream marker (an M4 match with offset 0) or when the input runs
//! out, which lets us salvage partial data out of truncated replays.

use crate::error::{Error, Result};

const M2_MAX_OFFSET: usize = 0x0800;

/// Decompression result. `complete` is false when the compressed input ran out
/// before the end-of-stream marker was reached (truncated file).
pub struct Decoded {
    pub data: Vec<u8>,
    pub complete: bool,
}

pub fn decompress(src: &[u8]) -> Result<Decoded> {
    if src.is_empty() {
        return Err(Error::Lzo("empty input".into()));
    }

    #[allow(unused_assignments)]
    let mut ip = 0usize;
    let mut out: Vec<u8> = Vec::with_capacity(src.len() * 2);

    // Copies a literal run, bailing out (gracefully) if the input is exhausted.
    macro_rules! literals {
        ($n:expr) => {{
            let n = $n;
            if ip + n > src.len() {
                let take = src.len().saturating_sub(ip);
                out.extend_from_slice(&src[ip..ip + take]);
                return Ok(Decoded {
                    data: out,
                    complete: false,
                });
            }
            out.extend_from_slice(&src[ip..ip + n]);
            ip += n;
        }};
    }

    // Copies a match, byte by byte, so overlapping copies work.
    macro_rules! copy_match {
        ($pos:expr, $len:expr) => {{
            let pos: isize = $pos;
            let len: usize = $len;
            if pos < 0 {
                return Err(Error::Lzo(format!(
                    "invalid match: distance {} with output length {} at input offset {}",
                    -pos,
                    out.len(),
                    ip
                )));
            }
            let mut p = pos as usize;
            for _ in 0..len {
                out.push(out[p]);
                p += 1;
            }
        }};
    }

    // Extended length: a run of 0x00 bytes each adding 255, then a final byte.
    macro_rules! ext_len {
        ($t:expr, $base:expr) => {{
            let mut t: usize = $t;
            loop {
                if ip >= src.len() {
                    return Ok(Decoded {
                        data: out,
                        complete: false,
                    });
                }
                if src[ip] == 0 {
                    t = t.wrapping_add(255);
                    ip += 1;
                } else {
                    break;
                }
            }
            t = t.wrapping_add($base).wrapping_add(src[ip] as usize);
            ip += 1;
            t
        }};
    }

    // Tail literals + the next token, used at several places in the format.
    macro_rules! tail_and_token {
        () => {{
            let trailing = (src[ip - 2] & 3) as usize;
            literals!(trailing);
            if ip >= src.len() {
                return Ok(Decoded {
                    data: out,
                    complete: false,
                });
            }
            let token = src[ip] as usize;
            ip += 1;
            token
        }};
    }

    let mut token: usize;

    if src[0] > 17 {
        ip = 1;
        token = src[0] as usize - 17;
        if token < 4 {
            // Short literal run, directly followed by a token.
            literals!(token);
            if ip >= src.len() {
                return Ok(Decoded {
                    data: out,
                    complete: false,
                });
            }
            token = src[ip] as usize;
            ip += 1;
        } else {
            literals!(token);
            // first_literal_run
            if ip >= src.len() {
                return Ok(Decoded {
                    data: out,
                    complete: false,
                });
            }
            token = src[ip] as usize;
            ip += 1;
            if token < 16 {
                if ip >= src.len() {
                    return Ok(Decoded {
                        data: out,
                        complete: false,
                    });
                }
                let hi = src[ip] as usize;
                let dist = (1 + M2_MAX_OFFSET + (token >> 2) + (hi << 2)) as isize;
                ip += 1;
                copy_match!(out.len() as isize - dist, 3);
                token = tail_and_token!();
            }
        }
    } else {
        token = src[0] as usize;
        ip = 1;
    }

    loop {
        if token >= 16 {
            // ---- match ----
            if token >= 64 {
                // M2: distance 1..2048, length 3..9 (or extended)
                if ip >= src.len() {
                    return Ok(Decoded {
                        data: out,
                        complete: false,
                    });
                }
                let dist = 1 + ((token >> 2) & 7) + ((src[ip] as usize) << 3);
                ip += 1;
                let mut len = (token >> 5) - 1;
                if len == 0 {
                    len = ext_len!(0usize, 0usize);
                }
                copy_match!(out.len() as isize - dist as isize, len + 2);
            } else if token >= 32 {
                // M3: distance 1..16384
                let mut len = token & 31;
                if len == 0 {
                    len = ext_len!(0usize, 31usize);
                }
                if ip + 2 > src.len() {
                    return Ok(Decoded {
                        data: out,
                        complete: false,
                    });
                }
                let raw = u16::from_le_bytes([src[ip], src[ip + 1]]) as usize;
                ip += 2;
                let dist = 1 + (raw >> 2);
                copy_match!(out.len() as isize - dist as isize, len + 2);
            } else {
                // M4: distance 1..49151, offset 0 means end of stream
                let mut len = token & 7;
                if len == 0 {
                    len = ext_len!(0usize, 7usize);
                }
                if ip + 2 > src.len() {
                    return Ok(Decoded {
                        data: out,
                        complete: false,
                    });
                }
                let raw = u16::from_le_bytes([src[ip], src[ip + 1]]) as usize;
                ip += 2;
                let dist_raw = ((token & 8) << 11) + (raw >> 2);
                if dist_raw == 0 {
                    // End of stream marker.
                    return Ok(Decoded {
                        data: out,
                        complete: true,
                    });
                }
                // The reference implementation subtracts 0x4000 from the match
                // position (not from the distance).
                let pos = out.len() as isize - dist_raw as isize - 0x4000;
                copy_match!(pos, len + 2);
            }

            token = tail_and_token!();
        } else {
            // ---- literal run ----
            let mut len = token;
            if len == 0 {
                len = ext_len!(0usize, 15usize);
            }
            literals!(len + 3);

            // first_literal_run
            if ip >= src.len() {
                return Ok(Decoded {
                    data: out,
                    complete: false,
                });
            }
            token = src[ip] as usize;
            ip += 1;
            if token >= 16 {
                continue;
            }
            if ip >= src.len() {
                return Ok(Decoded {
                    data: out,
                    complete: false,
                });
            }
            let hi = src[ip] as usize;
            let dist = (1 + M2_MAX_OFFSET + (token >> 2) + (hi << 2)) as isize;
            ip += 1;
            copy_match!(out.len() as isize - dist, 3);
            token = tail_and_token!();
        }
    }
}
