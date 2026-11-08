mod audio;
mod beats;
mod empty;
mod eq;
mod keys;
mod library;
mod menu;
mod media;
mod playlist;
mod playlists;
mod queue;
mod regions;
mod resume;
mod scan;
mod session;
mod speed;
mod state;
mod theme;
mod trainer;
mod ui;
mod variations;
mod view;
mod wave;

use gpui::{
    App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, prelude::*, px,
    size,
};

fn main() {
    gpui_platform::application()
        .with_assets(ui::Assets)
        // On macOS GPUI keeps running without a window; odeion has no menu bar or other
        // way back, so closing the window quits (the window saves first, see `view`).
        .with_quit_mode(gpui::QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            ui::load_fonts(cx);
            // Where the window was left, if that display is still connected.
            let settings = state::Settings::load(&view::settings_file());
            let saved = settings.window.and_then(|w| {
                let display = cx
                    .displays()
                    .into_iter()
                    .find(|d| d.uuid().is_ok_and(|u| u.to_string() == w.display))?;
                let bounds = Bounds::new(point(px(w.x), px(w.y)), size(px(w.width), px(w.height)));
                Some((display.id(), bounds))
            });
            let (display_id, bounds) = match saved {
                Some((id, bounds)) => (Some(id), bounds),
                None => (None, Bounds::centered(None, size(px(1000.), px(860.)), cx)),
            };
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    // On a Mac the theme's background runs up behind the traffic lights;
                    // elsewhere the system draws its own title bar and buttons.
                    titlebar: Some(TitlebarOptions {
                        title: Some("Odeion".into()),
                        appears_transparent: cfg!(target_os = "macos"),
                        // Centred in odeion's 40 px title bar (the lights are 12 px).
                        traffic_light_position: Some(point(px(14.), px(14.))),
                    }),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| view::Odeion::new(window, cx)),
            )
            .unwrap();
            cx.activate(true);
        });
}
