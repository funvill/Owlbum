//! The full form of `command!` and `area!`: an async command, generic over the
//! runtime, with arguments, an optional argument, an injected `AppHandle` and
//! an event. Later commands copy this shape.

mod common;

use std::sync::mpsc;
use std::time::Duration;

use common::invoke;
use tauri::Listener;
use tauri::test::{MockRuntime, mock_builder};

mod demo {
    use owlbum_core::{AppError, Event};
    use owlbum_lib::{area, command};
    use serde::{Deserialize, Serialize};
    use tauri::{AppHandle, Runtime};
    use ts_rs::TS;

    #[derive(Debug, PartialEq, Serialize, Deserialize, TS)]
    pub struct Seen {
        pub photo_id: i64,
        pub note: Option<String>,
    }

    #[derive(Clone, Serialize, TS)]
    pub struct DemoPing {
        pub photo_id: i64,
    }

    impl Event for DemoPing {
        const NAME: &'static str = "demo_ping";
    }

    area! {
        prefix: "demo",
        commands: [demo_seen],
        events: [DemoPing],
    }

    command! {
        /// Records that a photo was seen and tells the page.
        pub async fn demo_seen<R: Runtime>(
            photo_id: i64,
            note: Option<String>;
            app: AppHandle<R>,
        ) -> Seen {
            owlbum_lib::events::emit(&app, &DemoPing { photo_id }).map_err(AppError::internal)?;
            Ok(Seen { photo_id, note })
        }
    }
}

fn demo_window() -> (common::TestApp, tauri::WebviewWindow<MockRuntime>) {
    common::window(
        mock_builder().invoke_handler(owlbum_lib::commands::registry::invoke_handler(vec![
            demo::module(),
        ])),
    )
}

#[test]
fn arguments_are_passed_by_their_snake_case_names() {
    let (_app, window) = demo_window();
    let reply = invoke(
        &window,
        "demo_seen",
        serde_json::json!({ "photo_id": 7, "note": "hi" }),
    )
    .expect("demo_seen answers");
    assert_eq!(
        reply.deserialize::<demo::Seen>().unwrap(),
        demo::Seen {
            photo_id: 7,
            note: Some("hi".into())
        }
    );

    // An optional argument may be left out.
    let reply = invoke(&window, "demo_seen", serde_json::json!({ "photo_id": 8 })).unwrap();
    assert_eq!(reply.deserialize::<demo::Seen>().unwrap().note, None);

    // camelCase is not accepted.
    assert!(invoke(&window, "demo_seen", serde_json::json!({ "photoId": 7 })).is_err());
}

#[test]
fn a_command_can_send_an_event() {
    let (app, window) = demo_window();
    let (tx, rx) = mpsc::channel();
    app.listen("demo_ping", move |e| {
        tx.send(e.payload().to_owned()).unwrap();
    });
    invoke(&window, "demo_seen", serde_json::json!({ "photo_id": 3 })).unwrap();
    let payload = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the event arrives");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&payload).unwrap(),
        serde_json::json!({ "photo_id": 3 })
    );
}

#[test]
fn the_bindings_describe_the_command_and_event() {
    let files = owlbum_lib::bindings::render(&[demo::module::<MockRuntime>()]);
    let demo = &files["demo.ts"];
    assert!(
        demo.contains(
            "demo_seen: { args: { photo_id: number; note: string | null }; result: Seen };"
        ),
        "{demo}"
    );
    assert!(demo.contains("demo_ping: DemoPing;"), "{demo}");
    assert!(
        demo.contains("import type { DemoPing } from \"./DemoPing\";"),
        "{demo}"
    );
    assert!(files.contains_key("Seen.ts"));
}
