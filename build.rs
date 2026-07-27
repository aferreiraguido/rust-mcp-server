// build.rs
// Lee version.txt y expone la versión como constante en OUT_DIR.
// Equivalente al filtrado Maven de version.properties de los MCP Java/Cobol.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=version.txt");
    println!("cargo:rerun-if-changed=build.rs");

    let version = fs::read_to_string("version.txt")
        .expect("version.txt not found in project root")
        .trim()
        .to_string();

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set by cargo");
    let dest = Path::new(&out_dir).join("version.rs");
    fs::write(
        &dest,
        format!("pub const VERSION: &str = \"{}\";\n", version),
    )
    .expect("failed to write OUT_DIR/version.rs");
}
