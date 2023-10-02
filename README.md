# tetris-tui
Tetris in the terminal

![Play tetris in 2-player mode](./tetris-2-player.gif)

## Installation

You can download the latest binary from the [release page](https://github.com/quantonganh/tetris-tui/releases).

### macOS

```sh
$ brew install quantonganh/tap/tetris-tui
```

### Windows

```sh
scoop bucket add quantonganh https://github.com/quantonganh/scoop-bucket.git
scoop install tetris-tui
```

### Install on NetBSD

```
# pkgin install tetris-tui
```

### Install via cargo

```
$ cargo install tetris-tui
```

## Usage

```
$ tetris-tui -h
Tetris in the terminal

Usage: tetris-tui [OPTIONS]

Options:
  -m, --multiplayer
  -s, --server-address <SERVER_ADDRESS>
  -h, --help                             Print help
  -V, --version                          Print version
```

### Single player mode

```sh
$ tetris-tui
```

### 2-player mode

Player 1:

```sh
$ tetris-tui -m
Server started. Please invite your competitor to connect to 192.168.1.183:8080.
```

Player 2:

```sh
$ tetris-tui -m -s 192.168.1.183:8080
```