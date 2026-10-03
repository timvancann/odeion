mod audio;
mod beats;
mod keys;
mod library;
mod media;
mod playlist;
mod regions;
mod resume;
mod scan;
mod state;
mod theme;
mod ui;
mod variations;
mod view;
mod wave;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, prelude::*, px,
    size,
};

fn main() {
    Application::new()
        .with_assets(ui::Assets)
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
                    // The theme's background runs up behind the traffic lights.
                    titlebar: Some(TitlebarOptions {
                        title: Some("riff".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| view::Riff::new(window, cx)),
            )
            .unwrap();
            cx.activate(true);
        });
}
