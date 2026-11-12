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

    // Atlantis Evolution in the browser: its scripts (Lua) call the movie
    // and sound decoders deep in their stack, whose frames are large (the
    // Theora decoder's state); WebAssembly's default stack of 1 MiB, which
    // the desktop's 8 MiB main thread never reaches, overflows into the
    // heap.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg=-zstack-size=8388608");
    }

    // The program file's icon on Windows, and its main thread's stack:
    // 16 MiB, as on the desktop elsewhere the games run on the main
    // thread (winit's event loop must) with an 8 MiB stack, and Windows
    // gives it 1 MiB unless the program file asks for more (Atlantis
    // Evolution's movie opening once overflowed it: 1 MiB of frames).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bins=/STACK:16777216");
        } else {
            println!("cargo:rustc-link-arg-bins=-Wl,--stack,16777216");
        }
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
