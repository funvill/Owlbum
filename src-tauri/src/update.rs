//! The updater (docs/08-desktop-and-release.md, "Updates"): one state
//! machine in the shell that runs the quiet and manual checks, the download
//! and the install, sends each change to the page as `update_state`, each
//! outcome as `update_message`, and keeps Help's update item in step.
//!
//! The network work is behind [`UpdateSource`]. `run` installs the plugin
//! source when the program has a public key; shell tests install a fake, so
//! no test reaches the network.

use std::sync::{Arc, Mutex, PoisonError};

use owlbum_core::app::update::{self as rules, UpdateMessage, UpdateState, UpdateStateKind};
use owlbum_core::appdata::AppData;
use tauri::{AppHandle, Manager, Runtime};

/// Where updates come from. Every call blocks; the shell calls them on a
/// thread of its own.
pub trait UpdateSource: Send + Sync {
    /// A newer version's number, or `None` when there is none (a feed that
    /// answers 404, because nothing is published yet, is `None` too).
    fn check(&self) -> Result<Option<String>, String>;
    /// Downloads and verifies the offered version, telling `progress` each
    /// rounded percentage.
    fn download(&self, progress: &mut dyn FnMut(u8)) -> Result<(), String>;
    /// Something that stops the install before anything is closed.
    fn install_problem(&self) -> Option<String>;
    /// Installs and starts the new version. On Windows the installer takes
    /// over and this process ends; an answer means it failed.
    fn install(&self) -> Result<(), String>;
}

struct Inner {
    source: Option<Arc<dyn UpdateSource>>,
    dev: bool,
    state: UpdateState,
}

/// The updater's state, managed by `setup`. It starts `unavailable` until
/// `configure` gives it a source.
pub struct Updates {
    inner: Mutex<Inner>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                source: None,
                dev: false,
                state: state(UpdateStateKind::Unavailable, None),
            }),
        }
    }
}

fn state(kind: UpdateStateKind, version: Option<String>) -> UpdateState {
    UpdateState {
        state: kind,
        version,
        percent: None,
    }
}

impl Updates {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets where updates come from. `None` leaves the updater
    /// `unavailable`: a development build, or no public key.
    pub fn configure(&self, source: Option<Arc<dyn UpdateSource>>, dev: bool) {
        let mut inner = self.lock();
        let kind = if source.is_some() {
            UpdateStateKind::Idle
        } else {
            UpdateStateKind::Unavailable
        };
        inner.source = source;
        inner.dev = dev;
        inner.state = state(kind, None);
    }

    /// The state now.
    pub fn current(&self) -> UpdateState {
        self.lock().state.clone()
    }

    /// Whether a quiet check may ever run: an installed build with a key.
    pub fn set_up(&self) -> bool {
        let inner = self.lock();
        inner.source.is_some() && !inner.dev
    }

    fn source(&self) -> Option<Arc<dyn UpdateSource>> {
        self.lock().source.clone()
    }
}

/// Moves to a new state: kept, sent to the page, and shown on the
/// taskbar while it downloads.
fn set<R: Runtime>(app: &AppHandle<R>, new: UpdateState) {
    if let Some(u) = app.try_state::<Updates>() {
        u.lock().state = new.clone();
    }
    taskbar(app, &new);
    let _ = crate::events::emit(app, &new);
}

/// The download's progress on the window's taskbar button (issue 178);
/// cleared in every other state. Best effort: a window that is gone, or a
/// desktop without the bar, shows nothing.
fn taskbar<R: Runtime>(app: &AppHandle<R>, state: &UpdateState) {
    use tauri::window::{ProgressBarState, ProgressBarStatus};
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let bar = match rules::taskbar_percent(state) {
        Some(percent) => ProgressBarState {
            status: Some(ProgressBarStatus::Normal),
            progress: Some(u64::from(percent)),
        },
        None => ProgressBarState {
            status: Some(ProgressBarStatus::None),
            progress: None,
        },
    };
    let _ = window.set_progress_bar(bar);
}

