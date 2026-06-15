use bevy::prelude::*;
use ratatui::{backend::TestBackend, style::Color as RColor, Terminal};

// Character grid size of the TUI panel (top-left corner).
pub const TUI_COLS: u16 = 80;
pub const TUI_ROWS: u16 = 30;
// Popup buffer size (rendered centered over the full Bevy window).
pub const POPUP_COLS: u16 = 72;
pub const POPUP_ROWS: u16 = 24;
pub const FONT_SIZE: f32 = 14.0;

// ── Resources ────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct TuiFont(pub Handle<Font>);

#[derive(Resource)]
pub struct TuiRows(pub Vec<Entity>);

/// Main TUI context (no real terminal – uses TestBackend).
#[derive(Resource)]
pub struct TuiContext(pub Terminal<TestBackend>);

impl TuiContext {
    pub fn new(full_cols: u16, full_rows: u16) -> Self {
        let backend = TestBackend::new(full_cols, full_rows);
        let terminal = Terminal::new(backend).expect("Failed to create Ratatui terminal");
        TuiContext(terminal)
    }
}

/// Separate Ratatui context for popups, displayed centered over the full window.
#[derive(Resource)]
pub struct PopupTuiContext(pub Terminal<TestBackend>);

impl PopupTuiContext {
    pub fn new() -> Self {
        let backend = TestBackend::new(POPUP_COLS, POPUP_ROWS);
        let terminal = Terminal::new(backend).expect("Failed to create popup Ratatui terminal");
        PopupTuiContext(terminal)
    }
}

#[derive(Resource)]
pub struct PopupTuiRows(pub Vec<Entity>);

/// Marks the full-screen backdrop node used for popups.
#[derive(Component)]
pub struct PopupOverlayMarker;

// ── Plugin ────────────────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(
        Startup,
        (
            load_font,
            setup_main_overlay.after(load_font),
            setup_popup_overlay.after(load_font),
        ),
    )
    .add_systems(
        PostUpdate,
        (update_main_overlay_from_buffer, update_popup_overlay_from_buffer),
    );
}

// ── Startup systems ───────────────────────────────────────────────────────────

fn load_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(TuiFont(asset_server.load("fonts/mono.ttf")));
}

fn setup_main_overlay(mut commands: Commands, _font: Res<TuiFont>) {
    let root = commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(0.),
                top: Val::Px(0.),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                ..default()
            },
            z_index: ZIndex::Global(100),
            ..default()
        })
        .id();

    let mut rows = Vec::with_capacity(TUI_ROWS as usize);
    for _ in 0..TUI_ROWS {
        let row = spawn_text_row(&mut commands);
        commands.entity(root).add_child(row);
        rows.push(row);
    }
    commands.insert_resource(TuiRows(rows));
}

fn setup_popup_overlay(mut commands: Commands, _font: Res<TuiFont>) {
    commands.insert_resource(PopupTuiContext::new());

    // Full-screen semi-transparent backdrop, starts hidden.
    let overlay = commands
        .spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                background_color: Color::srgba(0., 0., 0., 0.75).into(),
                z_index: ZIndex::Global(200),
                visibility: Visibility::Hidden,
                ..default()
            },
            PopupOverlayMarker,
        ))
        .id();

    let mut rows = Vec::with_capacity(POPUP_ROWS as usize);
    for _ in 0..POPUP_ROWS {
        let row = spawn_text_row(&mut commands);
        commands.entity(overlay).add_child(row);
        rows.push(row);
    }
    commands.insert_resource(PopupTuiRows(rows));
}

fn spawn_text_row(commands: &mut Commands) -> Entity {
    commands
        .spawn(TextBundle {
            text: Text::from_sections([]),
            style: Style {
                flex_shrink: 0.,
                height: Val::Px(FONT_SIZE * 1.2),
                ..default()
            },
            ..default()
        })
        .id()
}

// ── Per-frame update systems ──────────────────────────────────────────────────

fn update_main_overlay_from_buffer(
    tui_ctx: Option<Res<TuiContext>>,
    tui_rows: Option<Res<TuiRows>>,
    font: Option<Res<TuiFont>>,
    mut query: Query<&mut Text>,
) {
    let (Some(ctx), Some(rows), Some(font)) = (tui_ctx, tui_rows, font) else {
        return;
    };
    let buffer = ctx.0.backend().buffer();
    let buf_width = buffer.area.width;
    let row_cap = TUI_COLS.min(buf_width);

    for (row_idx, &entity) in rows.0.iter().enumerate() {
        let Ok(mut text) = query.get_mut(entity) else {
            continue;
        };
        let start = row_idx as u16 * buf_width;
        let end = start + row_cap;
        if end as usize > buffer.content.len() {
            text.sections.clear();
            continue;
        }
        fill_text_sections(&mut text, &buffer.content[start as usize..end as usize], &font.0);
    }
}

