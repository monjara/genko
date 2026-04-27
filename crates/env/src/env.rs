use std::ffi::OsString;

pub fn lindera_dictinary_path() -> Option<OsString> {
    std::env::var_os("GENKO_LINDERA_DICTIONARY_PATH")
}

pub fn development_mode() -> bool {
    std::env::var("GENKO_DEVELOPMENT_MODE").is_ok()
}

pub fn watch_mode() -> bool {
    let debug = std::env::var("GENKO_DEVELOPMENT_MODE").is_ok();
    println!("debug: {debug}");
    std::env::var("GENKO_DEVELOPMENT_WATCH_MODE").is_ok()
}
