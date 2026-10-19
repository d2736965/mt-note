fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let config = stylance_cli::Config::load(manifest_dir.clone().into())
        .expect("Failed to load stylance config");
    declare_rerun_deps(&manifest_dir);
    stylance_cli::run(&config).expect("Stylance failed");
}

fn declare_rerun_deps(manifest_dir: &str) {
    println!("cargo:rerun-if-changed={manifest_dir}/assets/styles/*.scss");
    println!("cargo:rerun-if-changed={manifest_dir}/src/**/*.scss");
}
