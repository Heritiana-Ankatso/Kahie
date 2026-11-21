fn main() {
    #[cfg(feature = "giac")]
    giac();
}

/// Build the C++ shim and link Giac.
///
/// - Default: the system library (`libgiac-dev` on Linux).
/// - `GIAC_LIB_DIR` (or `GIAC_LIB_DIR_<target with underscores>`): a static minimal build
///   (`libgiac.a`, `libmpfr.a`, `libgmp.a`, headers in `<dir>/include/giac`), as produced by
///   the `giac-android` CI workflow. On Android the C++ runtime is linked statically, so no
///   extra shared library ships in the APK.
#[cfg(feature = "giac")]
fn giac() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let key = format!("GIAC_LIB_DIR_{}", target.replace('-', "_"));
    println!("cargo:rerun-if-env-changed={key}");
    println!("cargo:rerun-if-env-changed=GIAC_LIB_DIR");
    println!("cargo:rerun-if-changed=src/giac_shim.cpp");
    let dir = std::env::var(&key).or_else(|_| std::env::var("GIAC_LIB_DIR")).ok();
    let android = target.contains("android");

    let mut b = cc::Build::new();
    b.cpp(true)
        .file("src/giac_shim.cpp")
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-w");
    if let Some(dir) = &dir {
        b.include(format!("{dir}/include"));
        // Must match the defines Giac's own Makefile compiles the library with.
        for d in ["HAVE_CONFIG_H", "TIMEOUT", "GIAC_GENERIC_CONSTANTS", "USE_OBJET_BIDON"] {
            b.define(d, None);
        }
        if android {
            b.cpp_link_stdlib(Some("c++_static"));
        }
    }
    b.compile("giacshim");

    match dir {
        Some(dir) => {
            println!("cargo:rustc-link-search=native={dir}");
            println!("cargo:rustc-link-lib=static=giac");
            println!("cargo:rustc-link-lib=static=mpfr");
            println!("cargo:rustc-link-lib=static=gmp");
            if android {
                println!("cargo:rustc-link-lib=c++abi");
                println!("cargo:rustc-link-lib=log");
            }
            // Extra libraries a native configure may have picked up (e.g. "mpfi").
            if let Ok(extra) = std::env::var("GIAC_EXTRA_LIBS") {
                for l in extra.split_whitespace() {
                    println!("cargo:rustc-link-lib={l}");
                }
            }
        }
        None => {
            println!("cargo:rustc-link-lib=giac");
            println!("cargo:rustc-link-lib=gmp");
        }
    }
    println!("cargo:rerun-if-env-changed=GIAC_EXTRA_LIBS");
}
