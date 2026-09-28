//! The window's icon (title bar, taskbar, Alt-Tab). Bevy has no setting for
//! it, so it goes to the winit window once that exists. The exe's own icon
//! on Windows is a resource (build.rs); both come from `icon/`.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_icon.run_if(not(resource_exists::<IconSet>)));
    }
}

#[derive(Resource)]
struct IconSet;

fn set_icon(
    mut commands: Commands,
    window: Single<Entity, With<PrimaryWindow>>,
    // winit windows may only be touched from the main thread.
    _main_thread: NonSendMarker,
) {
    let set = WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(*window) else {
            return false;
        };
        let image = image::load_from_memory(include_bytes!("../icon/window.png"))
            .expect("icon/window.png is a PNG")
            .into_rgba8();
        let (width, height) = image.dimensions();
        match winit::window::Icon::from_rgba(image.into_raw(), width, height) {
            Ok(icon) => window.set_window_icon(Some(icon)),
            Err(err) => warn!("window icon: {err}"),
        }
        true
    });
    if set {
        commands.insert_resource(IconSet);
    }
}
