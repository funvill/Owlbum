//! The network policy (docs/08-desktop-and-release.md, "Network policy"):
//! the Content-Security-Policy matches the design document, and the page's
//! capability grants only the app's own commands and event listening, from
//! the app's own page.

use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

mod common;

fn conf() -> serde_json::Value {
    serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
}

/// The policy as the design document writes it: the one line in the code
/// block under "Content-Security-Policy".
fn documented_csp() -> String {
    let doc = include_str!("../../docs/08-desktop-and-release.md").replace("\r\n", "\n");
    let section = doc
        .split("### Content-Security-Policy\n")
        .nth(1)
        .expect("docs/08 has a Content-Security-Policy section");
    let block = section
        .split("```text\n")
        .nth(1)
        .expect("the section has a text block");
    block.lines().next().unwrap().to_string()
}

#[test]
fn the_content_security_policy_is_the_documented_one() {
    let conf = conf();
    assert_eq!(conf["app"]["security"]["csp"], documented_csp());
}

#[test]
fn the_policy_names_one_remote_host_the_tile_server() {
    let csp = documented_csp();
    let remote: Vec<&str> = csp
        .split([' ', ';'])
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
        .filter(|s| !s.ends_with(".localhost"))
        .collect();
    assert_eq!(remote, ["https://tile.openstreetmap.org"]);
    assert!(csp.contains("script-src 'self';"), "no script exception");
    assert!(csp.contains("default-src 'self';"));
}

#[test]
fn the_development_policy_only_adds_the_vite_server() {
    let conf = conf();
    let dev = conf["app"]["security"]["devCsp"].as_str().unwrap();
    for directive in documented_csp().split("; ") {
        let name = directive.split(' ').next().unwrap();
        let dev_directive = dev
            .split("; ")
            .find(|d| d.split(' ').next() == Some(name))
            .unwrap_or_else(|| panic!("devCsp lacks {name}"));
        for source in directive.split(' ') {
            assert!(
                dev_directive.split(' ').any(|s| s == source),
                "devCsp {name} lacks {source}"
            );
        }
        for extra in dev_directive.split(' ').filter(|s| !directive.contains(*s)) {
            assert!(
                extra.contains("localhost:5173") || extra == "'unsafe-inline'",
                "devCsp adds {extra}"
            );
        }
    }
}

#[test]
fn the_capability_grants_only_commands_and_event_listening() {
    let cap: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(cap["windows"], serde_json::json!(["main"]));
    assert_eq!(cap["local"], true);
    assert!(cap.get("remote").is_none(), "no remote address is granted");
    let commands: Vec<String> = owlbum_lib::commands::modules::<MockRuntime>()
        .iter()
        .flat_map(|m| m.commands.iter().map(|c| c.name.replace('_', "-")))
        .collect();
    for p in cap["permissions"].as_array().unwrap() {
        let p = p.as_str().expect("permissions are names");
        let ok = p == "core:event:allow-listen"
            || p == "core:event:allow-unlisten"
            || p.strip_prefix("allow-")
                .is_some_and(|c| commands.iter().any(|n| n == c));
        assert!(ok, "the page should not be granted {p}");
    }
}

/// The real context (with the generated permission checks), on the mock
/// runtime.
fn real_app() -> (common::TestApp, WebviewWindow<MockRuntime>) {
    let mut context: tauri::Context<MockRuntime> = tauri::generate_context!(test = true);
    context.set_assets(Box::new(noop_assets()));
    let app = owlbum_lib::setup(mock_builder())
        .build(context)
        .expect("the app builds");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("the window builds");
    (common::TestApp(app), window)
}

fn call(
    window: &WebviewWindow<MockRuntime>,
    url: &str,
    cmd: &str,
    args: serde_json::Value,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: url.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
}

#[test]
fn the_page_may_call_the_apps_commands_and_nothing_else() {
    let (_app, window) = real_app();
    let json = serde_json::json!({});
    assert!(call(&window, common::PAGE_URL, "app_version", json.clone()).is_ok());
    for plugin in [
        "plugin:opener|open_url",
        "plugin:dialog|open",
        "plugin:window|close",
        "plugin:webview|create_webview_window",
    ] {
        let args = serde_json::json!({ "url": "https://example.com" });
        assert!(
            call(&window, common::PAGE_URL, plugin, args).is_err(),
            "{plugin} should be refused"
        );
    }
}

#[test]
fn a_remote_page_may_call_nothing() {
    let (_app, window) = real_app();
    let reply = call(
        &window,
        "https://example.com/",
        "app_version",
        serde_json::json!({}),
    );
    assert!(reply.is_err(), "a remote page called a command");
}
