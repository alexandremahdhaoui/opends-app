use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PAYLOAD_FILES: &[&str] = &[
    "opends-uhid.dll",
    "opends-uhid.inf",
    "opends-uhid.cat",
    "opends.cer",
    "OpenDS.exe",
];

fn main() {
    println!("cargo:rerun-if-changed=setup.rc");
    println!("cargo:rerun-if-changed=setup.manifest");
    println!("cargo:rerun-if-env-changed=OPENDS_PAYLOAD_DIR");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    write_payload(&out);
    embed_manifest(&out);
}

fn write_payload(out: &Path) {
    let dir = env::var("OPENDS_PAYLOAD_DIR").ok().map(PathBuf::from);

    let mut source = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");

    if let Some(dir) = dir.as_ref() {
        for leaf in PAYLOAD_FILES {
            let path = dir.join(leaf);

            println!("cargo:rerun-if-changed={}", path.display());

            if !path.exists() {
                println!("cargo:warning=payload file {} is missing", path.display());
                continue;
            }

            source.push_str(&format!(
                "    ({:?}, include_bytes!({:?})),\n",
                leaf,
                path.display().to_string()
            ));
        }
    }

    source.push_str("];\n");

    fs::write(out.join("payload.rs"), source).unwrap();
}

fn embed_manifest(out: &Path) {
    if env::var("OPENDS_NO_MANIFEST").is_ok() {
        println!("cargo:warning=manifest skipped by OPENDS_NO_MANIFEST");
        return;
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        return;
    }

    let object = out.join("setup.o");

    let compiled = Command::new("x86_64-w64-mingw32-windres")
        .args(["setup.rc", "-O", "coff", "-o"])
        .arg(&object)
        .status();

    match compiled {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bin=OpenDS-Setup={}", object.display());
        }
        _ => println!("cargo:warning=windres missing, OpenDS-Setup will not self elevate"),
    }
}
