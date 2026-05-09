fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", config).expect("Slint compile failed");
    println!("cargo:rerun-if-changed=assets/fonts");
}
