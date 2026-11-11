mod engine;
mod model;
mod mpris;
mod tray;
mod ui;

use std::cell::Cell;

use rune::{App, AppSpec};

use engine::Engine;
use ui::{Link, MAIN_MIN, MAIN_SCENES, MAIN_SIZE, MAIN_TITLE, Main, main_key, main_poll, main_setup, main_view};

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
    rune::set_app_id("baliset");
    rune::prewarm_gpu();
    let engine = build_engine();
    let link = Link::new(engine.clone());

    let app = App::new(AppSpec {
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

    let main_handle = app.handle();

    let tray_started = Cell::new(false);
    let tray_parts = (link.clone(), main_handle.clone());
    let mpris_parts = (engine, main_handle.clone());
    let app = app
        .init({
            let me = main_handle;
            move || {
                let wake = me.clone();
                link.subscribe(move || wake.update(|s: &mut Main, tx| s.store_changed(tx)));
                Main::new(link).with_handle(me)
            }
        })
        .on_ready(move |_| {
            if !tray_started.replace(true) && !rune::is_headless() {
                let (link, main) = tray_parts.clone();
                tray::start(link, main);
                let (engine, main) = mpris_parts.clone();
                let quit = main.clone();
                mpris::start(engine, move || main.open(), move || quit.quit());
            }
        });
    app.run();
}
