//! Assets embedded in the executable and served over the custom `inkwell://`
//! protocol (mapped to `http://inkwell.localhost` on Windows).

pub struct Asset {
    pub bytes: &'static [u8],
    pub mime: &'static str,
}

pub fn lookup(path: &str) -> Option<Asset> {
    let (bytes, mime): (&'static [u8], &'static str) = match path {
        "/" | "/index.html" => (
            include_bytes!("../ui/index.html") as &'static [u8],
            "text/html; charset=utf-8",
        ),
        "/styles.css" => (
            include_bytes!("../ui/styles.css") as &'static [u8],
            "text/css; charset=utf-8",
        ),
        "/app.js" => (
            include_bytes!("../ui/app.js") as &'static [u8],
            "text/javascript; charset=utf-8",
        ),
        "/bridge.js" => (
            include_bytes!("../ui/bridge.js") as &'static [u8],
            "text/javascript; charset=utf-8",
        ),
        "/vendor/highlight.min.js" => (
            include_bytes!("../ui/vendor/highlight.min.js") as &'static [u8],
            "text/javascript; charset=utf-8",
        ),
        "/vendor/hljs-dark.css" => (
            include_bytes!("../ui/vendor/hljs-dark.css") as &'static [u8],
            "text/css; charset=utf-8",
        ),
        _ => return None,
    };
    Some(Asset { bytes, mime })
}
