fn main() {
    std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1");
    slint_build::compile("ui/app.slint").expect("slint compile");
}
