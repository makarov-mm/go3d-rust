//! 3D Go: the game of Go on boards of 9, 13 and 19 lines, on OpenGL 4.6.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod board_geometry;
mod board_view;
mod camera;
mod engine;
mod gfx;
mod go;
mod math;
mod platform;
mod renderer;
mod settings;
mod shaders;
mod sound;
mod stone_model;
mod strings;
mod text;
mod themes;
mod ui;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = app::run() {
        platform::show_error("3D Go", &error.to_string());
        std::process::exit(1);
    }
}
