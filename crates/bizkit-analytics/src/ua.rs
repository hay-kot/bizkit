//! A small User-Agent reader: the browser, OS and device buckets reports
//! show, and a bot check.
//!
//! Order matters below: many browsers name others in their UA (Edge says
//! Chrome and Safari, Chrome says Safari).

/// What a User-Agent says, in report buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Agent {
    /// The browser family.
    pub browser: &'static str,
    /// The OS family.
    pub os: &'static str,
    /// `desktop`, `mobile` or `tablet`.
    pub device: &'static str,
}

/// Fragments of crawler, monitor and script User-Agents, lowercase.
const BOT_MARKERS: &[&str] = &[
    "bot",
    "crawl",
    "spider",
    "slurp",
    "headless",
    "lighthouse",
    "phantomjs",
    "puppeteer",
    "playwright",
    "selenium",
    "python-",
    "python/",
    "curl/",
    "wget/",
    "go-http-client",
    "java/",
    "okhttp",
    "axios/",
    "node-fetch",
    "undici",
    "facebookexternalhit",
    "embedly",
    "preview",
    "monitor",
    "uptime",
    "pingdom",
    "statuscake",
    "httpclient",
    "scrapy",
    "feedfetcher",
];

/// Whether `ua` looks like a crawler, monitor or script rather than a person.
#[must_use]
pub fn is_bot(ua: &str) -> bool {
    let ua = ua.trim();
    if ua.len() < 10 || !ua.contains('/') {
        return true;
    }
    let lower = ua.to_ascii_lowercase();
    BOT_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Reads `ua`. `width`, the window width the snippet sent, decides the
/// device when present: phones under 576 CSS pixels, tablets under 992.
#[must_use]
pub fn parse(ua: &str, width: Option<u32>) -> Agent {
    let os = os(ua);
    Agent { browser: browser(ua), os, device: device(ua, os, width) }
}

fn browser(ua: &str) -> &'static str {
    let has = |needle: &str| ua.contains(needle);
    if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        "Edge"
    } else if has("OPR/") || has("Opera") {
        "Opera"
    } else if has("SamsungBrowser/") {
        "Samsung Internet"
    } else if has("Firefox/") || has("FxiOS/") {
        "Firefox"
    } else if has("Chrome/") || has("CriOS/") || has("Chromium/") {
        "Chrome"
    } else if has("Safari/") && has("Version/") {
        "Safari"
    } else {
        "Other"
    }
}

fn os(ua: &str) -> &'static str {
    let has = |needle: &str| ua.contains(needle);
    if has("iPhone") || has("iPad") || has("iPod") {
        "iOS"
    } else if has("Android") {
        "Android"
    } else if has("Windows") {
        "Windows"
    } else if has("CrOS") {
        "Chrome OS"
    } else if has("Mac OS X") || has("Macintosh") {
        "macOS"
    } else if has("Linux") || has("X11") {
        "Linux"
    } else {
        "Other"
    }
}

fn device(ua: &str, os: &str, width: Option<u32>) -> &'static str {
    if let Some(width) = width.filter(|w| *w > 0) {
        return match width {
            0..576 => "mobile",
            576..992 => "tablet",
            _ => "desktop",
        };
    }
    if ua.contains("iPad") || ua.contains("Tablet") || (os == "Android" && !ua.contains("Mobile")) {
        "tablet"
    } else if ua.contains("Mobi") || ua.contains("iPhone") {
        "mobile"
    } else {
        "desktop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    const SAFARI_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1";
    const EDGE_WIN: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";
    const FIREFOX_LINUX: &str =
        "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0";
    const CHROME_ANDROID: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36";
    const SAMSUNG_TAB: &str = "Mozilla/5.0 (Linux; Android 13; SM-X700) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/23.0 Chrome/115.0.0.0 Safari/537.36";

    #[test]
    fn buckets() {
        let cases = [
            (CHROME_MAC, "Chrome", "macOS", "desktop"),
            (SAFARI_IPHONE, "Safari", "iOS", "mobile"),
            (EDGE_WIN, "Edge", "Windows", "desktop"),
            (FIREFOX_LINUX, "Firefox", "Linux", "desktop"),
            (CHROME_ANDROID, "Chrome", "Android", "mobile"),
            (SAMSUNG_TAB, "Samsung Internet", "Android", "tablet"),
        ];
        for (ua, browser, os, device) in cases {
            assert_eq!(parse(ua, None), Agent { browser, os, device }, "{ua}");
        }
    }

    #[test]
    fn width_decides_the_device() {
        assert_eq!(parse(CHROME_MAC, Some(400)).device, "mobile");
        assert_eq!(parse(CHROME_MAC, Some(800)).device, "tablet");
        assert_eq!(parse(SAFARI_IPHONE, Some(1200)).device, "desktop");
        assert_eq!(parse(SAFARI_IPHONE, Some(0)).device, "mobile", "zero is no width");
    }

    #[test]
    fn bots() {
        for ua in [
            "",
            "Mozilla",
            "Googlebot/2.1 (+http://www.google.com/bot.html)",
            "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
            "curl/8.4.0",
            "python-requests/2.31.0",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/120.0.0.0 Safari/537.36",
            "facebookexternalhit/1.1",
        ] {
            assert!(is_bot(ua), "{ua}");
        }
        for ua in [CHROME_MAC, SAFARI_IPHONE, EDGE_WIN, FIREFOX_LINUX, CHROME_ANDROID, SAMSUNG_TAB]
        {
            assert!(!is_bot(ua), "{ua}");
        }
    }

    #[test]
    fn fuzz_parse() {
        bolero::check!().with_type::<(String, Option<u32>)>().for_each(|(ua, width)| {
            let agent = parse(ua, *width);
            assert!(matches!(agent.device, "desktop" | "mobile" | "tablet"), "device");
            assert!(!ua.trim().is_empty() || is_bot(ua), "an empty agent is a bot");
        });
    }
}
