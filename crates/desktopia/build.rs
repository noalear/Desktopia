//! Stamps the build with the commit it came from. CI sets GITHUB_SHA, and the
//! updater compares it with the commit of the "latest" release; a build made
//! anywhere else has no stamp and never replaces itself.

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    let sha = std::env::var("GITHUB_SHA").unwrap_or_default();
    println!("cargo:rustc-env=DESKTOPIA_BUILD={}", sha.trim());
}