fn say<R: Runtime>(app: &AppHandle<R>, message: UpdateMessage) {
    let _ = crate::events::emit(app, &message);
}

fn current_version() -> String {
    owlbum_core::app::version().version
}

/// Runs `job` on a thread of its own: checks and downloads block.
fn spawn(job: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("owlbum-update".into())
        .spawn(job);
}

/// A check (docs/08, "When it checks"). A manual check always says what
/// it found; a quiet one speaks only when a newer version exists. A
/// second request while one runs joins it; while a version is offered or
/// downloaded, a manual check offers it again.
pub fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let (source, dev, now) = {
        let inner = updates.lock();
        (inner.source.clone(), inner.dev, inner.state.clone())
    };
    let Some(source) = source else {
        if manual {
            say(
                app,
                if dev {
                    UpdateMessage::DevBuild
                } else {
                    UpdateMessage::NotSetUp
                },
            );
        }
        return;
    };
    match now.state {
        UpdateStateKind::Idle | UpdateStateKind::Unavailable => {}
        UpdateStateKind::Available if manual => {
            say(
                app,
                UpdateMessage::Found {
                    latest: now.version.unwrap_or_default(),
                    current: current_version(),
                },
            );
            return;
        }
        UpdateStateKind::Ready if manual => {
            say(
                app,
                UpdateMessage::Downloaded {
                    version: now.version.unwrap_or_default(),
                },
            );
            return;
        }
        _ => return,
    }
    set(app, state(UpdateStateKind::Checking, None));
    tracing::info!(target: "update", "checking ({})", if manual { "manual" } else { "quiet" });
    let app = app.clone();
    spawn(move || {
        let answer = source.check();
        let current = current_version();
        match answer {
            Ok(found) => {
                record_check(&app);
                match found.filter(|v| rules::is_newer(v, &current)) {
                    Some(latest) => {
                        tracing::info!(target: "update", "{latest} is available");
                        set(
                            &app,
                            state(UpdateStateKind::Available, Some(latest.clone())),
                        );
                        say(&app, UpdateMessage::Found { latest, current });
                    }
                    None => {
                        tracing::info!(target: "update", "up to date");
                        set(&app, state(UpdateStateKind::Idle, None));
                        if manual {
                            say(&app, UpdateMessage::UpToDate { current });
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(target: "update", "the check failed: {e}");
                set(&app, state(UpdateStateKind::Idle, None));
                if manual {
                    say(
                        &app,
                        UpdateMessage::CheckFailed {
                            reason: rules::reason(&e),
                        },
                    );
                }
            }
        }
    });
}

/// `updates.last_check` is written after any check that got an answer.
fn record_check<R: Runtime>(app: &AppHandle<R>) {
    if let Some(data) = app.try_state::<AppData>() {
        let patch = serde_json::json!({ "updates.last_check": crate::commands::album::now_ms() });
        if let Some(patch) = patch.as_object() {
            let _ = data.set_settings(patch);
        }
    }
}

/// Downloads the offered version (docs/08, "Download"). Only from
/// `available`; anything else is ignored.
pub fn download<R: Runtime>(app: &AppHandle<R>) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let now = updates.current();
    let (Some(source), UpdateStateKind::Available) = (updates.source(), now.state) else {
        return;
    };
    let version = now.version.unwrap_or_default();
    set(
        app,
        UpdateState {
            state: UpdateStateKind::Downloading,
            version: Some(version.clone()),
            percent: Some(0),
        },
    );
    tracing::info!(target: "update", "downloading {version}");
    let app = app.clone();
    spawn(move || {
        let mut last = 0_u8;
        let mut progress = |percent: u8| {
            if percent != last {
                last = percent;
                set(
                    &app,
                    UpdateState {
                        state: UpdateStateKind::Downloading,
                        version: Some(version.clone()),
                        percent: Some(percent),
                    },
                );
            }
        };
        match source.download(&mut progress) {
            Ok(()) => {
                tracing::info!(target: "update", "{version} downloaded and verified");
                set(&app, state(UpdateStateKind::Ready, Some(version.clone())));
                say(&app, UpdateMessage::Downloaded { version });
            }
            Err(e) => {
                tracing::warn!(target: "update", "the download failed: {e}");
                set(&app, state(UpdateStateKind::Available, Some(version)));
                say(
                    &app,
                    UpdateMessage::DownloadFailed {
                        reason: rules::reason(&e),
                    },
                );
            }
        }
    });
}

/// Restart now (docs/08, "Install"): a problem found first shows "Cannot
/// install" and closes nothing; otherwise the closing steps run with
/// reason `update` and the install follows them. Only from `ready`.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let now = updates.current();
    let (Some(source), UpdateStateKind::Ready) = (updates.source(), now.state) else {
        return;
    };
    if let Some(problem) = source.install_problem() {
        tracing::warn!(target: "update", "cannot install: {problem}");
        say(app, UpdateMessage::CannotInstall { problem });
        return;
    }
    set(app, state(UpdateStateKind::Installing, now.version.clone()));
    tracing::info!(target: "update", "closing to install {}", now.version.unwrap_or_default());
    if !crate::closing::requested_for(app, crate::closing::Reason::Update) {
        // No close handshake (shell tests without one): install now.
        after_close(app);
    }
}

