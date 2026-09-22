# List of features

This page lists what luv can do today.

## Platforms

- Runs on 64 bit Windows and Linux.
- Games you ship run on the same systems.
- Drawing uses Vulkan.

## Scripting

- Games are written in [Luau](https://luau.org).
- Full type info for the editor through `types.d.luau`.
- `require` by path with `.luaurc` aliases. See [Scripts and modules](../manual/scripts.md).
- Calls that wait only pause the script that made them. The main thread never blocks. See [Yielding and coroutines](../manual/yielding.md).
- Code between `EnterParallel()` and `ExitParallel()` runs on its own thread. See [Parallel code](../manual/parallel.md).
- [Signals](../reference/signal.md) for events and a [Messenger](../reference/messenger.md) that sends messages between threads.
- [Bulk](../reference/bulk.md) updates change many objects in one call.

## Windows

- As many windows as you want, each with its own frame loop and frame rate.
- Windowed, borderless, maximized, fullscreen and exclusive fullscreen modes.
- `PreFrame`, `OnFrame` and `AfterFrame` signals with the frame time.
- Live resize signals while the user drags the window edge.
- Screen info like size, scale and refresh rate through [Viewport](../reference/viewport.md).

## Drawing

- Shapes: rectangle, circle, triangle, right triangle, diamond, pentagon, hexagon and octagon, with outlines.
- Images with tinting, flipping, sprite sheets and pixel art mode.
- Text from TrueType and OpenType fonts, with bold, italic, underline, outlines, wrapping and alignment.
- Position, size, anchor point, rotation and draw order on every object.
- Alpha, additive, multiply and opaque blending.
- Point, area, radius and ray queries to find objects on screen.

### Image formats

PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, TGA, DDS, HDR, OpenEXR, PNM, QOI, farbfeld and SVG.

## Shaders

- Custom shaders in WGSL, GLSL or SPIR-V.
- A shader prelude with engine data, shape math and helpers.
- Send numbers, vectors, colors, arrays, structs, textures and even other renderables to a shader.
- Custom renderables that draw anything with your own vertex and fragment shaders.
- Post processing passes that run on the whole window.
- Render hooks: native code that writes shader data every frame on the render thread.

See [Shaders](../manual/shaders.md) and [Post processing](../manual/post-processing.md).

## Sound

- A node based sound system. You link sources, modifiers and outputs together.
- Sounds from files, from bytes in memory, or streamed as raw samples.
- 26 modifiers: gain, pan, filters, equalizer, echo, reverb, chorus, flanger, phaser, tremolo, vibrato, distortion, bit crusher, compressor, limiter, noise gate, pitch shift, ring modulator, stereo width and a meter.
- 3D positions with a listener, distance roll off, cones and binaural sound.
- Pick the output device and list every device.
- Capture any sound as packets of samples.
- Clean pausing and stopping with no clicks.

See [Sound](../manual/sound.md).

### Sound formats

WAV, MP3, FLAC, OGG Vorbis, Opus, AAC and M4A, AIFF, CAF, MKV and WebM.

## Input

- Keyboard keys and typed text.
- Mouse buttons, movement, scrolling, cursor icons and mouse lock.
- Controllers with buttons, sticks, triggers and vibration.
- Touch screens with pressure.
- Checks for when a device is plugged in or removed.

See [Input](../manual/input.md).

## Files and data

- A file system library close to Lua's `io`, plus helpers like `readFile` and `readDir`.
- Game assets that load from the packed game.
- A save folder for each game.
- JSON, JSONC, TOML and YAML with [Serde](../reference/serde.md).

## Networking

- HTTP and HTTPS requests.
- TCP clients and servers.
- UDP sockets.
- WebSocket clients and servers.

See [Networking](../manual/networking.md).

## Security

- Hashes: MD5, SHA1, SHA2, SHA3 and BLAKE3.
- HMAC, HKDF and PBKDF2.
- Password hashing with Argon2.
- Encryption with AES GCM and ChaCha20Poly1305.
- Signing with Ed25519 and ECDSA, and key exchange with X25519.

See [Crypto](../reference/crypto.md).

## Randomness

- Seeded random numbers that repeat the same way every time.
- Random picks, weighted picks, shuffles and samples.
- Gaussian and exponential numbers, random colors, strings and UUIDs.
- Noise and fractal noise.

See [Random](../reference/random.md).

## Native code

- Call functions in any DLL or shared library.
- Structs, arrays, pointers and callbacks.
- Plugins in C, C++ or Rust that add classes and functions to Luau.
- The `native` folder is built for you.

See [Native plugins](../manual/native-plugins.md).

## Shipping

- `luv package` makes one program with your scripts and assets inside.
- Scripts ship as bytecode, not source.
- [Containers](../manual/containers.md) for DLC and mods.

See [Shipping your game](../manual/shipping.md).
