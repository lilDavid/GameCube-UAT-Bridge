use std::ffi::OsStr;

use walkdir::WalkDir;

fn main() {
    println!("cargo::rerun-if-changed=src/dme_bridge.cpp");
    for entry in WalkDir::new("py-dolphin-memory-engine/Source/Common") {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.extension().and_then(OsStr::to_str) == Some("cpp") {
                    println!("cargo::rerun-if-changed={}", path.display());
                }
            }
            Err(err) => println!("cargo::warning={}", err),
        }
    }

    let dme_library = cmake::Config::new("py-dolphin-memory-engine/Source")
        .build_target("all")
        .build()
        .join("build");

    println!("cargo::rustc-link-search=native={}", dme_library.display());
    println!("cargo::rustc-link-lib=static=dolphin-memory-engine");

    cc::Build::new()
        .cpp(true)
        .include("py-dolphin-memory-engine/Source")
        .file("src/dme_bridge.cpp")
        .compile("dme_bridge");
}
