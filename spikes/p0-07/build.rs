fn main() {
    let mut b = cc::Build::new();
    b.cpp(true).file("src/shim.cpp").flag_if_supported("-std=c++17").flag_if_supported("-w");
    // GIAC_LIB_DIR: use a static minimal Giac build (headers in $GIAC_LIB_DIR/../include if present).
    if let Ok(dir) = std::env::var("GIAC_LIB_DIR") {
        if let Ok(inc) = std::env::var("GIAC_INCLUDE_DIR") {
            b.include(inc);
        }
        // Must match the defines Giac's src/Makefile compiles the library with: they change
        // declarations in the headers (e.g. control_c is a function only with TIMEOUT).
        for d in ["HAVE_CONFIG_H", "TIMEOUT", "GIAC_GENERIC_CONSTANTS", "USE_OBJET_BIDON"] {
            b.define(d, None);
        }
        b.compile("giacshim");
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=static=giac");
        println!("cargo:rustc-link-lib=mpfr");
        println!("cargo:rustc-link-lib=gmp");
        println!("cargo:rustc-link-lib=stdc++");
    } else {
        b.compile("giacshim");
        println!("cargo:rustc-link-lib=giac");
        println!("cargo:rustc-link-lib=gmp");
    }
    println!("cargo:rerun-if-changed=src/shim.cpp");
    println!("cargo:rerun-if-env-changed=GIAC_LIB_DIR");
}
