# TcpServer

Inherits: [BaseGameObject](basegameobject.md)

A TCP server that accepts clients.

## Description

You get a TcpServer from [Net.TcpListen](net.md#tcplisten). Each new client arrives as a [TcpSocket](tcpsocket.md) through [Connected](#connected).

An open server keeps the game running. Call [Close](#close) when you are done. Closing the server does not close its clients. Close them too.

> [!NOTE]
> Close a server with [Close](#close) so that `Closed` fires. `Destroy` closes it too, but `Closed` does not fire.

[Net.WebSocketListen](net.md#websocketlisten) gives a [WebSocketServer](websocket.md#websocketserver). It has the same members, but its Connected signal gives WebSocket objects.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"TcpServer"`. Read only. |
| `IsOpen` | `boolean` | `true` until the server closes. Read only. |
| `Host` | `string` | The IP address the server listens on. Read only. |
| `Port` | `number` | The port the server listens on. When you passed `0`, this is the port the system picked. Read only. |

## Methods

### Close

```luau
server:Close()
```

Stops accepting new clients. [Closed](#closed) fires soon after. Clients that are connected stay open. Calling Close again does nothing. It does not yield.

## Signals

### Connected

```luau
server.Connected: Signal<TcpSocket>
```

Fires with a [TcpSocket](tcpsocket.md) for each new client. Bind the handlers of the client in your handler before it yields. That way no data is lost.

When the server fails to accept a client, luv reports the error and keeps going.

```luau
local Net = import("Net")

local server = Net.TcpListen(7777)
server.Connected:BindHandler("echo", function(client)
	print("client from", client.RemoteHost)
	client.Received:BindHandler("echo", function(data)
		client:Send(data)
	end)
end)
print("listening on port", server.Port)
```

### Closed

```luau
server.Closed: Signal<()>
```

Fires once when the server has stopped. It has no arguments.
