fn main() {
    // Compile GResource bundle for plugin-specific icons (weather, media, pomodoro)
    glib_build_tools::compile_resources(
        &["src/resources/icons"],
        "src/resources/icons.gresource.xml",
        "icons.gresource",
    );

    println!("cargo:rerun-if-changed=src/resources/icons.gresource.xml");
    println!("cargo:rerun-if-changed=src/resources/icons/");

    // Experimental Slint frontend: compile the launcher window markup.
    #[cfg(feature = "slint-ui")]
    {
        slint_build::compile("src/ui_slint/launcher.slint")
            .expect("failed to compile launcher.slint");
        println!("cargo:rerun-if-changed=src/ui_slint/launcher.slint");
    }
}
