# Type index

Every type in `types.d.luau` is listed here, with what it inherits and where it is explained. Use these names in type annotations, for example `local size: UDim = window.Size`.

The Inherits column links to the parent type. A type gets every member of its parent.

## Basics

| Type | Inherits | What it is |
| --- | --- | --- |
| [BaseGameObject](basegameobject.md) | none | The base of every engine object. Has `ClassName`, `Name` and `Destroy`. |
| [UDim](udim.md) | none | A position or size with `X`, `Y` and `Z`. |
| `UDimMetatable` | none | The math operators of a UDim. See [UDim](udim.md). |
| `UDim_API` | none | The `udim` global. See [UDim](udim.md). |
| [Color](color.md) | none | A color with `R`, `G`, `B` and `A`. |
| `ColorMetatable` | none | The math operators of a Color. See [Color](color.md). |
| `Color_API` | none | The `color` global. See [Color](color.md). |
| [EnumItem](enums.md#enumitem) | none | One item of an enum, with `Name`, `Value` and `EnumType`. |
| `Enums` | none | The `enum` global. See [Enums](enums.md). |
| `Imports` | none | Every name `import` accepts. See [import](globals.md#import). |
| [Signal](signal.md) | [BaseGameObject](basegameobject.md) | An event you can bind handlers to and fire. |
| `Signal_API` | none | The `Signal` library with `Signal.new`. See [Signal](signal.md). |
| [Messenger](messenger.md) | [BaseGameObject](basegameobject.md) | Sends messages by topic between threads. |
| `Messenger_API` | none | The same as `Messenger`. See [Messenger](messenger.md). |
| `Bulk_API` | none | The `Bulk` library. See [Bulk](bulk.md). |

## Enum types

Each enum has two types. `XEnum` is one item, and `X_Enum` is the table of all items.

| Item type | Table type | Page |
| --- | --- | --- |
| `AudioFormatEnum` | `AudioFormat_Enum` | [AudioFormat](enums.md#audioformat) |
| `BlendModeEnum` | `BlendMode_Enum` | [BlendMode](enums.md#blendmode) |
| `CipherAlgorithmEnum` | `CipherAlgorithm_Enum` | [CipherAlgorithm](enums.md#cipheralgorithm) |
| `ControllerAxisEnum` | `ControllerAxis_Enum` | [ControllerAxis](enums.md#controlleraxis) |
| `ControllerButtonEnum` | `ControllerButton_Enum` | [ControllerButton](enums.md#controllerbutton) |
| `ControllerStickEnum` | `ControllerStick_Enum` | [ControllerStick](enums.md#controllerstick) |
| `HashAlgorithmEnum` | `HashAlgorithm_Enum` | [HashAlgorithm](enums.md#hashalgorithm) |
| `KeyAlgorithmEnum` | `KeyAlgorithm_Enum` | [KeyAlgorithm](enums.md#keyalgorithm) |
| `KeyCodeEnum` | `KeyCode_Enum` | [KeyCode](enums.md#keycode) |
| `MouseButtonEnum` | `MouseButton_Enum` | [MouseButton](enums.md#mousebutton) |
| `MouseIconEnum` | `MouseIcon_Enum` | [MouseIcon](enums.md#mouseicon) |
| `MouseLockModeEnum` | `MouseLockMode_Enum` | [MouseLockMode](enums.md#mouselockmode) |
| `ResampleModeEnum` | `ResampleMode_Enum` | [ResampleMode](enums.md#resamplemode) |
| `RollOffModeEnum` | `RollOffMode_Enum` | [RollOffMode](enums.md#rolloffmode) |
| `ShapeTypeEnum` | `ShapeType_Enum` | [ShapeType](enums.md#shapetype) |
| `TextXAlignmentEnum` | `TextXAlignment_Enum` | [TextXAlignment](enums.md#textxalignment) |
| `TextYAlignmentEnum` | `TextYAlignment_Enum` | [TextYAlignment](enums.md#textyalignment) |
| `WindowTypeEnum` | `WindowType_Enum` | [WindowType](enums.md#windowtype) |

## Windows and screens

| Type | Inherits | What it is |
| --- | --- | --- |
| [Window](window.md) | [BaseGameObject](basegameobject.md) | An open window. |
| `Window_API` | none | The `Window` library with `Window.new`. See [Window](window.md#new). |
| `WindowConfig` | none | The config table of `Window.new`. See [Window](window.md#new). |
| `WindowAPIs` | none | Every name `window:GetAPI` accepts. See [Window](window.md). |
| [PostProcess](postprocess.md) | [BaseGameObject](basegameobject.md) | A full window shader pass. |
| [Screen](viewport.md#screen) | none | Info about one monitor. |
| `Viewport_API` | none | The `Viewport` library. See [Viewport](viewport.md). |

## Drawing

| Type | Inherits | What it is |
| --- | --- | --- |
| `Renderable_API` | none | The API from `window:GetAPI("Renderable")`. See [Renderable API](renderable-api.md). |
| [Renderable](renderable.md) | [BaseGameObject](basegameobject.md) | The base of every drawn object. |
| `RenderableCustom` | [Renderable](renderable.md) | The `"Renderable"` class, drawn only by your shaders. Adds `VertexCount` and `InstanceCount`. |
| `RenderablePlacement` | none | The `Position`, `Size`, `AnchorPoint`, `Rotation` and `Color` fields. See [Placement](renderable.md#placement). |
| [RenderableShape](renderableshape.md) | [Renderable](renderable.md), `RenderablePlacement` | A filled shape. |
| [RenderableImage](renderableimage.md) | [Renderable](renderable.md), `RenderablePlacement` | An image. |
| [RenderableText](renderabletext.md) | [Renderable](renderable.md), `RenderablePlacement` | Text in a font. |
| `RenderableClasses` | none | Maps each class name to its type. See [Renderable API](renderable-api.md#new). |
| `RenderableConfig` | none | Config fields every class accepts. See [Config](renderable.md#config). |
| `RenderablePlacementConfig` | none | Config fields for placement. See [Config](renderable.md#config). |
| `RenderableCustomConfig` | `RenderableConfig` | Config for the `"Renderable"` class. See [Config](renderable.md#config). |
| `RenderableShapeConfig` | `RenderableConfig`, `RenderablePlacementConfig` | Config for shapes. See [RenderableShape](renderableshape.md#config). |
| `RenderableImageConfig` | `RenderableConfig`, `RenderablePlacementConfig` | Config for images. See [RenderableImage](renderableimage.md#config). |
| `RenderableTextConfig` | `RenderableConfig`, `RenderablePlacementConfig` | Config for text. See [RenderableText](renderabletext.md#config). |
| `RenderableConfigs` | none | Maps each class name to its config type. |
| `RenderableAnyConfig` | `RenderableConfig`, `RenderablePlacementConfig` | Every config field of every class in one type. `Renderable.new` takes this. |
| [QueryParams](renderable-api.md#queryparams) | none | Include and exclude lists for queries. |
| [RaycastResult](renderable-api.md#raycastresult) | none | What a raycast hit. |

## Shaders

| Type | Inherits | What it is |
| --- | --- | --- |
| `Shader_API` | none | The `Shader` library. See [Shader](shader-library.md). |
| [Shader](shader.md) | [BaseGameObject](basegameobject.md) | A compiled shader. |
| [ShaderCombo](shadercombo.md) | [BaseGameObject](basegameobject.md) | Several shader sources joined into one. |
| [ShaderSource](shader-library.md#shadersource) | none | Anything `Shader.Compile` accepts. |
| [ShaderComboOptions](shader-library.md#shadercombooptions) | none | The options of `Shader.Combine`. |
| [ShaderEntryPoint](shader.md#shaderentrypoint) | none | One entry point of a compiled shader. |
| `ShaderLanguage` | none | `"wgsl"`, `"glsl"` or `"spirv"`. See [ShaderSource](shader-library.md#shadersource). |
| `ShaderStage` | none | The stage of a GLSL source. See [ShaderSource](shader-library.md#shadersource). |
| [ShaderValue](shader-library.md#shadervalue) | none | Any value `WriteShaderData` accepts. |

## Input

| Type | Inherits | What it is |
| --- | --- | --- |
| `Input_API` | none | The keyboard API. See [Input API](input.md). |
| `Mouse_API` | none | The mouse API. See [Mouse API](mouse.md). |
| `Controller_API` | none | The controller API. See [Controller API](controller.md). |
| [ControllerInfo](controller.md#controllerinfo) | none | One connected controller. |
| `Touch_API` | none | The touch API. See [Touch API](touch.md). |
| [TouchPoint](touch.md#touchpoint) | none | One finger on the screen. |

## Sound

| Type | Inherits | What it is |
| --- | --- | --- |
| `Sound_API` | none | The API from `window:GetAPI("Sound")`. See [Sound API](sound-api.md). |
| [SoundListener](sound-api.md#soundlistener) | none | Where the player hears from. |
| [NodeObject](nodeobject.md) | [BaseGameObject](basegameobject.md) | The base of every sound node. |
| [NodeInput](nodeports.md) | none | The port that sends a node's sound on. |
| [NodeOutput](nodeports.md) | none | The port that takes sound in. |
| [SoundNode](soundnode.md) | [NodeObject](nodeobject.md) | A sound loaded from a file. |
| [FromString](soundnode.md#fromstring) | [SoundNode](soundnode.md) | A sound loaded from bytes in memory. |
| [FromBytes](frombytes.md) | [NodeObject](nodeobject.md) | A stream of raw samples you push. |
| [ToSpeaker](tospeaker.md) | [NodeObject](nodeobject.md) | Plays sound on a device, with optional 3D position. |
| [ToBytes](tobytes.md) | [NodeObject](nodeobject.md) | Turns sound into packets. |
| [AudioPacket](audiopacket.md) | none | One packet of samples. |
| [SoundModifier](soundmodifier.md) | [NodeObject](nodeobject.md) | The base of every modifier. |
| `SoundModifiers` | none | Maps each modifier name to its type. See [Modifier list](modifiers.md). |
| `SoundNodeConfig` | none | Config of `SoundNode` and `FromString`. See [Config](soundnode.md#config). |
| `FromBytesConfig` | none | Config of `FromBytes`. See [Config](frombytes.md#config). |
| `ToSpeakerConfig` | none | Config of `ToSpeaker`. See [Config](tospeaker.md#config). |
| `ToBytesConfig` | none | Config of `ToBytes`. See [Config](tobytes.md#config). |

### Modifier types

Every modifier type inherits [SoundModifier](soundmodifier.md).

| Type | Page |
| --- | --- |
| `GainModifier` | [Gain](modifiers.md#gain) |
| `PanModifier` | [Pan](modifiers.md#pan) |
| `LowPassModifier` | [LowPass](modifiers.md#lowpass) |
| `HighPassModifier` | [HighPass](modifiers.md#highpass) |
| `BandPassModifier` | [BandPass](modifiers.md#bandpass) |
| `NotchModifier` | [Notch](modifiers.md#notch) |
| `PeakModifier` | [Peak](modifiers.md#peak) |
| `LowShelfModifier` | [LowShelf](modifiers.md#lowshelf) |
| `HighShelfModifier` | [HighShelf](modifiers.md#highshelf) |
| `EqualizerModifier` | [Equalizer](modifiers.md#equalizer) |
| `EchoModifier` | [Echo](modifiers.md#echo) |
| `ReverbModifier` | [Reverb](modifiers.md#reverb) |
| `ChorusModifier` | [Chorus](modifiers.md#chorus) |
| `FlangerModifier` | [Flanger](modifiers.md#flanger) |
| `PhaserModifier` | [Phaser](modifiers.md#phaser) |
| `TremoloModifier` | [Tremolo](modifiers.md#tremolo) |
| `VibratoModifier` | [Vibrato](modifiers.md#vibrato) |
| `DistortionModifier` | [Distortion](modifiers.md#distortion) |
| `BitCrusherModifier` | [BitCrusher](modifiers.md#bitcrusher) |
| `CompressorModifier` | [Compressor](modifiers.md#compressor) |
| `LimiterModifier` | [Limiter](modifiers.md#limiter) |
| `NoiseGateModifier` | [NoiseGate](modifiers.md#noisegate) |
| `PitchShiftModifier` | [PitchShift](modifiers.md#pitchshift) |
| `RingModulatorModifier` | [RingModulator](modifiers.md#ringmodulator) |
| `StereoWidthModifier` | [StereoWidth](modifiers.md#stereowidth) |
| `MeterModifier` | [Meter](modifiers.md#meter) |

## Files and processes

| Type | Inherits | What it is |
| --- | --- | --- |
| `FS_API` | none | The `FS` library. See [FS](fs.md). |
| [File](file.md) | [BaseGameObject](basegameobject.md) | An open file. |
| [Metadata](fs.md#metadata-2) | none | What `FS.metadata` returns. |
| [ReadFormat](fs.md#readformat) | none | A format for `read` and `lines`. |
| `Asset_API` | none | The `Asset` library. See [Asset](asset.md). |
| [Asset](asset.md#asset-object) | [BaseGameObject](basegameobject.md) | A loaded asset file. |
| `Process_API` | none | The `Process` library. See [Process](process.md). |
| [Dirs](process.md#dirs) | none | Well known folders like the save folder. |
| [ProcessOptions](process.md#processoptions) | none | Options of `Process.spawn` and `Process.start`. |
| [ProcessStatus](process.md#processstatus) | none | How a program ended. |
| [ProcessResult](process.md#processresult) | [ProcessStatus](process.md#processstatus) | How a program ended plus its output. |
| [Child](child.md) | [BaseGameObject](basegameobject.md) | A running program started with `Process.start`. |
| `Serde_API` | none | The `Serde` library. See [Serde](serde.md). |
| `SerdeFormat` | none | `"json"`, `"jsonc"`, `"toml"` or `"yaml"`. See [Serde](serde.md). |

## Networking

| Type | Inherits | What it is |
| --- | --- | --- |
| `Net_API` | none | The `Net` library. See [Net](net.md). |
| [HttpRequest](net.md#httprequest) | none | The request table of `Net.Request`. |
| `HttpMethod` | none | A method name like `"GET"`. See [HttpRequest](net.md#httprequest). |
| [HttpResponse](net.md#httpresponse) | none | What an HTTP request returns. |
| [TcpSocket](tcpsocket.md) | [BaseGameObject](basegameobject.md) | A TCP connection. |
| [TcpServer](tcpserver.md) | [BaseGameObject](basegameobject.md) | A TCP server. |
| [UdpSocket](udpsocket.md) | [BaseGameObject](basegameobject.md) | A UDP socket. |
| [WebSocket](websocket.md) | [BaseGameObject](basegameobject.md) | A WebSocket connection. |
| [WebSocketServer](websocket.md#websocketserver) | [BaseGameObject](basegameobject.md) | A WebSocket server. It has the same members as a [TcpServer](tcpserver.md), but `Connected` gives WebSocket objects. |

## Crypto and random

| Type | Inherits | What it is |
| --- | --- | --- |
| `Crypto_API` | none | The `Crypto` library. See [Crypto](crypto.md). |
| `Bytes` | none | A `string` or a `buffer`. Crypto and DLL functions take either. |
| [Hasher](hasher.md) | none | A hash you feed in parts. |
| [KeyPair](crypto.md#keypair) | none | A public and private key. |
| `Random_API` | none | The `Random` library with `Random.new`. See [Random](random.md#new). |
| [Random](random.md) | none | A seeded random number generator. |

## Native code

| Type | Inherits | What it is |
| --- | --- | --- |
| `DLL_API` | none | The `DLL` library. See [DLL](dll.md). |
| [Library](library.md) | [BaseGameObject](basegameobject.md) | A loaded DLL or shared library. |
| [NativeFunction](nativefunction.md) | [BaseGameObject](basegameobject.md) | A C function you can call. |
| [NativeFunctionOptions](nativefunction.md#nativefunctionoptions) | none | Options for a native function. |
| [Callback](callback.md) | [BaseGameObject](basegameobject.md) | A Luau function that C can call. |
| [Pointer](pointer.md) | none | An address in memory. |
| `PointerMetatable` | none | The `==` and `tostring` of a Pointer. See [Pointer](pointer.md). |
| [StructType](structtype.md) | none | A C struct layout. |
| [StructField](structtype.md#structfield) | none | One field of a struct. |
| [ArrayType](arraytype.md) | none | A C array layout. |
| `DLLTypeName` | none | A C type name like `"i32"`. See [Type names](dll.md#type-names). |
| `DLLType` | none | A type name, a StructType or an ArrayType. See [Type names](dll.md#type-names). |
| `NativeHandle` | none | A Pointer, NativeFunction or Callback. See [DLL](dll.md). |

## Containers

| Type | Inherits | What it is |
| --- | --- | --- |
| `Container_API` | none | The `Container` library. See [Container](container.md). |
| [ContainerLibrary](containerlibrary.md) | [BaseGameObject](basegameobject.md) | A loaded container. |
