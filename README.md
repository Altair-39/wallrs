# wallrs
Simple TUI Wallpaper Manger for X11/Wayland.

![wallrs Demo](assets/demo.mp4)

## Table of contents
* [Requirements](#requirements)
* [Installation](#installation)
* [Features](#features)
* [Configuration](#configuration)
* [Keybindings](#keybindings)

## Requirements

- feh (for X11)
- swww (for Wayland)
- pywal (optiona)

## Installation

From crates: 
```
cargo install wallrs
```

From Source, after cloning the repository:
```
cargo install --path .
```

## Features

- Change dinamically your wallpaper.
- History of recently used wallpapers.
- Toggle favorite wallpapers to find them easily.
- Mouse support.
- Vim motion.

## Configuration

All the configuration happens in a config.toml file.

- wallpaper_dir: the directory root of the wallpapers library.
- vim_motion (true/false).
- enable_mouse_support (true/false).
- list_position ("top"/"bottom"/"left"/"right")
- transition_type (only for swww)

The position and the visibility of the tabs are customizable. 

```

[[tabs]]
name = "Wallpapers"
enabled = true

[[tabs]]
name = "Favorites"
enabled = true

[[tabs]]
name = "History"
enabled = true

```

The parameter of the wallpapers are customizable.

```
[commands]
swww = ["--transition-fps", "60"]


```

## Keybindings

The keybindings are configurable in a keybindings.toml file.

- search
- favorite 
- multi_select
