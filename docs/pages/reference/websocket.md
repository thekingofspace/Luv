# WebSocket

Inherits: [BaseGameObject](basegameobject.md)

One WebSocket connection.

## Description

You get a WebSocket from [Net.WebSocketConnect](net.md#websocketconnect), or from the Connected signal of a [WebSocketServer](#websocketserver).

Each message you send arrives as one [Received](#received) event on the other side. A message is text or binary.

An open WebSocket keeps the game running. Call [Close](#close) when you are done.

> [!NOTE]
> Close a WebSocket with [Close](#close) so that `Closed` fires. `Destroy` closes it too, but `Closed` does not fire.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"WebSocket"`. Read only. |
| `IsOpen` | `boolean` | `true` until the connection closes. It turns `false` right before [Closed](#closed) fires. Read only. |

## Methods

### Send

```luau
socket:Send(message: string | buffer, binary: boolean?)
```

Sends one message. This yields the calling coroutine.

- A string is sent as a text message.
- A buffer is sent as a binary message.
- With `binary` set to `true`, a string is sent as a binary message.
- A string that is not valid UTF-8 is always sent as a binary message.

It errors with `the WebSocket is closed` or `cannot send over the WebSocket: <reason>`.

### Close

```luau
socket:Close(code: number?, reason: string?)
```

Sends a close message and closes the connection. `code` defaults to `1000` and `reason` to `""`. [Closed](#closed) fires on this side with the same code and reason. It does not wait for the other side to answer. Calling Close on a closed WebSocket does nothing. It does not yield.

## Signals

### Received

```luau
socket.Received: Signal<string, boolean>
```

Fires for each message. The arguments are the message and `true` when it is a binary message. Binary messages arrive as strings too. Use `buffer.fromstring` when you need a buffer.

### Closed

```luau
socket.Closed: Signal<number, string>
```

Fires once when the connection closes. The arguments are a code and a reason.

| Code | When |
| --- | --- |
| the code you passed | You called [Close](#close). |
| the code of the other side | The other side closed the connection. |
| `1005` | The other side closed without a code. |
| `1006` | The connection broke or ended without a close message. The reason says why. |

```luau
local Net = import("Net")

local socket = Net.WebSocketConnect("wss://example.com/chat", { ["X-Player"] = "one" })
socket.Received:BindHandler("chat", function(message, binary)
	print(if binary then "binary" else "text", message)
end)
socket.Closed:BindHandler("log", function(code, reason)
	print("closed", code, reason)
end)
socket:Send("hello")
```

## WebSocketServer

A server that accepts WebSocket clients. You get one from [Net.WebSocketListen](net.md#websocketlisten). Its `ClassName` is `"WebSocketServer"`.

It has the same members as [TcpServer](tcpserver.md): `IsOpen`, `Host`, `Port`, `Close`, `Connected` and `Closed`. Only [Connected](tcpserver.md#connected) is different. It gives a WebSocket for each client, after the client has finished connecting. Clients that fail to connect are dropped without an event.

An open server keeps the game running. Closing it does not close its clients. Close a server with `Close` so that `Closed` fires.

```luau
local Net = import("Net")

local server = Net.WebSocketListen(8080)
server.Connected:BindHandler("echo", function(client)
	client.Received:BindHandler("echo", function(message, binary)
		client:Send(message, binary)
	end)
end)
print("listening on port", server.Port)
```
