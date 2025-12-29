fn main() {
    // Compile GResource bundle for icons
    glib_build_tools::compile_resources(
        &["src/resources/icons"],
        "src/resources/icons.gresource.xml",
        "icons.gresource",
    );

    // Rerun if icon files change
    println!("cargo:rerun-if-changed=src/resources/icons.gresource.xml");
    println!("cargo:rerun-if-changed=src/resources/icons/");
}