/// The user chose "Go back" at the close question: the install is called
/// off and the update stays ready.
pub fn called_off<R: Runtime>(app: &AppHandle<R>) {
    if let Some(updates) = app.try_state::<Updates>() {
        let now = updates.current();
        tracing::info!(target: "update", "the install was called off");
        set(app, state(UpdateStateKind::Ready, now.version));
    }
}

/// The closing steps have run (the album is closed): install. If it fails,
/// the current version starts again, the log says why, and the new start
/// shows "Install failed" ([`report_failed_install`]).
pub fn after_close<R: Runtime>(app: &AppHandle<R>) {
    let Some(source) = app.try_state::<Updates>().and_then(|u| u.source()) else {
        app.exit(0);
        return;
    };
    match source.install() {
        Ok(()) => app.exit(0),
        Err(e) => {
            tracing::error!(target: "update", "the install failed: {e}; starting this version again");
            if let Some(data) = app.try_state::<AppData>() {
                rules::record_install_failed(data.dir(), &e);
            }
            app.restart();
        }
    }
}

/// At `app_startup`: an install the copy before could not run is said now
/// ("Install failed"), once.
pub fn report_failed_install<R: Runtime>(app: &AppHandle<R>) {
    let Some(data) = app.try_state::<AppData>() else {
        return;
    };
    if let Some(reason) = rules::take_install_failed(data.dir()) {
        tracing::info!(target: "update", "telling the page the last install failed: {reason}");
        say(app, UpdateMessage::InstallFailed { reason });
    }
}

/// The More menu's update item was chosen: check, download or install, by state.
pub fn menu_chosen<R: Runtime>(app: &AppHandle<R>) {
    let now = app.try_state::<Updates>().map(|u| u.current().state);
    match now {
        Some(UpdateStateKind::Available) => download(app),
        Some(UpdateStateKind::Ready) => {
            let version = app
                .try_state::<Updates>()
                .and_then(|u| u.current().version)
                .unwrap_or_default();
            say(app, UpdateMessage::Downloaded { version });
        }
        _ => check(app, true),
    }
}

/// Tests the quiet check's rule and runs it when allowed.
pub fn quiet<R: Runtime>(app: &AppHandle<R>) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let Some(data) = app.try_state::<AppData>() else {
        return;
    };
    let Ok(settings) = data.settings() else {
        return;
    };
    let env_off = rules::env_set(std::env::var_os(rules::NO_CHECK_VAR).as_deref());
    if rules::quiet_allowed(
        updates.set_up(),
        env_off,
        &settings,
        crate::commands::album::now_ms(),
    ) {
        check(app, false);
    }
}

