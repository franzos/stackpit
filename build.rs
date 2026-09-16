use std::path::Path;

/// Assets the templates stamp with a version. Fonts stay out: style.css and the
/// preload links reference them by exact URL, so a suffix would need syncing in
/// two places.
const VERSIONED_ASSETS: &[&str] = &[
    "templates/style.css",
    "assets/icon.png",
    "static/bulk.js",
    "static/chart.umd.min.js",
    "static/charts.js",
    "static/confirm.js",
    "static/email-provider.js",
    "static/frames.js",
    "static/select-all.js",
    "static/stop-propagation.js",
];

/// FNV-1a. Only has to change when an asset does, so it needs no hash crate.
fn fnv1a(bytes: &[u8], mut hash: u64) -> u64 {
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // The static_loader! proc macro does not register .ftl file-content
    // deps, so edits to locales/ must force a rebuild explicitly.
    println!("cargo:rerun-if-changed={}", root.join("locales").display());

    let mut hash = 0xcbf2_9ce4_8422_2325;
    for rel in VERSIONED_ASSETS {
        let path = root.join(rel);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("reading {} for the asset version: {e}", path.display()));
        hash = fnv1a(&bytes, hash);
    }
    println!("cargo:rustc-env=STACKPIT_ASSET_VERSION={hash:016x}");
}
