fn main() {
    // Compile GResource bundle for plugin-specific icons (weather, media, pomodoro)
    glib_build_tools::compile_resources(
        &["src/resources/icons"],
        "src/resources/icons.gresource.xml",
        "icons.gresource",
    );

    println!("cargo:rerun-if-changed=src/resources/icons.gresource.xml");
    println!("cargo:rerun-if-changed=src/resources/icons/");
}