/// Starts the quiet check's clock: 10 seconds after `app_startup`, then
/// every 6 hours. Only once, and only when the updater is set up.
pub fn start_quiet<R: Runtime>(app: &AppHandle<R>) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    if !app.try_state::<Updates>().is_some_and(|u| u.set_up()) {
        return;
    }
    let app = app.clone();
    STARTED.call_once(move || {
        let _ = std::thread::Builder::new()
            .name("owlbum-update-clock".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(rules::QUIET_DELAY_MS));
                loop {
                    quiet(&app);
                    std::thread::sleep(std::time::Duration::from_millis(rules::QUIET_EVERY_MS));
                }
            });
    });
}

/// The real source: `tauri-plugin-updater` reading `feed`.
pub struct PluginSource<R: Runtime> {
    app: AppHandle<R>,
    feed: tauri::Url,
    offered: Mutex<Option<tauri_plugin_updater::Update>>,
    downloaded: Mutex<Option<Vec<u8>>>,
}

impl<R: Runtime> PluginSource<R> {
    /// A source reading `feed`. The plugin must be registered.
    pub fn new(app: AppHandle<R>, feed: tauri::Url) -> Self {
        Self {
            app,
            feed,
            offered: Mutex::new(None),
            downloaded: Mutex::new(None),
        }
    }
}

impl<R: Runtime> UpdateSource for PluginSource<R> {
    fn check(&self) -> Result<Option<String>, String> {
        use tauri_plugin_updater::UpdaterExt;
        let updater = self
            .app
            .updater_builder()
            .endpoints(vec![self.feed.clone()])
            .and_then(tauri_plugin_updater::UpdaterBuilder::build)
            .map_err(|e| e.to_string())?;
        match tauri::async_runtime::block_on(updater.check()) {
            Ok(Some(update)) => {
                let version = update.version.clone();
                *self.offered.lock().unwrap_or_else(PoisonError::into_inner) = Some(update);
                Ok(Some(version))
            }
            Ok(None) => Ok(None),
            // The plugin says this for every answer that is not a success:
            // asked again, only 404 means nothing is published yet.
            Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
                tauri::async_runtime::block_on(feed_status(&self.feed)).map(|()| None)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn download(&self, progress: &mut dyn FnMut(u8)) -> Result<(), String> {
        let update = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("no version is offered")?;
        let mut got: u64 = 0;
        let bytes = tauri::async_runtime::block_on(update.download(
            |chunk, total| {
                got += chunk as u64;
                if let Some(total) = total.filter(|t| *t > 0) {
                    let percent = (got.saturating_mul(100) / total).min(100);
                    progress(u8::try_from(percent).unwrap_or(100));
                }
            },
            || {},
        ))
        .map_err(|e| e.to_string())?;
        *self
            .downloaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(bytes);
        Ok(())
    }

    fn install_problem(&self) -> Option<String> {
        let have = self
            .downloaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some();
        (!have).then(|| "the downloaded update is gone.".to_owned())
    }

    fn install(&self) -> Result<(), String> {
        let update = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("no version is offered")?;
        let bytes = self
            .downloaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or("the downloaded update is gone.")?;
        update.install(bytes).map_err(|e| e.to_string())
    }
}

/// What the feed answers now, by its status alone (a GET, redirects
/// followed, as the plugin asks). Only after the plugin found no release.
async fn feed_status(feed: &tauri::Url) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("owlbum/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(feed.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    rules::feed_status(response.status().as_u16())
}

/// The public key in `plugins.updater.pubkey` of the configuration, or
/// `None` while it is empty: the updater is not set up until a person
/// makes the signing key (docs/08, "The signing key").
pub fn public_key(config: &tauri::Config) -> Option<String> {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
}
