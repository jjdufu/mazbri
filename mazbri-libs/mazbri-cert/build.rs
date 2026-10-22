use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let default = manifest_dir.join("../../mazbri-license.pub");
    let path = env::var_os("MAZBRI_LICENSE_PUB").map_or(default, PathBuf::from);
    println!("cargo:rerun-if-env-changed=MAZBRI_LICENSE_PUB");
    println!("cargo:rerun-if-changed={}", path.display());

    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("无法读取许可证公钥 {}: {error}", path.display());
    });
    let mut lines = text.lines();
    let kid = lines.next().unwrap_or("").trim();
    let hex = lines.next().unwrap_or("").trim();
    if kid.is_empty()
        || kid.contains('"')
        || kid.contains('\\')
        || kid.chars().any(char::is_whitespace)
    {
        panic!("许可证公钥第一行不是合法 kid: {}", path.display());
    }
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        panic!("许可证公钥第二行不是 64 位小写 hex: {}", path.display());
    }

    let mut bytes = format!("pub const LICENSE_KID: &str = \"{kid}\";\n");
    bytes.push_str("pub const PRODUCTION_PUBLIC_KEY: [u8; 32] = [");
    for index in (0..64).step_by(2) {
        bytes.push_str(&format!("0x{},", &hex[index..index + 2]));
    }
    bytes.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("license_pub.rs");
    fs::write(&out, bytes).unwrap_or_else(|error| {
        panic!("无法写入 {}: {error}", out.display());
    });
}
