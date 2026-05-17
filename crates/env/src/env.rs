use std::ffi::OsString;

pub fn load() {
    match dotenvy::dotenv() {
        Ok(path) => {
            println!("loaded environment from {}", path.display());
        }
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            eprintln!("failed to load .env: {error}");
        }
    }
}

pub fn lindera_dictinary_path() -> Option<OsString> {
    std::env::var_os("SOUKOU_LINDERA_DICTIONARY_PATH")
}

pub fn development_mode() -> bool {
    std::env::var("SOUKOU_DEVELOPMENT_MODE").is_ok()
}

pub fn watch_mode() -> bool {
    let debug = std::env::var("SOUKOU_DEVELOPMENT_MODE").is_ok();
    println!("debug: {debug}");
    std::env::var("SOUKOU_DEVELOPMENT_WATCH_MODE").is_ok()
}

pub fn auth_sign_in_url() -> Option<String> {
    std::env::var("SOUKOU_AUTH_SIGN_IN_URL").ok()
}

pub fn auth_account_url() -> Option<String> {
    std::env::var("SOUKOU_AUTH_ACCOUNT_URL").ok()
}

pub fn auth_callback_url() -> String {
    std::env::var("SOUKOU_AUTH_CALLBACK_URL")
        .unwrap_or_else(|_| "soukou://auth/callback".to_string())
}
