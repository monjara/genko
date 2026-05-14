use std::ffi::OsString;

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
