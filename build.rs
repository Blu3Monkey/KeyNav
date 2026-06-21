fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        if std::path::Path::new("installer/assets/keynav.ico").exists() {
            res.set_icon("installer/assets/keynav.ico");
        }
        if let Err(e) = res.compile() {
            eprintln!("winres: {e}");
        }
    }
}
