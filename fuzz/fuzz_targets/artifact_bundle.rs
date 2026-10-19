#![no_main]

use libfuzzer_sys::fuzz_target;
use stackpit::fuzz_api::{decode_sourcemap, parse_artifact_bundle_sync};
use stackpit::ingest::sourcemap::resolve_frame;

fuzz_target!(|input: (&[u8], u32, u32)| {
    let (bytes, line, col) = input;
    if let Ok(entries) = parse_artifact_bundle_sync(bytes) {
        for entry in entries {
            assert!(!entry.debug_id.is_empty());
            if let Ok(sm) = decode_sourcemap(&entry.data) {
                let _ = resolve_frame(&sm, line, col);
            }
        }
    }
    // Stored sourcemaps are zstd blobs; a raw one must fail cleanly, not panic.
    if let Ok(sm) = decode_sourcemap(bytes) {
        let _ = resolve_frame(&sm, line, col);
        let _ = resolve_frame(&sm, 0, 0);
    }
});
