fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../../App.ico");
        println!("cargo:rerun-if-changed=app.rc");
        embed_resource::compile("app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed Windows application icon");
    }
}
