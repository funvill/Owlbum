//! The window's geometry (docs/08-desktop-and-release.md, "Geometry"): it
//! is restored from the `window` setting when the window is made, followed
//! as the window moves and is resized, written 2 seconds after the last
//! change, and written again when the window closes.
//!
//! The bounds are the window's outer position and inner size, in physical
//! pixels, as Tauri sets them back.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use owlbum_core::appdata::AppData;
use owlbum_core::appdata::settings::WindowSetting;
use serde_json::json;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// The size of a window with no saved size.
pub const DEFAULT_SIZE: (u32, u32) = (1440, 900);
/// The smallest the window may be.
pub const MIN_SIZE: (u32, u32) = (960, 600);
/// How long after the last move or resize the bounds are written.
pub const SAVE_DELAY: Duration = Duration::from_secs(2);

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(&self, x: i32, y: i32) -> bool {
        let (x, y) = (i64::from(x), i64::from(y));
        let (left, top) = (i64::from(self.x), i64::from(self.y));
        x >= left
            && x < left + i64::from(self.width)
            && y >= top
            && y < top + i64::from(self.height)
    }
}

/// A display: its whole area and its work area (without the task bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    pub area: Rect,
    pub work: Rect,
}

/// Where and how big the window opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub width: u32,
    pub height: u32,
    /// The top-left corner, or None to centre it on the primary display.
    pub position: Option<(i32, i32)>,
    pub maximised: bool,
}

/// The off-screen check: the title bar can be reached on display `d`.
pub fn reachable(x: i32, y: i32, width: u32, d: &Rect) -> bool {
    let (x, y, width) = (i64::from(x), i64::from(y), i64::from(width));
    let (dx, dy, dw, dh) = (
        i64::from(d.x),
        i64::from(d.y),
        i64::from(d.width),
        i64::from(d.height),
    );
    x + width > dx + 100 && x < dx + dw - 100 && y >= dy - 10 && y < dy + dh - 50
}

/// Works out the window's placement from the saved setting and the
/// displays (the primary one first). The rules are the table "Restoring".
pub fn restore(saved: &WindowSetting, displays: &[Display]) -> Placement {
    let maximised = saved.maximised;
    let (Some(x), Some(y), Some(width), Some(height)) =
        (saved.x, saved.y, saved.width, saved.height)
    else {
        let (width, height) = fit(DEFAULT_SIZE, displays.first());
        return Placement {
            width,
            height,
            position: None,
            maximised,
        };
    };
    let lands_on = displays
        .iter()
        .find(|d| d.area.contains(x, y))
        .or_else(|| displays.first());
    let (width, height) = fit((width, height), lands_on);
    let shown = displays.iter().any(|d| reachable(x, y, width, &d.area));
    Placement {
        width,
        height,
        position: shown.then_some((x, y)),
        maximised,
    }
}

/// Raises a size to the minimum, then reduces it to the display's work area.
fn fit((width, height): (u32, u32), display: Option<&Display>) -> (u32, u32) {
    let (mut w, mut h) = (width.max(MIN_SIZE.0), height.max(MIN_SIZE.1));
    if let Some(d) = display {
        if d.work.width > 0 {
            w = w.min(d.work.width);
        }
        if d.work.height > 0 {
            h = h.min(d.work.height);
        }
    }
    (w, h)
}

fn displays<R: Runtime>(window: &WebviewWindow<R>) -> Vec<Display> {
    let rect = |pos: &PhysicalPosition<i32>, size: &PhysicalSize<u32>| Rect {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    };
    let of = |m: &tauri::Monitor| Display {
        area: rect(m.position(), m.size()),
        work: rect(&m.work_area().position, &m.work_area().size),
    };
    let primary = window.primary_monitor().ok().flatten();
    let mut all: Vec<Display> = primary.iter().map(of).collect();
    for m in window.available_monitors().unwrap_or_default() {
        let d = of(&m);
        if !all.contains(&d) {
            all.push(d);
        }
    }
    all
}

