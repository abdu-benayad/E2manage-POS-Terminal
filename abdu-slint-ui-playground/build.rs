use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let library_paths = HashMap::from([(
        "abdu-slint-ui".to_string(),
        manifest_dir.join("../abdu-slint-ui/lib.slint"),
    )]);
    let config =
        slint_build::CompilerConfiguration::new().with_library_paths(library_paths);
    slint_build::compile_with_config("ui/playground.slint", config).unwrap();
}
