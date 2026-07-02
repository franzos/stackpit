use std::path::Path;

fn main() {
    // The static_loader! proc macro does not register .ftl file-content
    // deps, so edits to locales/ must force a rebuild explicitly.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed={}", root.join("locales").display());
}
