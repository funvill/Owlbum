//! The real platform calls behind file actions
//! ([`crate::commands::file::Desktop`]): the native confirm, the bin, the
//! file manager, the clipboard and dragging out.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::commands::file::Desktop;
use tauri::{AppHandle, Manager, Runtime};

/// The platform calls, over the main window.
pub struct NativeDesktop<R: Runtime> {
    app: AppHandle<R>,
    /// Kept for the life of the program: on Linux the clipboard's content
    /// is served by this object and goes when it is dropped.
    clipboard: Mutex<Option<arboard::Clipboard>>,
}

impl<R: Runtime> NativeDesktop<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            clipboard: Mutex::new(None),
        }
    }

    fn with_clipboard(
        &self,
        job: impl FnOnce(&mut arboard::Clipboard) -> Result<(), arboard::Error>,
    ) -> Result<(), String> {
        let mut held = self.clipboard.lock().unwrap_or_else(|p| p.into_inner());
        if held.is_none() {
            *held = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let clipboard = held.as_mut().ok_or_else(String::new)?;
        job(clipboard).map_err(|e| e.to_string())
    }
}

impl<R: Runtime> Desktop for NativeDesktop<R> {
    fn confirm_trash(&self, message: &str, detail: &str) -> bool {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        let mut dialog = self
            .app
            .dialog()
            .message(format!("{message}\n\n{detail}"))
            .title(owlbum_core::text::files::trash_button())
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                owlbum_core::text::files::trash_button(),
                owlbum_core::text::files::CANCEL.to_owned(),
            ));
        if let Some(window) = self.app.get_webview_window("main") {
            dialog = dialog.parent(&window);
        }
        dialog.blocking_show()
    }

    fn trash(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| match e {
            trash::Error::Os { description, .. } => description,
            other => other.to_string(),
        })
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
    }

    fn copy_text(&self, text: &str) -> Result<(), String> {
        self.with_clipboard(|c| c.set_text(text))
    }

    fn copy_image(&self, width: u32, height: u32, rgba: Vec<u8>) -> Result<(), String> {
        self.with_clipboard(|c| {
            c.set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: rgba.into(),
            })
        })
    }

    #[cfg(windows)]
    fn copy_files(&self, paths: &[PathBuf]) -> Result<(), String> {
        let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        let _open = clipboard_win::Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
        // The list replaces whatever the clipboard held.
        clipboard_win::raw::empty().map_err(|e| e.to_string())?;
        clipboard_win::raw::set_file_list(&list).map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    fn copy_files(&self, _paths: &[PathBuf]) -> Result<(), String> {
        Err(owlbum_core::text::files::NOT_SUPPORTED.to_owned())
    }

    #[cfg(windows)]
    fn start_drag(&self, paths: Vec<PathBuf>, icon: Vec<u8>) -> Result<(), String> {
        let window = self
            .app
            .get_webview_window("main")
            .ok_or_else(|| "no window".to_owned())?;
        let (send, receive) = std::sync::mpsc::channel();
        self.app
            .run_on_main_thread(move || {
                // Copy only: the receiving program never moves the original.
                let started = drag::start_drag(
                    &window,
                    drag::DragItem::Files(paths),
                    drag::Image::Raw(icon),
                    |_, _| {},
                    drag::Options {
                        skip_animatation_on_cancel_or_failure: false,
                        mode: drag::DragMode::Copy,
                    },
                );
                let _ = send.send(started.map_err(|e| e.to_string()));
            })
            .map_err(|e| e.to_string())?;
        receive.recv().map_err(|e| e.to_string())?
    }

    #[cfg(not(windows))]
    fn start_drag(&self, _paths: Vec<PathBuf>, _icon: Vec<u8>) -> Result<(), String> {
        Err(owlbum_core::text::files::NOT_SUPPORTED.to_owned())
    }
}