/// Places the hidden window as the `window` setting says.
pub fn place<R: Runtime>(window: &WebviewWindow<R>, saved: &WindowSetting) {
    let placement = restore(saved, &displays(window));
    let _ = window.set_size(PhysicalSize::new(placement.width, placement.height));
    match placement.position {
        Some((x, y)) => {
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
        None => {
            let _ = window.center();
        }
    }
    if placement.maximised {
        let _ = window.maximize();
    }
}

/// The bounds being followed, and when they last changed.
#[derive(Debug, Default)]
pub struct Geometry {
    state: Mutex<Followed>,
}

#[derive(Debug, Default)]
struct Followed {
    /// The bounds to write, once the window has moved or been resized.
    bounds: Option<WindowSetting>,
    last_change: Option<Instant>,
    /// A writer thread waits for the changes to settle.
    waiting: bool,
}

impl Geometry {
    fn lock(&self) -> std::sync::MutexGuard<'_, Followed> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The window moved or was resized: follow its bounds, and write them
/// [`SAVE_DELAY`] after the last change. Bounds are taken only while the
/// window is neither maximised, minimised nor full screen; maximising
/// keeps the last plain bounds and records `maximised`.
pub fn changed<R: Runtime>(window: &tauri::Window<R>) {
    let app = window.app_handle();
    let Some(geometry) = app.try_state::<Geometry>() else {
        return;
    };
    if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let maximised = window.is_maximized().unwrap_or(false);
    let mut f = geometry.lock();
    let mut bounds = f.bounds.unwrap_or_else(|| {
        app.try_state::<AppData>()
            .and_then(|d| d.settings().ok())
            .map(|s| s.window)
            .unwrap_or_default()
    });
    bounds.maximised = maximised;
    if !maximised && let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
        bounds.x = Some(pos.x);
        bounds.y = Some(pos.y);
        bounds.width = Some(size.width);
        bounds.height = Some(size.height);
    }
    f.bounds = Some(bounds);
    f.last_change = Some(Instant::now());
    if f.waiting {
        return;
    }
    f.waiting = true;
    drop(f);
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("owlbum-geometry".into())
        .spawn(move || {
            loop {
                let wait = {
                    let geometry = app.state::<Geometry>();
                    let f = geometry.lock();
                    let since = f.last_change.map_or(SAVE_DELAY, |t| t.elapsed());
                    SAVE_DELAY.saturating_sub(since)
                };
                if wait.is_zero() {
                    break;
                }
                std::thread::sleep(wait);
            }
            app.state::<Geometry>().lock().waiting = false;
            save(&app);
        });
}

/// Writes the followed bounds, if the window moved or was resized since
/// the last write. Closing calls it as its step 3.
pub fn save<R: Runtime>(app: &AppHandle<R>) {
    let Some(geometry) = app.try_state::<Geometry>() else {
        return;
    };
    let Some(bounds) = geometry.lock().bounds.take() else {
        return;
    };
    let Some(data) = app.try_state::<AppData>() else {
        return;
    };
    let patch = json!({ "window": bounds });
    if let Err(e) = data.set_settings(patch.as_object().expect("an object")) {
        tracing::warn!(target: "app", "the window's place could not be saved: {}", e.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn display(x: i32, y: i32, width: u32, height: u32) -> Display {
        Display {
            area: rect(x, y, width, height),
            work: rect(x, y, width, height - 40),
        }
    }

    fn saved(
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        max: bool,
    ) -> WindowSetting {
        WindowSetting {
            x,
            y,
            width: w,
            height: h,
            maximised: max,
        }
    }

    const HD: fn() -> Vec<Display> = || vec![display(0, 0, 1920, 1080)];

    #[test]
    fn the_default_setting_opens_the_default_size_centred() {
        let p = restore(&WindowSetting::default(), &HD());
        assert_eq!(
            p,
            Placement {
                width: 1440,
                height: 900,
                position: None,
                maximised: false
            }
        );
    }

    #[test]
    fn a_missing_number_opens_the_default_size_centred_but_keeps_maximised() {
        let p = restore(&saved(Some(10), None, Some(1000), Some(700), true), &HD());
        assert_eq!(
            (p.width, p.height, p.position, p.maximised),
            (1440, 900, None, true)
        );
    }

    #[test]
    fn a_good_setting_is_used_as_it_is() {
        let p = restore(
            &saved(Some(100), Some(50), Some(1200), Some(800), false),
            &HD(),
        );
        assert_eq!(
            (p.width, p.height, p.position),
            (1200, 800, Some((100, 50)))
        );
    }

    #[test]
    fn a_small_size_is_raised_to_the_minimum() {
        let p = restore(&saved(Some(0), Some(0), Some(300), Some(200), false), &HD());
        assert_eq!((p.width, p.height), MIN_SIZE);
    }

    #[test]
    fn a_large_size_is_reduced_to_the_work_area_of_its_display() {
        let displays = vec![display(0, 0, 1920, 1080), display(1920, 0, 1280, 1024)];
        let p = restore(
            &saved(Some(2000), Some(0), Some(3000), Some(2000), false),
            &displays,
        );
        assert_eq!((p.width, p.height), (1280, 984));
        assert_eq!(p.position, Some((2000, 0)));
    }

    #[test]
    fn an_unreachable_position_keeps_the_size_and_centres() {
        // On a display that was unplugged.
        let p = restore(
            &saved(Some(-1900), Some(10), Some(1200), Some(800), false),
            &HD(),
        );
        assert_eq!((p.width, p.height, p.position), (1200, 800, None));
        // The title bar above the top.
        let p = restore(
            &saved(Some(100), Some(-20), Some(1200), Some(800), false),
            &HD(),
        );
        assert_eq!(p.position, None);
        // Reachable on a second display to the left.
        let displays = vec![display(0, 0, 1920, 1080), display(-1920, 0, 1920, 1080)];
        let p = restore(
            &saved(Some(-1900), Some(10), Some(1200), Some(800), true),
            &displays,
        );
        assert_eq!((p.position, p.maximised), (Some((-1900, 10)), true));
    }

    #[test]
    fn the_off_screen_check_follows_the_spec() {
        let d = rect(0, 0, 1920, 1080);
        assert!(reachable(0, 0, 1000, &d));
        assert!(
            !reachable(-900, 0, 1000, &d),
            "only 100 px or less shows on the left"
        );
        assert!(reachable(-899, 0, 1000, &d));
        assert!(!reachable(1820, 0, 1000, &d));
        assert!(reachable(1819, 0, 1000, &d));
        assert!(reachable(0, -10, 1000, &d));
        assert!(!reachable(0, -11, 1000, &d));
        assert!(!reachable(0, 1030, 1000, &d));
        assert!(reachable(0, 1029, 1000, &d));
    }

    #[test]
    fn with_no_display_known_only_the_minimum_applies() {
        let p = restore(&saved(Some(0), Some(0), Some(5000), Some(5000), false), &[]);
        assert_eq!((p.width, p.height, p.position), (5000, 5000, None));
    }
}
