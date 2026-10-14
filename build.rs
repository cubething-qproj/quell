//! Records build metadata for `--build-info` in `dev` builds.

fn main() {
    #[cfg(feature = "dev")]
    built::write_built_file().expect("Failed to get build info");
}
