//! LZO1X decoder tests.
//!
//! The vectors were produced with the reference LZO implementation
//! (`lzo1x_1_compress`) so the decoder is checked against the real format,
//! including the stream-finaliser behaviour that differs between the
//! minilzo reference and the full library.

use trackmaniare::lzo;

fn decompress(bytes: &[u8]) -> (Vec<u8>, bool) {
    let out = lzo::decompress(bytes).expect("decompress");
    (out.data, out.complete)
}

/// "0123456789" * 3 + "56789", compressed with the reference LZO1X-1
/// compressor.
const VECTOR_1: &[u8] = &[
    0x07, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x2a, 0x24, 0x00, 0x0a, 0x32,
    0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x35, 0x36, 0x37, 0x38, 0x39, 0x11, 0x00, 0x00,
];
const VECTOR_1_PLAIN: &[u8] = b"01234567890123456789012345678956789";

#[test]
fn decompresses_literals_and_matches() {
    let (out, complete) = decompress(VECTOR_1);
    assert_eq!(out, VECTOR_1_PLAIN);
    assert!(complete, "stream should end on the EOF marker");
}

/// 40 literal bytes only (first instruction is a long literal run).
const VECTOR_2: &[u8] = &[
    0x39, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
    0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
    0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x11, 0x00, 0x00,
];

#[test]
fn decompresses_long_literal_run() {
    let (out, complete) = decompress(VECTOR_2);
    assert_eq!(out.len(), 40);
    assert_eq!(out[0], 0x00);
    assert_eq!(out[39], 0x27);
    assert!(complete);
}

#[test]
fn rejects_empty_input() {
    assert!(lzo::decompress(&[]).is_err());
}

#[test]
fn truncated_stream_returns_partial_data() {
    let cut = &VECTOR_1[..VECTOR_1.len() - 4];
    let out = lzo::decompress(cut).expect("partial data is still returned");
    assert!(!out.complete);
}
