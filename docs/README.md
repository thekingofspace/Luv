# luv

A Luau game engine for Windows and Linux.

## Install with Rokit

1. Install [Rokit](https://github.com/rojo-rbx/rokit).
2. Add luv globally:

```shell
rokit add --global thekingofspace/Luv luv
```

3. Check that it works:

```shell
luv --version
```

To update later, run `rokit update --global luv`.

## Build from source

You need:

- [Rust](https://rustup.rs), the stable version.
- A C compiler. On Windows this is the Visual Studio Build Tools. On Linux this is `gcc`.
- On Linux, the sound and controller headers:

```shell
sudo apt install pkg-config libasound2-dev libudev-dev
```

Then build and install:

```shell
git clone https://github.com/thekingofspace/Luv
cd Luv
cargo install --path . --locked
```

## Docs

Everything else is in the [docs](pages/about/introduction.md).
