# System requirements

This page lists what you need to make games with luv and what players need to run them.

## To run a luv game

| Part | Needs |
| --- | --- |
| System | 64 bit Windows 10 or 11, or 64 bit Linux on x86_64. |
| Graphics | A GPU with Vulkan drivers. Most GPUs from the last ten years work. |
| Sound | Any output device. A game still runs without one, it just makes no sound. |
| Controllers | Optional. Any controller the system sees. |

On Linux the game also needs these system libraries. Most desktop systems have them already.

| Library | Used for |
| --- | --- |
| `libvulkan1` | Drawing. |
| `libasound2` | Sound. |
| `libudev1` | Controllers. |
| X11 or Wayland with `libxkbcommon` | Windows and the keyboard. |

A game without windows, like a server, does not need a GPU or a display.

## To make games

You need everything above, plus:

| Tool | When you need it |
| --- | --- |
| `luv` | Always. See [Installing luv](../start/installing.md). |
| A code editor | Always. VS Code with the Luau Language Server works best. See [Editor setup](../start/editor-setup.md). |
| A C compiler | Only for C or C++ files in `native/`. Use the Visual Studio Build Tools on Windows and `gcc` or `clang` on Linux. |
| Rust | Only for Rust crates in `native/`. Get it from [rustup.rs](https://rustup.rs). |

## Not supported

luv does not run on macOS, phones, consoles or the web.
