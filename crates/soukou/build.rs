#[cfg(target_os = "windows")]
fn main() {
    print_auth_env_rerun_rules();
    let _ = embed_resource::compile("resources/windows/soukou.rc", embed_resource::NONE);
}

#[cfg(not(target_os = "windows"))]
fn main() {
    print_auth_env_rerun_rules();
}

fn print_auth_env_rerun_rules() {
    println!("cargo:rerun-if-env-changed=SOUKOU_SITE_URL");
    println!("cargo:rerun-if-env-changed=SOUKOU_SUPABASE_URL");
    println!("cargo:rerun-if-env-changed=SOUKOU_SUPABASE_PUBLISHABLE_KEY");
    println!("cargo:rerun-if-env-changed=SOUKOU_AUTH_CALLBACK_SCHEME");
}
