fn main() {
    // Only compile the Slint UI when the binary that uses it is being built.
    #[cfg(feature = "slint-ui")]
    slint_build::compile("ui/main.slint").unwrap();
}
