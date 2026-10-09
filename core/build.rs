//! Use the real action table when the player has generated it from their own
//! copy of Elden Ring; otherwise build against the stand-in.

use std::path::Path;

const GENERATED: &str = "src/sim/extracted.rs";

fn main() {
    println!("cargo:rerun-if-changed={GENERATED}");
    println!("cargo::rustc-check-cfg=cfg(er_data)");
    if Path::new(GENERATED).exists() {
        println!("cargo:rustc-cfg=er_data");
    } else {
        println!("cargo:warning=No {GENERATED}: building with the stand-in action table. See README.md to generate the real one.");
    }
}
