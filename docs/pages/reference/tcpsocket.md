# TcpSocket

Inherits: [BaseGameObject](basegameobject.md)

One TCP connection.

## Description

You get a TcpSocket from [Net.TcpConnect](net.md#tcpconnect), or from the [Connected](tcpserver.md#connected) signal of a [TcpServer](tcpserver.md).

[Received](#received) gives the data in pieces as it arrives, up to 64 KiB at a time. The pieces do not match your [Send](#send) calls. One Send can arrive in many pieces, and many Sends can arrive in one piece. Mark where each message ends yourself, for example with a new line.

An open socket keeps the game running. Call [Close](#close) when you are done.

> [!NOTE]
> Close a socket with [Close](#close) so that `Closed` fires. `Destroy` closes it too, but `Closed` does not fire.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"TcpSocket"`. Read only. |
| `IsOpen` | `boolean` | `true` until the socket closes. It turns `false` right before [Closed](#closed) fires, not when you call Close. Read only. |
| `RemoteHost` | `string` | The IP address of the other side. Read only. |
| `RemotePort` | `number` | The port of the other side. Read only. |
| `LocalPort` | `number` | The port on this side. Read only. |

## Methods

### Send

```luau
socket:Send(data: string | buffer)
```

Sends the data. This yields the calling coroutine until the system has taken all of it.

It errors when:

- The socket is closed. The message is `the TcpSocket is closed`.
- Sending fails. The message is `cannot send over the TcpSocket: <reason>`.
- `data` has another type. The message is `expected a string or buffer to send, got <type>`.

### Close

```luau
socket:Close()
```

Closes the connection. Data that was not read yet is dropped. [Closed](#closed) fires soon after with `"closed"`. The other side gets `"the connection was closed by the other side"`. Calling Close again does nothing. It does not yield.

## Signals

### Received

```luau
socket.Received: Signal<string>
```

Fires with each piece of data that arrives. A piece is at most 64 KiB.

### Closed

```luau
socket.Closed: Signal<string>
```

Fires once when the socket closes. The argument is the reason.

| Reason | When |
| --- | --- |
| `"closed"` | You called [Close](#close). |
| `"the connection was closed by the other side"` | The other side closed the connection. |
| another message | The connection failed. The message comes from the system. |

```luau
local Net = import("Net")

local socket = Net.TcpConnect("127.0.0.1", 7777)
socket.Received:BindHandler("print", function(data)
	print("got", data)
end)
socket.Closed:BindHandler("log", function(reason)
	print("closed:", reason)
end)
socket:Send("hello\n")
```
