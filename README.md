# wallrs
Simple TUI Wallpaper Manger for X11/Wayland.

## Table of contents
* [Requirements](#requirements)
* [Installation](#installation)
* [Features](#features)
* [Configuration](#configuration)

## Requirements

- feh (for X11)
- swww (for Wayland)
- pywal

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
- vim_motion.
- enable_mouse_support.
