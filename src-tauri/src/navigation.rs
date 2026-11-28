//! The webview's navigation rules (docs/08-desktop-and-release.md,
//! "Navigation and new windows"): the window may show only the app's own
//! page. Every other navigation, including a redirect, and every new window
//! is refused and logged. Links the user should follow are opened in their
//! own browser by `app_open_…` commands, never by the page.

use tauri::Url;

/// Whether the main window may navigate to `url`. The app's own page is
/// `tauri://localhost` on Linux and macOS and `http://tauri.localhost` (or
/// `https://` with `useHttpsScheme`) on Windows. `dev_url` is the Vite
/// server's address when the program runs under `tauri dev`; its origin is
/// then the app's page too.
pub fn allowed(url: &Url, dev_url: Option<&Url>) -> bool {
    let own = match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost") && url.port().is_none(),
        _ => false,
    };
    own || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

/// The navigation handler for the main window: allows the app's own page,
/// refuses and logs everything else.
pub fn check(url: &Url, dev_url: Option<&Url>) -> bool {
    let ok = allowed(url, dev_url);
    if !ok {
        tracing::warn!(target: "app", "refused navigation {url}");
    }
    ok
}

/// Logs a refused new window or pop-up. Every one is refused.
pub fn refuse_new_window(url: &Url) {
    tracing::warn!(target: "app", "refused new window {url}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn the_apps_own_page_is_allowed_in_every_platform_form() {
        assert!(allowed(&url("tauri://localhost/"), None));
        assert!(allowed(&url("tauri://localhost/index.html"), None));
        assert!(allowed(&url("http://tauri.localhost/"), None));
        assert!(allowed(&url("https://tauri.localhost/#/grid"), None));
    }

    #[test]
    fn anywhere_else_is_refused() {
        for s in [
            "https://github.com/funvill/Owlbum",
            "https://tile.openstreetmap.org/1/1/1.png",
            "http://owlbum.localhost/thumb/1",
            "owlbum://thumb/1",
            "http://tauri.localhost.example.com/",
            "http://tauri.localhost:8080/",
            "tauri://evil/",
            "file:///C:/Windows/",
            "javascript:alert(1)",
            "data:text/html,hi",
            "about:blank",
            "http://localhost:5173/",
        ] {
            assert!(!allowed(&url(s), None), "{s} should be refused");
        }
    }

    #[test]
    fn under_tauri_dev_the_vite_server_is_the_apps_page() {
        let dev = url("http://localhost:5173");
        assert!(allowed(
            &url("http://localhost:5173/src/main.tsx"),
            Some(&dev)
        ));
        assert!(!allowed(&url("http://localhost:5174/"), Some(&dev)));
        assert!(!allowed(&url("https://localhost:5173/"), Some(&dev)));
        assert!(!allowed(&url("https://example.com/"), Some(&dev)));
    }
}
