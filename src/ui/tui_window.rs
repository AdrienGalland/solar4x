use bevy::{
    prelude::*,
    render::camera::{ClearColorConfig, RenderTarget},
    window::{WindowRef, WindowResized},
};

use crate::ui::tui_config::{TuiWindowMode, TuiWindowSettings};
use crate::ui::tui_overlay::{
    MainOverlayRoot, PopupOverlayRoot, TuiWindowFontSize, FONT_H_RATIO, FONT_SIZE, FONT_W_RATIO,
    TUI_COLS, TUI_ROWS,
};

#[derive(Component)]
pub struct TuiWindowMarker;

#[derive(Component)]
pub struct TuiCameraMarker;

#[derive(Resource, Default)]
pub struct TuiWindowState {
    pub window_entity: Option<Entity>,
    pub camera_entity: Option<Entity>,
}

pub struct TuiWindowPlugin;

impl Plugin for TuiWindowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TuiWindowState>().add_systems(
            Update,
            (
                apply_placement_mode.run_if(resource_changed::<TuiWindowSettings>),
                handle_tui_window_resize,
            ),
        );
    }
}

/// Compute the font size that fills a window of given pixel dimensions with the TUI grid.
fn fit_font_size(window_w: f32, window_h: f32) -> f32 {
    let fs_w = window_w / (TUI_COLS as f32 * FONT_W_RATIO);
    let fs_h = window_h / (TUI_ROWS as f32 * FONT_H_RATIO);
    fs_w.min(fs_h)
}

fn apply_placement_mode(
    settings: Res<TuiWindowSettings>,
    mut state: ResMut<TuiWindowState>,
    mut commands: Commands,
    main_root: Option<Res<MainOverlayRoot>>,
    popup_root: Option<Res<PopupOverlayRoot>>,
    camera3d: Query<Entity, With<Camera3d>>,
) {
    match settings.get_mode() {
        TuiWindowMode::Integrated => {
            // Retarget TUI roots to the Camera3d before despawning the TUI camera so UI
            // is never orphaned (removing TargetCamera is ambiguous mid-flush).
            if let Ok(cam3d) = camera3d.get_single() {
                if let Some(main) = &main_root {
                    commands.entity(main.0).insert((
                        TargetCamera(cam3d),
                        // Restore original top-left layout for the integrated overlay.
                        Style {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.),
                            top: Val::Px(0.),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::FlexStart,
                            ..default()
                        },
                    ));
                }
                if let Some(popup) = &popup_root {
                    commands.entity(popup.0).insert(TargetCamera(cam3d));
                }
            }
            if let Some(cam) = state.camera_entity.take() {
                commands.entity(cam).despawn_recursive();
            }
            if let Some(win) = state.window_entity.take() {
                commands.entity(win).despawn_recursive();
            }
            commands.insert_resource(TuiWindowFontSize(FONT_SIZE));
        }
        TuiWindowMode::SeparateWindow => {
            if state.window_entity.is_some() {
                return;
            }
            let config = settings.get_config();
            let font_size = fit_font_size(config.window_width, config.window_height);

            let window_entity = commands
                .spawn((
                    Window {
                        title: "Solar4X — TUI".to_string(),
                        resolution: bevy::window::WindowResolution::new(
                            config.window_width,
                            config.window_height,
                        ),
                        ..default()
                    },
                    TuiWindowMarker,
                ))
                .id();

            // order: -1 keeps Camera3d (order 0) as the default for untagged UI.
            let camera_entity = commands
                .spawn((
                    Camera2dBundle {
                        camera: Camera {
                            target: RenderTarget::Window(WindowRef::Entity(window_entity)),
                            clear_color: ClearColorConfig::Custom(Color::BLACK),
                            order: -1,
                            ..default()
                        },
                        ..default()
                    },
                    TuiCameraMarker,
                ))
                .id();

            state.window_entity = Some(window_entity);
            state.camera_entity = Some(camera_entity);

            if let Some(main) = &main_root {
                commands.entity(main.0).insert((
                    TargetCamera(camera_entity),
                    // Fill the window and center the text block so any rounding
                    // discrepancy doesn't leave content stuck in a corner.
                    Style {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.),
                        top: Val::Px(0.),
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ));
            }
            if let Some(popup) = &popup_root {
                commands
                    .entity(popup.0)
                    .insert(TargetCamera(camera_entity));
            }
            commands.insert_resource(TuiWindowFontSize(font_size));
        }
    }
}

/// Recompute font size when the TUI window is resized so the text keeps filling it.
fn handle_tui_window_resize(
    mut resize_events: EventReader<WindowResized>,
    state: Res<TuiWindowState>,
    mut fs_res: ResMut<TuiWindowFontSize>,
) {
    let Some(win_entity) = state.window_entity else {
        resize_events.clear();
        return;
    };
    for ev in resize_events.read() {
        if ev.window == win_entity && ev.width > 0.0 && ev.height > 0.0 {
            fs_res.0 = fit_font_size(ev.width, ev.height);
        }
    }
}
