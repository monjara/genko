pub fn watch_mode() -> bool {
    std::env::var("SOUKOU_DEVELOPMENT_WATCH_MODE").is_ok()
}

pub fn development_mode() -> bool {
    std::env::var("SOUKOU_DEVELOPMENT_MODE").is_ok()
}
