# Installing luv

luv is one program named `luv`. You use it to make projects, run them and pack them for release. The easiest way to get it is with Rokit.

## Install with Rokit

[Rokit](https://github.com/rojo-rbx/rokit) is a tool manager. It downloads programs from GitHub releases and puts them on your PATH. A global install lets you run `luv` from any folder.

### 1. Install Rokit

Run this in a terminal. Skip this step if you already have Rokit.

```powershell title="Windows"
Invoke-RestMethod https://raw.githubusercontent.com/rojo-rbx/rokit/main/scripts/install.ps1 | Invoke-Expression
```

```shell title="Linux"
curl -sSf https://raw.githubusercontent.com/rojo-rbx/rokit/main/scripts/install.sh | bash
```

Close the terminal and open a new one so it sees the new PATH.

### 2. Add luv globally

```shell
rokit add --global thekingofspace/Luv luv
```

The last word is the command name. Keep it as `luv`, or Rokit names the command after the repository.

The first time you add the tool, Rokit asks if you trust it. Answer yes.

### 3. Check that it works

```shell
luv --version
```

This prints the version, for example `luv 0.1.0`.

## Update luv

```shell
rokit update --global luv
```

This moves your global install to the newest release.

## Pin luv in a project

You can also pin a version inside one project. Everyone who works on the project then uses the same `luv`.

```shell
rokit init
rokit add thekingofspace/Luv luv
```

This writes a `rokit.toml` file next to your game:

```toml
[tools]
luv = "thekingofspace/Luv@0.1.0"
```

Other people run `rokit install` inside the project to get the same version. A project pin wins over the global install while you are inside that folder.

## Download a release by hand

Every release on GitHub has two zip files:

| File | For |
| --- | --- |
| `luv-<version>-windows-x86_64.zip` | 64 bit Windows |
| `luv-<version>-linux-x86_64.zip` | 64 bit Linux |

Each zip holds only the `luv` program. Unzip it into a folder that is on your PATH.

## Build from source

You need:

- [Rust](https://rustup.rs), the stable version.
- A C compiler. On Windows this is the Visual Studio Build Tools. On Linux this is `gcc`.
- On Linux, the sound and controller headers. On Ubuntu and Debian you get them with the command below.

```shell
sudo apt install pkg-config libasound2-dev libudev-dev
```

Then build and install:

```shell
git clone https://github.com/thekingofspace/Luv
cd Luv
cargo install --path . --locked
```

Cargo puts `luv` in its own `bin` folder, which is already on your PATH if you installed Rust with rustup.

## Making a release

This part is for the people who publish luv itself.

The repository has a GitHub workflow named **Release** that you start by hand from the **Actions** tab. It only runs when a `changelog.md` file sits in the root of the repository.

1. Set the new version in `Cargo.toml`.
2. Write the release notes in `changelog.md`.
3. Push, then run the **Release** workflow.

The workflow builds `luv` for Windows and Linux, zips each build and publishes a release named after the version, for example `v0.1.0`. The text of `changelog.md` becomes the release notes and the release is marked as the latest one. Rokit picks it up right away. The workflow stops early if a release for that version already exists.
