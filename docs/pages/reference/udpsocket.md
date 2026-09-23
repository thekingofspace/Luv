# UdpSocket

Inherits: [BaseGameObject](basegameobject.md)

A UDP socket on a port.

## Description

You get a UdpSocket from [Net.UdpBind](net.md#udpbind). One socket can send to any address and receive from anyone. Each [Received](#received) event is one packet.

An open socket keeps the game running. Call [Close](#close) when you are done.

> [!NOTE]
> Close a socket with [Close](#close) so that `Closed` fires. `Destroy` closes it too, but `Closed` does not fire.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"UdpSocket"`. Read only. |
| `IsOpen` | `boolean` | `true` until the socket closes. Close and [Destroy](basegameobject.md#destroy) turn it `false` at once. Read only. |
| `Host` | `string` | The IP address the socket uses. Read only. |
| `Port` | `number` | The port the socket uses. Read only. |

## Methods

### Send

```luau
socket:Send(data: string | buffer, host: string, port: number)
```

Sends one packet to `host` and `port`. `host` can be an IP address or a host name. This yields the calling coroutine. It errors with `the UdpSocket is closed` or `cannot send to <host>:<port>: <reason>`.

### Close

```luau
socket:Close()
```

Closes the socket. [Closed](#closed) fires soon after. Calling Close again does nothing. It does not yield.

## Signals

### Received

```luau
socket.Received: Signal<string, string, number>
```

Fires for each packet. The arguments are the data, the IP address of the sender and the port of the sender. Send a reply with `socket:Send(reply, host, port)`.

```luau
local Net = import("Net")

local socket = Net.UdpBind(7777)
socket.Received:BindHandler("pong", function(data, host, port)
	socket:Send("pong:" .. data, host, port)
end)
```

### Closed

```luau
socket.Closed: Signal<()>
```

Fires once when the socket closes. It has no arguments. It also fires when receiving stops because of an error. luv reports that error first.
