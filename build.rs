use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=TPM_RELEASE_VERSION");

    let build_version = env::var("TPM_RELEASE_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            env::var("CARGO_PKG_VERSION")
                .expect("Cargo should provide CARGO_PKG_VERSION during builds")
        });
    println!("cargo:rustc-env=TPM_BUILD_VERSION={build_version}");

    let target = env::var("TARGET").expect("Cargo should provide TARGET during builds");
    println!("cargo:rustc-env=TPM_BUILD_TARGET={target}");
}
