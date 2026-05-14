#[cfg(target_os = "windows")]
fn main() {
    embed_resource::compile("resources/windows/soukou.rc", embed_resource::NONE);
}

#[cfg(not(target_os = "windows"))]
fn main() {}
