//! The allowed-hosts rule for model downloads (docs/07-tagging.md, "Allowed
//! hosts"). Every redirect hop is checked with it too.

/// The host of `url` when it may be fetched: `https`, no explicit port, no
/// user name or password, and a Hugging Face host. `Err` carries the host
/// (or the whole text when it has none), for "Download host not allowed".
pub fn allowed(url: &str) -> Result<String, String> {
    let rest = url.strip_prefix("https://").ok_or_else(|| host_of(url))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains(':') {
        return Err(host_of(url));
    }
    let host = authority.to_ascii_lowercase();
    let ok =
        host == "huggingface.co" || host.ends_with(".huggingface.co") || host.ends_with(".hf.co");
    // A host made only of dots and a suffix (".hf.co") is not a name.
    if ok && !host.starts_with('.') && !host.contains("..") {
        Ok(host)
    } else {
        Err(host)
    }
}

/// The host part of any URL-like text, for the error message.
fn host_of(url: &str) -> String {
    let after = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    host.split(':').next().unwrap_or(host).to_ascii_lowercase()
}

/// Resolves a redirect's `Location` against the URL that sent it: an
/// absolute URL as it is, a path from the root against the same origin.
pub fn resolve(base: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_owned();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("https", base));
    let authority = rest.split('/').next().unwrap_or("");
    if let Some(stripped) = location.strip_prefix("//") {
        return format!("{scheme}://{stripped}");
    }
    if location.starts_with('/') {
        return format!("{scheme}://{authority}{location}");
    }
    let dir = base.rsplit_once('/').map_or(base, |(d, _)| d);
    format!("{dir}/{location}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nine_urls_of_the_table() {
        let cases = [
            ("https://huggingface.co/x/resolve/abc/model.onnx", true),
            ("https://cdn-lfs.huggingface.co/a/b", true),
            ("https://cas-bridge.xethub.hf.co/a/b", true),
            ("https://cdn-lfs.hf.co/a", true),
            ("http://huggingface.co/x", false),
            ("https://huggingface.co.evil.com/x", false),
            ("https://evilhuggingface.co/x", false),
            ("https://huggingface.co:8443/x", false),
            ("https://user:pw@huggingface.co/x", false),
        ];
        for (url, ok) in cases {
            assert_eq!(allowed(url).is_ok(), ok, "{url}");
        }
    }

    #[test]
    fn the_refused_host_is_named() {
        assert_eq!(allowed("https://evil.com/x"), Err("evil.com".into()));
        assert_eq!(
            allowed("https://HuggingFace.co/x"),
            Ok("huggingface.co".into())
        );
    }

    #[test]
    fn locations_resolve_against_the_sender() {
        let base = "https://huggingface.co/r/resolve/abc/onnx/m.onnx";
        assert_eq!(
            resolve(base, "https://cdn-lfs.hf.co/x"),
            "https://cdn-lfs.hf.co/x"
        );
        assert_eq!(resolve(base, "/api/x"), "https://huggingface.co/api/x");
        assert_eq!(
            resolve(base, "n.onnx"),
            "https://huggingface.co/r/resolve/abc/onnx/n.onnx"
        );
    }
}
