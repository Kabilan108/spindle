mod engine;
mod model;
mod mpris;
mod tray;
mod ui;

use std::cell::Cell;

use rune::{Align, App, AppSpec, DisplayTarget, OverlayPlacement, ScreenEdge, WindowOptions, WindowSpec};

use engine::Engine;
use ui::mini::{MINI_SCENES, MINI_SIZE, MINI_TITLE, mini_key, mini_poll, mini_setup, mini_view};
use ui::{Link, MAIN_MIN, MAIN_SCENES, MAIN_SIZE, MAIN_TITLE, Main, Mini, main_key, main_poll, main_setup, main_view};

fn has_flag(flag: &str) -> bool {
    rune::extra_args().iter().any(|arg| arg == flag)
}

/// The one place the engine is built. Headless runs and `--demo` always get the demo engine;
/// the Spotify engine plugs in here for everything else.
fn build_engine() -> Engine {
    if rune::is_headless() || has_flag("--demo") {
        return Engine::demo();
    }
    #[cfg(feature = "spotify")]
    match Engine::spotify() {
        Ok(engine) => return engine,
        Err(error) => log::error!("can't start the Spotify engine, showing the demo instead: {error}"),
    }
    Engine::demo()
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    rune::set_app_id("spindle");
    rune::prewarm_gpu();
    let engine = build_engine();
    let link = Link::new(engine.clone());

    let mut app = App::new(AppSpec {
        min_size: Some(MAIN_MIN),
        scenes: MAIN_SCENES,
        setup: Some(main_setup),
        poll: Some(main_poll),
        on_key: Some(main_key),
        ..AppSpec::new(MAIN_TITLE, MAIN_SIZE, || unreachable!("built by App::init"), main_view)
    })
    .keep_running()
    .on_visibility(|s, visible, tx| {
        if visible {
            s.resume(tx);
        }
    })
    .on_focus(|s, focused, tx| {
        if focused {
            s.resume(tx);
        }
    });

    let mini_app = App::new(AppSpec {
        min_size: Some(MINI_SIZE),
        scenes: MINI_SCENES,
        setup: Some(mini_setup),
        poll: Some(mini_poll),
        on_key: Some(mini_key),
        ..AppSpec::new(MINI_TITLE, MINI_SIZE, || unreachable!("built by App::init"), mini_view)
    })
    .float_on_open()
    .on_close(|_, _| ui::mini::set_shown(false))
    .on_visibility(|s, visible, tx| {
        if visible {
            s.resume(tx);
        }
    });
    ui::mini::set_shown(has_flag("--mini"));
    let (main_handle, mini_handle) = (app.handle(), mini_app.handle());

    let mini_app = mini_app.init({
        let (link, me, main) = (link.clone(), mini_handle.clone(), main_handle.clone());
        move || {
            let wake = me.clone();
            link.subscribe(move || wake.update(|s: &mut Mini, tx| s.store_changed(tx)));
            Mini::new(link).with_handles(me, main)
        }
    });
    let mini_options = WindowOptions {
        borderless: true,
        always_on_top: true,
        visible: has_flag("--mini"),
        resizable: false,
        min_size: Some(MINI_SIZE),
        max_size: Some(MINI_SIZE),
        ..WindowOptions::default()
    };
    app.window(WindowSpec::from_app("mini", mini_app).options(mini_options));
    // X11 only: Wayland compositors place windows themselves.
    mini_handle.place(DisplayTarget::Primary, OverlayPlacement::Edge { edge: ScreenEdge::Bottom, margin: 56.0, align: Align::End, inset: 24.0 });

    let tray_started = Cell::new(false);
    let tray_parts = (link.clone(), main_handle.clone(), mini_handle.clone());
    let mpris_parts = (engine, main_handle.clone());
    let app = app
        .init({
            let (me, mini) = (main_handle, mini_handle);
            move || {
                let wake = me.clone();
                link.subscribe(move || wake.update(|s: &mut Main, tx| s.store_changed(tx)));
                Main::new(link).with_handles(me, mini)
            }
        })
        .on_ready(move |_| {
            if !tray_started.replace(true) && !rune::is_headless() {
                let (link, main, mini) = tray_parts.clone();
                tray::start(link, main, mini);
                let (engine, main) = mpris_parts.clone();
                let quit = main.clone();
                mpris::start(engine, move || main.open(), move || quit.quit());
            }
        });
    app.run();
}
