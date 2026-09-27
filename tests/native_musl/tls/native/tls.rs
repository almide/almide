// Builds a blocking reqwest client on rustls: this installs ring's crypto
// provider and loads the webpki roots, so ring's C code is linked AND run.
// No network is touched.
pub fn client_ready() -> String {
    match reqwest::blocking::Client::builder().use_rustls_tls().build() {
        Ok(_) => "tls client ok".to_string(),
        Err(e) => format!("tls client failed: {e}"),
    }
}
