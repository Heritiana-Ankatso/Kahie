fn main() {
    cc::Build::new().cpp(true).file("src/shim.cpp").flag_if_supported("-std=c++17").flag_if_supported("-w").compile("giacshim");
    println!("cargo:rustc-link-lib=giac");
    println!("cargo:rustc-link-lib=gmp");
    println!("cargo:rerun-if-changed=src/shim.cpp");
}
