#[cfg(target_os = "windows")]
fn main() {
    verify_compiled_auth_env();
    embed_resource::compile("resources/windows/soukou.rc", embed_resource::NONE);
}

#[cfg(not(target_os = "windows"))]
fn main() {
    verify_compiled_auth_env();
}

fn verify_compiled_auth_env() {
    println!("cargo:rerun-if-env-changed=SOUKOU_SITE_URL");
    println!("cargo:rerun-if-env-changed=SOUKOU_SUPABASE_URL");
    println!("cargo:rerun-if-env-changed=SOUKOU_SUPABASE_PUBLISHABLE_KEY");

    let running_on_github_actions = std::env::var_os("GITHUB_ACTIONS").is_some();
    if !running_on_github_actions {
        return;
    }

    for key in [
        "SOUKOU_SITE_URL",
        "SOUKOU_SUPABASE_URL",
        "SOUKOU_SUPABASE_PUBLISHABLE_KEY",
    ] {
        std::env::var(key)
            .unwrap_or_else(|_| panic!("missing required compile-time environment variable: {key}"));
    }
}
