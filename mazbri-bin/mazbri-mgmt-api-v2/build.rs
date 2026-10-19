use std::path::Path;

use ts_rs::{Config, TS};

#[path = "src/handlers/types.rs"]
pub mod api_types;

fn main() {
    println!("cargo:rerun-if-changed=src/handlers/types.rs");
    println!("cargo:rustc-check-cfg=cfg(mazbri_skip_frontend_embed)");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let workspace_root = Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find workspace root");
    export_bindings(workspace_root);
    println!("cargo:rerun-if-env-changed=MAZBRI_SKIP_FRONTEND_EMBED");
    let skip_frontend_embed = matches!(
        std::env::var("MAZBRI_SKIP_FRONTEND_EMBED").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    );
    if skip_frontend_embed {
        println!("cargo:rustc-cfg=mazbri_skip_frontend_embed");
        println!(
            "cargo:warning=Skipping frontend dist embed because MAZBRI_SKIP_FRONTEND_EMBED is set"
        );
        return;
    }
    let frontend_dir = workspace_root.join("mazbri-web");
    let dist_dir = frontend_dir.join("dist");
    if !dist_dir.exists() {
        panic!(
            "Frontend dist not found at {:?}! Please run: ./scripts/build.sh --frontend-only or cd mazbri-mgmt-web && pnpm build\n\
             / 前端 dist 未找到！请运行: ./scripts/build.sh --frontend-only 或 cd mazbri-mgmt-web && pnpm build",
            dist_dir
        );
    }
    println!("前端 dist 已找到，将嵌入二进制: {}", dist_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        frontend_dir.join("src").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        frontend_dir.join("public").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        frontend_dir.join("index.html").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        frontend_dir.join("vite.config.ts").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        frontend_dir.join("package.json").display()
    );
    println!("cargo:rerun-if-changed={}", dist_dir.display());
}

fn export_bindings(workspace_root: &Path) {
    let cfg = Config::new().with_out_dir(workspace_root.join("mazbri-web/src/types/api/generated"));
    api_types::ListParams::export_all(&cfg).expect("failed to export ListParams bindings");
    api_types::CreateTenantRequest::export_all(&cfg)
        .expect("failed to export CreateTenantRequest bindings");
    api_types::UpdateApiKeyRequest::export_all(&cfg)
        .expect("failed to export UpdateApiKeyRequest bindings");
    api_types::AdminUser::export_all(&cfg).expect("failed to export AdminUser bindings");
    api_types::LoginResponse::export_all(&cfg).expect("failed to export LoginResponse bindings");
}
