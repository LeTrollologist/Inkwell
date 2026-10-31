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
        "/vendor/katex/katex.min.js" => (
            include_bytes!("../ui/vendor/katex/katex.min.js") as &'static [u8],
            "text/javascript; charset=utf-8",
        ),
        "/vendor/katex/katex.min.css" => (
            include_bytes!("../ui/vendor/katex/katex.min.css") as &'static [u8],
            "text/css; charset=utf-8",
        ),
        "/vendor/katex/LICENSE" => (
            include_bytes!("../ui/vendor/katex/LICENSE") as &'static [u8],
            "text/plain; charset=utf-8",
        ),
        "/vendor/katex/fonts/KaTeX_AMS-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_AMS-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Caligraphic-Bold.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Caligraphic-Bold.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Caligraphic-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Caligraphic-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Fraktur-Bold.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Fraktur-Bold.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Fraktur-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Fraktur-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Main-Bold.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Main-Bold.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Main-BoldItalic.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Main-BoldItalic.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Main-Italic.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Main-Italic.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Main-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Main-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Math-BoldItalic.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Math-BoldItalic.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Math-Italic.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Math-Italic.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_SansSerif-Bold.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_SansSerif-Bold.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_SansSerif-Italic.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_SansSerif-Italic.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_SansSerif-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_SansSerif-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Script-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Script-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Size1-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Size1-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Size2-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Size2-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Size3-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Size3-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Size4-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Size4-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        "/vendor/katex/fonts/KaTeX_Typewriter-Regular.woff2" => (
            include_bytes!("../ui/vendor/katex/fonts/KaTeX_Typewriter-Regular.woff2") as &'static [u8],
            "font/woff2",
        ),
        _ => return None,
    };
    Some(Asset { bytes, mime })
}
