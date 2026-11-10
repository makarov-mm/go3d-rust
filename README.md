# 3D Go — Rust and OpenGL 4.6

[![GitHub Sponsors](https://img.shields.io/github/sponsors/makarov-mm?style=flat&logo=github)](https://github.com/sponsors/makarov-mm)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-2024-DEA584?logo=rust&logoColor=white)
![OpenGL](https://img.shields.io/badge/OpenGL-4.6-5586A4?logo=opengl&logoColor=white)
![Windows](https://img.shields.io/badge/Windows-0078D6?logo=windows&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black)
[![LinkedIn](https://img.shields.io/badge/LinkedIn-0A66C2?logo=linkedin&logoColor=white)](https://www.linkedin.com/in/makarov-mm/)
[![Threads](https://img.shields.io/badge/Threads-000000?logo=threads&logoColor=white)](https://www.threads.net/@m.m.makarov)
[![Instagram](https://img.shields.io/badge/Instagram-E4405F?logo=instagram&logoColor=white)](https://www.instagram.com/m.m.makarov/)

A 3D game of Go written in Rust on OpenGL 4.6: the Rust version of the C++ game, with the same rules, computer player, graphics and interface.

There is no game engine, and no model, texture or sound file: the stones, the bowls and their lids are turned on a lathe in code at startup, the wood, slate, shell and glass are procedural, the lighting environment is baked on the GPU, and the sounds are synthesized. The shaders are built into the executable, so the game is a single file.

![Screenshot](screenshot.png)

![Themes](themes.png)

## Features

### Rules

- Boards of 9×9, 13×13 and 19×19, with handicaps of 2 to 9 stones (5 on 9×9) on the usual points
- Captures, the ko rule and positional superko; suicide is not allowed
- Area (Chinese) scoring with komi 7.5, or 0.5 with a handicap; two passes in a row end the game
- Counting: the dead stones are found by the computer and can be corrected with a click on a group; the territory is marked with squares, and the score is kept up to date. The count is accepted, or the game is played on
- Resignation, with a second click to confirm
- The points are named as on a printed board: columns `A`–`T` without `I`, rows from 1 at Black's side

### Play

- A computer opponent with five levels, from Novice to Master: Monte Carlo tree search with RAVE and priors, playouts that answer ataris, take stones, follow 3×3 shapes and leave eyes alone, several threads, and the empty corners first on the big boards. It passes when nothing is left to gain, and resigns a hopeless game. It thinks on threads of its own, so the window stays responsive
- Play Black, play White, two players on one computer, or watch a demo game
- A see-through stone shows where a click would play; the stones it would take rise a little and glow. A point where ko forbids a retake is marked
- Undo and redo, the move list with the stones each move took, the prisoners of each side
- Hints, with the suggested point ringed; an estimate of the score, with the territory and the stones that look dead shown on the board
- Copy the game as SGF and paste an SGF game to continue it (its main line, with the size, komi, handicap and setup stones)
- The game in progress and the settings are kept between sessions
- English and Russian interface, chosen from the system language
- Sound effects, full screen, awareness of the screen's scale, a dark title bar
- No work while nothing moves: the game waits for input instead of drawing the same picture

### Graphics

- Physically based shading: GGX highlights, a clear lacquer coat, light passing through thin stone edges, image-based lighting from a procedural studio environment
- Soft shadows (PCSS), the shade round stones on the board and round the feet of the bowls, and the stones reflected in the polished boards
- HDR rendering with 4× MSAA, bloom, ACES tone mapping, vignette and dithering
- Smooth edges everywhere: multisampling for the outlines, and the lines, the hoshi, the coordinates and the marks drawn by the shaders filtered over each pixel; highlights widen where the surface curves within a pixel, so they do not sparkle
- Stones shaped as Japanese stones are cut, both faces domed, drawn hundreds at once with instancing; each has its own grain, so no two look alike
- Animation: a stone rises from the player's bowl and flies to its point, then the stones it takes fly into the lid of the player who took them; a move taken back runs the other way, and a new game dissolves in
- Four themes: Kaya with slate and shell stones, Bamboo with Chinese yunzi stones, Glass, and Neon
- An interface of frosted-glass panels, sharp at any screen scale

## Controls

| Action | Control |
|---|---|
| Place a stone; mark a group dead or alive when counting | Click a point |
| Turn the view | Drag with either mouse button |
| Zoom | Mouse wheel |
| Take back / replay a move | `Ctrl+Z` / `Ctrl+Y`, `←` / `→`, `Backspace` |
| Pass | `P` |
| Hint | `H` |
| Estimate the score | `E` |
| Accept the count | `Enter` |
| New game | `N` |
| Flip the board | `F` |
| View from above | `V` |
| Reset the view | `Home` |
| Computer level | `1` … `5` |
| Who plays whom | `M` (`Shift+M` back) |
| Board size | `B` (`Shift+B` back) |
| Coordinates on / off | `C` |
| Next theme | `T` (`Shift+T` back) |
| Sound on / off | `S` |
| English / Russian | `L` |
| Show / hide the panel | `Tab` |
| Full screen | `F11`, `Alt+Enter` |
| Copy the game (SGF) | `Ctrl+C` |
| Paste a game (SGF) | `Ctrl+V` |
| Help | `F1` |
| Leave full screen | `Esc` |

The board size and the handicap apply at once to a game not yet begun or already over, and otherwise to the next game.

## Requirements

- Windows 10 or 11, or Linux (X11 or Wayland), x64
- A graphics card and driver with OpenGL 4.6 (4.5 is enough)
- Rust 1.85 or later (the 2024 edition); on Linux also the ALSA development files (`libasound2-dev` on Debian and Ubuntu)

## Building and running

```sh
cargo run --release
```

The executable is `target/release/go3d` (`go3d.exe` on Windows) and runs on its own. The settings and the game in progress are kept in `Go3D.ini`, in `%APPDATA%\Go3D` on Windows and in `$XDG_CONFIG_HOME/Go3D` (`~/.config/Go3D`) on Linux.

## Project structure

```text
.
├── assets/                Go3D.ico
├── shaders/               GLSL shaders, built into the executable
├── src/
│   ├── main.rs            entry point
│   ├── app.rs             game flow, input and interface
│   ├── app/
│   │   ├── window.rs      the window, its events and the frame loop
│   │   └── widgets.rs     the panel, buttons, selectors and notes of the interface
│   ├── go.rs              the rules: strings and liberties kept move by move, ko and superko, scoring, SGF
│   ├── engine.rs          the computer player: Monte Carlo tree search, playouts and the estimate of the score
│   ├── stone_model.rs     the stone, turned on a lathe
│   ├── board_geometry.rs  the layout for each size, and meshes of the board, the table, the bowls and the lids
│   ├── board_view.rs      the stones on the board, in the bowls and in the lids, and their animation
│   ├── camera.rs          orbit camera and mouse rays
│   ├── renderer.rs        the rendering pipeline
│   ├── themes.rs          materials and lighting of the themes
│   ├── gfx.rs             OpenGL objects and calls behind safe wrappers
│   ├── text.rs            font atlases and the board's coordinates drawn from the system's fonts
│   ├── ui.rs              drawing of the interface
│   ├── sound.rs           synthesized sound effects
│   ├── settings.rs        the settings file
│   ├── strings.rs         English and Russian texts
│   ├── shaders.rs         the shader sources built into the executable
│   ├── platform.rs        the window's OpenGL context, the settings folder, the system language, the icon
│   └── math.rs            vectors and matrices
├── build.rs               the icon and version information of the Windows executable
└── Cargo.toml
```

## Dependencies

| Crate | For |
|---|---|
| `winit` | the window and its input |
| `glutin`, `glutin-winit`, `raw-window-handle` | the OpenGL context |
| `gl` | the OpenGL functions |
| `fontdb`, `fontdue` | finding the system's fonts and drawing their glyphs |
| `cpal` | sound output |
| `arboard` | the clipboard |
| `sys-locale` | the system language |
| `png` (Linux) | the window's icon |
| `winresource` (build) | the icon and version information of the Windows executable |

## How a frame is drawn

1. **Shadow map** — the board, the bowls and the stones seen from the key light, for soft shadows; the stones in one instanced draw.
2. **Reflection** — the stones over the board mirrored under it, drawn at half resolution and blurred into mipmaps, so the polish of each theme can show a sharper or softer reflection.
3. **Main pass** — the table, the board, the bowls and the stones in HDR with 4× MSAA, lit by the key light and the baked environment. The lines, the hoshi, the coordinates and the marks of each point (territory, ko, hint) are drawn by the board's shader from a small texture of the points; the stones are drawn in one instanced call for each color, and the see-through ones, sorted, last.
4. **Post-processing** — the samples are resolved, bright parts spread into a six-level bloom, and the composite pass applies tone mapping and vignetting and frosts the glass behind the interface panels.
5. **Interface** — panels, text and icons, drawn on top.

The environment maps are baked with compute shaders when the theme changes, and the BRDF table once at startup.

## License

MIT License

Copyright (c) 2026 Mykhailo Makarov

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Support

If you found this project interesting or useful, you can support my work:

[![GitHub Sponsors](https://img.shields.io/github/sponsors/makarov-mm?style=flat&logo=github)](https://github.com/sponsors/makarov-mm)
