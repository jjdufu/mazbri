fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=locales/en-US.yml");
    println!("cargo:rerun-if-changed=locales/zh-CN.yml");
}
