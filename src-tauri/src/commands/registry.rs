//! How commands and events are declared, collected and described.
//!
//! Each area module (`commands/<area>.rs`) declares its commands with
//! [`command!`](crate::command) and lists them, with its events, in one
//! [`area!`](crate::area) call. [`super::modules`] collects the areas, and
//! [`invoke_handler`] routes each `invoke` to the area that owns the command.
//! The same lists feed the TypeScript binding generator
//! ([`crate::bindings`]), so a command's name, arguments and reply are written
//! once, in Rust.

use std::collections::HashMap;
use std::sync::Arc;

use owlbum_core::Event;
use tauri::Runtime;
use tauri::ipc::Invoke;
use ts_rs::{Config, TS};

/// A TypeScript view of one Rust type, kept as plain function pointers so a
/// registry can hold many different types.
#[derive(Clone, Copy)]
pub struct TypeRef {
    /// The type's name in TypeScript, for example `AppVersion` or
    /// `Array<Row>`.
    pub name: fn(&Config) -> String,
    /// Adds the type and everything it depends on to the files to generate.
    pub collect: fn(&mut crate::bindings::Files),
    /// Adds the types a module file must import to use `name`.
    pub imports: fn(&mut crate::bindings::Imports),
}

impl TypeRef {
    /// The reference for `T`.
    pub fn of<T: TS + 'static + ?Sized>() -> Self {
        Self {
            name: <T as TS>::name,
            collect: crate::bindings::collect::<T>,
            imports: crate::bindings::imports::<T>,
        }
    }
}

/// One command's signature.
#[derive(Clone)]
pub struct CommandSig {
    /// The `invoke` name, `snake_case`.
    pub name: &'static str,
    /// Each argument's name (as the page passes it) and type.
    pub args: Vec<(&'static str, TypeRef)>,
    /// The reply's type. Errors are always `AppError`.
    pub result: TypeRef,
}

/// One event's signature.
#[derive(Clone, Copy)]
pub struct EventSig {
    /// The event name, `snake_case`.
    pub name: &'static str,
    /// The payload's type.
    pub payload: TypeRef,
}

impl EventSig {
    /// The signature of the event `E`.
    pub fn of<E: Event>() -> Self {
        Self {
            name: E::NAME,
            payload: TypeRef::of::<E>(),
        }
    }
}

/// Implemented by [`command!`](crate::command) for each command, on a
/// zero-sized type that shares the command's name.
pub trait Signature {
    /// The command's signature.
    fn signature() -> CommandSig;
}

/// The handler type Tauri calls for an `invoke`. It answers false when it
/// does not know the command.
pub type Handler<R> = Box<dyn Fn(Invoke<R>) -> bool + Send + Sync>;

/// One area's commands and events.
pub struct Module<R: Runtime> {
    /// The command name prefix without its underscore, for example `app`.
    /// It also names the generated TypeScript file.
    pub prefix: &'static str,
    /// Tauri's handler for this area's commands.
    pub handler: Handler<R>,
    /// The area's command signatures.
    pub commands: Vec<CommandSig>,
    /// The area's events.
    pub events: Vec<EventSig>,
}

/// Builds the one `invoke` handler for the app from the areas' handlers.
///
/// # Panics
///
/// When two areas declare the same command name. A test runs this.
pub fn invoke_handler<R: Runtime>(
    modules: Vec<Module<R>>,
) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    let mut owner: HashMap<&'static str, usize> = HashMap::new();
    for (i, m) in modules.iter().enumerate() {
        for c in &m.commands {
            if let Some(prev) = owner.insert(c.name, i) {
                panic!(
                    "command `{}` is declared by both `{}` and `{}`",
                    c.name, modules[prev].prefix, m.prefix
                );
            }
        }
    }
    let handlers: Arc<Vec<Handler<R>>> = Arc::new(modules.into_iter().map(|m| m.handler).collect());
    move |invoke: Invoke<R>| match owner.get(invoke.message.command()) {
        Some(&i) => (handlers[i])(invoke),
        None => false,
    }
}

/// Declares a command.
///
/// Write the function as usual, without `#[tauri::command]` and with the
/// reply type alone (the macro wraps it in `Result<_, AppError>`). Arguments
/// the page passes come first; arguments Tauri injects (`State`, `AppHandle`,
/// `Window`) come after a `;`. The page passes arguments by their Rust names
/// (`snake_case`).
///
/// ```ignore
/// command! {
///     /// Opens an album.
///     pub async fn album_open<R: Runtime>(path: String; app: AppHandle<R>) -> AlbumStatus {
///         ...
///     }
/// }
/// ```
#[macro_export]
macro_rules! command {
    ($(#[$m:meta])* $v:vis async fn $($rest:tt)*) => {
        $crate::command!(@parse [$(#[$m])*] [$v] [async] $($rest)*);
    };
    ($(#[$m:meta])* $v:vis fn $($rest:tt)*) => {
        $crate::command!(@parse [$(#[$m])*] [$v] [] $($rest)*);
    };
    (@parse [$($m:tt)*] [$v:vis] [$($async:tt)?]
        $name:ident $(< $($g:ident : $gb:path),* $(,)? >)?
        ( $($arg:ident : $argty:ty),* $(,)? $(; $($inj:ident : $injty:ty),* $(,)?)? )
        -> $ret:ty $body:block
    ) => {
        $($m)*
        #[::tauri::command(rename_all = "snake_case")]
        $v $($async)? fn $name $(< $($g : $gb),* >)? (
            $($arg: $argty,)* $($($inj: $injty,)*)?
        ) -> ::std::result::Result<$ret, ::owlbum_core::AppError> $body

        #[doc(hidden)]
        #[allow(non_camel_case_types, dead_code)]
        pub struct $name {}

        impl $crate::commands::registry::Signature for $name {
            fn signature() -> $crate::commands::registry::CommandSig {
                $crate::commands::registry::CommandSig {
                    name: ::std::stringify!($name),
                    args: ::std::vec![$(
                        (::std::stringify!($arg), $crate::commands::registry::TypeRef::of::<$argty>())
                    ),*],
                    result: $crate::commands::registry::TypeRef::of::<$ret>(),
                }
            }
        }
    };
}

/// Lists an area's commands and events. Generates `pub fn module<R>()`.
///
/// ```ignore
/// area! {
///     prefix: "app",
///     commands: [app_version],
///     events: [AppClosing],
/// }
/// ```
#[macro_export]
macro_rules! area {
    (
        prefix: $prefix:literal,
        commands: [$($cmd:ident),* $(,)?],
        events: [$($ev:ty),* $(,)?] $(,)?
    ) => {
        /// This area's commands and events, for the registry.
        pub fn module<R: ::tauri::Runtime>() -> $crate::commands::registry::Module<R> {
            $crate::commands::registry::Module {
                prefix: $prefix,
                handler: ::std::boxed::Box::new(::tauri::generate_handler![$($cmd),*]),
                commands: ::std::vec![$(
                    <$cmd as $crate::commands::registry::Signature>::signature()
                ),*],
                events: ::std::vec![$($crate::commands::registry::EventSig::of::<$ev>()),*],
            }
        }
    };
}