fn update_popup_overlay_from_buffer(
    popup_ctx: Option<Res<PopupTuiContext>>,
    popup_rows: Option<Res<PopupTuiRows>>,
    font: Option<Res<TuiFont>>,
    mut query: Query<&mut Text>,
) {
    let (Some(ctx), Some(rows), Some(font)) = (popup_ctx, popup_rows, font) else {
        return;
    };
    let buffer = ctx.0.backend().buffer();
    let buf_width = buffer.area.width;

    for (row_idx, &entity) in rows.0.iter().enumerate() {
        let Ok(mut text) = query.get_mut(entity) else {
            continue;
        };
        let start = row_idx as u16 * buf_width;
        let end = (start + buf_width) as usize;
        if end > buffer.content.len() {
            text.sections.clear();
            continue;
        }
        fill_text_sections(&mut text, &buffer.content[start as usize..end], &font.0);
    }
}

fn fill_text_sections(text: &mut Text, cells: &[ratatui::buffer::Cell], font: &Handle<Font>) {
    text.sections.clear();
    let mut current_color: Option<RColor> = None;
    let mut current_str = String::new();

    for cell in cells {
        let fg = cell.style().fg.unwrap_or(RColor::Reset);
        if Some(fg) != current_color && !current_str.is_empty() {
            push_section(text, &current_str, current_color.unwrap_or(RColor::Reset), font);
            current_str.clear();
        }
        current_color = Some(fg);
        current_str.push_str(cell.symbol());
    }
    if !current_str.is_empty() {
        push_section(text, &current_str, current_color.unwrap_or(RColor::Reset), font);
    }
}

fn push_section(text: &mut Text, value: &str, color: RColor, font: &Handle<Font>) {
    text.sections.push(TextSection {
        value: value.to_string(),
        style: TextStyle {
            font: font.clone(),
            font_size: FONT_SIZE,
            color: ratatui_to_bevy_color(color),
        },
    });
}

// ── Color conversion ──────────────────────────────────────────────────────────

pub fn ratatui_to_bevy_color(color: RColor) -> Color {
    match color {
        RColor::Reset | RColor::White => Color::WHITE,
        RColor::Black => Color::srgb(0.1, 0.1, 0.1),
        RColor::Red => Color::srgb(0.8, 0.2, 0.2),
        RColor::LightRed => Color::srgb(1.0, 0.4, 0.4),
        RColor::Green => Color::srgb(0.2, 0.8, 0.2),
        RColor::LightGreen => Color::srgb(0.4, 1.0, 0.4),
        RColor::Yellow => Color::srgb(0.8, 0.8, 0.2),
        RColor::LightYellow => Color::srgb(1.0, 1.0, 0.4),
        RColor::Blue => Color::srgb(0.2, 0.4, 0.9),
        RColor::LightBlue => Color::srgb(0.4, 0.6, 1.0),
        RColor::Magenta => Color::srgb(0.8, 0.2, 0.8),
        RColor::LightMagenta => Color::srgb(1.0, 0.4, 1.0),
        RColor::Cyan => Color::srgb(0.2, 0.8, 0.8),
        RColor::LightCyan => Color::srgb(0.4, 1.0, 1.0),
        RColor::Gray => Color::srgb(0.6, 0.6, 0.6),
        RColor::DarkGray => Color::srgb(0.35, 0.35, 0.35),
        RColor::Rgb(r, g, b) => {
            Color::srgb(r as f32 / 255., g as f32 / 255., b as f32 / 255.)
        }
        RColor::Indexed(n) => ansi256_to_bevy(n),
    }
}

fn ansi256_to_bevy(n: u8) -> Color {
    let (r, g, b) = match n {
        0 => (0u8, 0u8, 0u8),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            let i = n - 16;
            let b = (i % 6) * 51;
            let g = ((i / 6) % 6) * 51;
            let r = (i / 36) * 51;
            (r, g, b)
        }
        232..=255 => {
            let v = (n - 232) * 10 + 8;
            (v, v, v)
        }
    };
    Color::srgb(r as f32 / 255., g as f32 / 255., b as f32 / 255.)
}
