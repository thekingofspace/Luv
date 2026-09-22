# BaseGameObject

Inherited by: [Signal](signal.md), [Messenger](messenger.md), [Window](window.md), [PostProcess](postprocess.md), [Renderable](renderable.md), [Shader](shader.md), [ShaderCombo](shadercombo.md), [Asset](asset.md#asset-object), [File](file.md), [Child](child.md), [TcpSocket](tcpsocket.md), [TcpServer](tcpserver.md), [UdpSocket](udpsocket.md), [WebSocket](websocket.md), [Library](library.md), [NativeFunction](nativefunction.md), [Callback](callback.md), [ContainerLibrary](containerlibrary.md), [NodeObject](nodeobject.md)

The base class of most engine objects.

## Description

Every class in the list above inherits from BaseGameObject, and so do their children. So each of these objects has a `ClassName`, a `Name` and a [Destroy](#destroy) method.

You never make a BaseGameObject yourself. You get objects from libraries, like [Signal.new](signal.md#new) or [Window.new](window.md#new).

`tostring(object)` returns its `Name`, so `print` shows the name too.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | The class of the object, like `"Signal"` or `"Window"`. Read only. |
| `Name` | `string` | A name you can change. It starts as the class name, unless the class picks another one. For example `Process.Heartbeat` is named `"Heartbeat"`. |

```luau
local Signal = import("Signal")

local touched = Signal.new()
print(touched.ClassName, touched.Name)
touched.Name = "Touched"
print(tostring(touched))
```

This prints `Signal` and `Signal`, and then `Touched`.

## Methods

### Destroy

```luau
object:Destroy()
```

Marks the object as destroyed. Each class then cleans up its own parts. For example a [Signal](signal.md#destroy) drops its handlers. Calling `Destroy` again does nothing.

After `Destroy`, methods that need a live object raise `<ClassName> '<Name>' has been destroyed`. For example a destroyed signal named `Touched` raises `Signal 'Touched' has been destroyed`. You can still read `ClassName` and `Name`.

```luau
local Signal = import("Signal")

local touched = Signal.new()
touched:Destroy()
touched:Destroy()

local ok = pcall(touched.Fire, touched)
print(ok)
```

This prints `false`.
