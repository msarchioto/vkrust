use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let link = manifest.join("link.ld");
    println!("cargo:rerun-if-changed={}", link.display());
    println!("cargo:rustc-link-arg=-T{}", link.display());

    let elf = std::env::var("USER_ELF").unwrap_or_else(|_| {
        manifest
            .join("../user/target/x86_64-unknown-none/release/shell")
            .display()
            .to_string()
    });
    let elf_path = PathBuf::from(&elf);
    if !elf_path.is_file() {
        panic!(
            "userspace shell ELF not found at {elf}. Build it first with `make user`."
        );
    }
    println!("cargo:rerun-if-env-changed=USER_ELF");
    println!("cargo:rerun-if-changed={elf}");
    println!("cargo:rustc-env=USER_ELF={elf}");
}
