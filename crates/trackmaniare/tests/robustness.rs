//! Regression tests for hostile inputs: crafted files must be rejected or
//! skipped cleanly instead of exhausting memory or overflowing the stack.
//!
//! C1: 9999999 samples x 4095 bytes used to be allocated (40.9 GB) from a
//! ~38 KB blob because only the two factors were bounded, never the product.
//! C2: a deeply nested folder tree used to recurse once per level.

use trackmaniare::gbx::Gbx;
use trackmaniare::reader::Reader;
use trackmaniare::record;

/// A `CPlugEntRecordData` archive declaring 9,999,999 samples of 4,095 bytes,
/// zlib-compressed like a real one.
fn crafted_record_blob() -> Vec<u8> {
    let mut archive = Vec::new();
    archive.extend(0i32.to_le_bytes()); // start_ms
    archive.extend(0i32.to_le_bytes()); // end_ms
    archive.extend(0u32.to_le_bytes()); // descriptor count
    archive.extend(0u32.to_le_bytes()); // notice count
    archive.push(1); // entList: has element
    archive.extend(0i32.to_le_bytes()); // type index
    archive.extend(0i32.to_le_bytes()); // u01
    archive.extend(0i32.to_le_bytes()); // u02
    archive.extend(0i32.to_le_bytes()); // u03
    archive.extend(0i32.to_le_bytes()); // u04
    archive.extend(9_999_999i32.to_le_bytes()); // num_samples
    archive.extend(4095i32.to_le_bytes()); // sample_size
    archive.push(0); // next element flag
    archive.push(0); // secondary sample flag
    archive.push(0); // bulk notice flag
    archive.extend(0i32.to_le_bytes()); // custom module list count

    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&archive, 6);
    let mut blob = Vec::new();
    blob.extend(record::CLASS_CPLUG_ENT_RECORD_DATA.to_le_bytes());
    blob.extend(11u32.to_le_bytes()); // archive version
    blob.extend((archive.len() as u32).to_le_bytes());
    blob.extend((compressed.len() as u32).to_le_bytes());
    blob.extend(compressed);
    blob
}

#[test]
fn declared_sample_product_is_bounded_by_the_blob() {
    let blob = crafted_record_blob();
    assert!(
        blob.len() < 4096,
        "the crafted blob must stay small, got {}",
        blob.len()
    );
    let mut r = Reader::new(&blob);
    let data = record::parse_node(&mut r).expect("guarded record parses");
    assert_eq!(data.version, 11);
    assert!(
        data.lists.iter().all(|l| l.samples.is_empty()),
        "no samples may be decoded beyond the blob"
    );
}

/// A GBX header with one external node and a folder tree nested `depth` levels
/// deep. Each level costs 8 bytes of input (empty name + child count).
fn nested_folder_gbx(depth: usize) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"GBX");
    data.extend_from_slice(&6u16.to_le_bytes());
    data.push(b'B'); // format
    data.push(b'U'); // reference table compression
    data.push(b'U'); // body compression: raw, no LZO
    data.push(b'R'); // edit state
    data.extend_from_slice(&0u32.to_le_bytes()); // class id
    data.extend_from_slice(&0u32.to_le_bytes()); // user data size
    data.extend_from_slice(&0u32.to_le_bytes()); // node count
    data.extend_from_slice(&1u32.to_le_bytes()); // external node count
    data.extend_from_slice(&0u32.to_le_bytes()); // ancestor level
    data.extend_from_slice(&1u32.to_le_bytes()); // root: one child
    for level in 0..depth {
        data.extend_from_slice(&0u32.to_le_bytes()); // empty folder name
        let children: u32 = if level + 1 == depth { 0 } else { 1 };
        data.extend_from_slice(&children.to_le_bytes());
    }
    // The single external node (resource-index variant).
    data.extend_from_slice(&4u32.to_le_bytes()); // flags: no file name
    data.extend_from_slice(&0u32.to_le_bytes()); // resource index
    data.extend_from_slice(&0u32.to_le_bytes()); // node index
    data.extend_from_slice(&0u32.to_le_bytes()); // use file (v >= 5)
    data
}

#[test]
fn deeply_nested_folder_tree_does_not_overflow_the_stack() {
    const DEPTH: usize = 200_000;
    let data = nested_folder_gbx(DEPTH);
    assert!(data.len() < 2 * 1024 * 1024);
    let gbx = Gbx::parse(&data).expect("folder tree parses without recursion");
    assert_eq!(gbx.external_nodes.len(), 1);
}
