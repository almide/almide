// Link a BLAS for dense matmul. On macOS that's Accelerate (a framework);
// elsewhere fall back to almide-kernel's register-tiled matmul (no link needed).
fn main() {
    println!("cargo:rustc-check-cfg=cfg(have_blas)");
    #[cfg(target_os = "macos")]
    {
        // Accelerate provides cblas_dgemm; dense matmul routes to it.
        println!("cargo:rustc-link-lib=framework=Accelerate");
        println!("cargo:rustc-cfg=have_blas");
    }
}
