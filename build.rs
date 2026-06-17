use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/boot.S");

    let target = std::env::var("TARGET").unwrap();
    if target != "x86_64-unknown-none" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();

    // 用 as --64 生成 64-bit ELF 对象文件（与 Rust 目标格式一致）
    let status = Command::new("as")
        .args(["--64", "src/boot.S", "-o", &format!("{}/boot.o", out_dir)])
        .status()
        .expect("Failed to run `as`");

    assert!(status.success(), "Assembly failed");

    // 将 boot.o 直接传给链接器
    println!("cargo:rustc-link-arg={}/boot.o", out_dir);
}
