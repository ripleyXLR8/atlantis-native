//! The commit the build comes from, shown with the version (empty when git
//! is not available, e.g. a source archive).
fn main() {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=ATLANTIS_NATIVE_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");

    // The program file's icon on Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../packaging/windows/atlantis-native.rc");
        println!("cargo:rerun-if-changed=../../assets/icon/atlantis-native.ico");
        embed_resource::compile(
            "../../packaging/windows/atlantis-native.rc",
            embed_resource::NONE,
        )
        .manifest_optional()
        .unwrap();
    }
}
