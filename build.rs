fn main() {
    for input in [
        "app.rc",
        "app.manifest",
        "app.ico",
        "kr_flag.ico",
        "us_flag.ico",
        "keyboard.ico",
    ] {
        println!("cargo:rerun-if-changed={input}");
    }
    embed_resource::compile("app.rc", embed_resource::NONE)
        .manifest_required()
        .expect("failed to compile required application icon resources");
}
