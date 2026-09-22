# Networking

luv can make HTTP requests and talk over TCP, UDP and WebSockets. Everything is in the [Net](../reference/net.md) library.

Every Net call yields only the calling coroutine. The rest of the game keeps running while it waits. See [Yielding and coroutines](yielding.md).

## HTTP requests

`Net.Get` fetches a page.

```luau
local Net = import("Net")

local response = Net.Get("https://example.com/news.txt")
if response.Ok then
	print(response.Body)
else
	print("failed with", response.StatusCode)
end
```

Use `Net.Request` for other methods, headers, a body or a time limit.

```luau
local Net = import("Net")
local Serde = import("Serde")

local response = Net.Request({
	Url = "https://example.com/api/scores",
	Method = "POST",
	Headers = { ["Content-Type"] = "application/json" },
	Body = Serde.Encode("json", { name = "player", score = 1200 }),
	Timeout = 10,
})
print(response.StatusCode, response.Body)
```

A status like `404` is not an error. Check `Ok` or `StatusCode`. A request that cannot reach the server throws an error. Catch it with `pcall`.

```luau
local Net = import("Net")

local ok, result = pcall(function()
	return Net.Get("https://example.com/")
end)
if not ok then
	print("offline:", result)
end
```

> [!TIP]
> Set `Timeout`. Without it, a request can wait forever.

## A TCP server and client

`Net.TcpListen` starts a server. `Net.TcpConnect` connects to one. Both sides get a [TcpSocket](../reference/tcpsocket.md) for the connection.

```luau
local Net = import("Net")

local server = Net.TcpListen(7777, "127.0.0.1")
server.Connected:BindHandler("echo", function(client)
	client.Received:BindHandler("echo", function(data)
		client:Send("echo:" .. data)
	end)
end)

local socket = Net.TcpConnect("127.0.0.1", 7777)
socket:Send("hello")
print(socket.Received:Wait())
socket:Close()
server:Close()
```

A few rules:

- Bind the handlers of a new client in the `Connected` handler, before it yields. Data that arrives while no handler is bound and nobody waits is lost.
- `Received` gives the data in pieces. The pieces do not match your `Send` calls. Mark where each message ends yourself, for example with a new line.
- Leave out the host to let players on other computers connect. `Net.TcpListen(7777)` listens on every IPv4 address.

## UDP

`Net.UdpBind` opens a [UdpSocket](../reference/udpsocket.md) on a port. Each `Received` event is one packet, with the address of the sender.

```luau
local Net = import("Net")

local socket = Net.UdpBind(7777)
socket.Received:BindHandler("pong", function(data, host, port)
	socket:Send("pong:" .. data, host, port)
end)
```

## WebSockets

`Net.WebSocketConnect` opens a [WebSocket](../reference/websocket.md). Both `ws://` and `wss://` work. Each message you send arrives as one message on the other side.

```luau
local Net = import("Net")

local socket = Net.WebSocketConnect("wss://example.com/chat")
socket.Received:BindHandler("chat", function(message, binary)
	if not binary then
		print(message)
	end
end)
socket.Closed:BindHandler("log", function(code, reason)
	print("closed", code, reason)
end)
socket:Send("hello")
```

`Net.WebSocketListen` starts a WebSocket server. It works like a TCP server, but its `Connected` signal gives a WebSocket for each client. See [WebSocketServer](../reference/websocket.md#websocketserver).

## Closing things so the game can end

An open socket or server keeps the game running. Close each one when you are done.

| Object | How to close it |
| --- | --- |
| [TcpSocket](../reference/tcpsocket.md) | `socket:Close()` |
| [TcpServer](../reference/tcpserver.md) | `server:Close()` |
| [UdpSocket](../reference/udpsocket.md) | `socket:Close()` |
| [WebSocket](../reference/websocket.md) | `socket:Close(1000, "bye")` |
| [WebSocketServer](../reference/websocket.md#websocketserver) | `server:Close()` |

- Closing a server does not close the clients it accepted. Close them too.
- Use `Close` so that `Closed` fires. `Destroy` closes too, but without `Closed`.
- [Process.exit](../reference/process.md#exit) ends the game even when things are still open.

This server stops after one minute, so the game can end:

```luau
local Net = import("Net")
local Process = import("Process")

local server = Net.TcpListen(7777)
local elapsed = 0
Process.Heartbeat:BindHandler("timeout", function(delta: number)
	elapsed += delta
	if elapsed > 60 then
		Process.Heartbeat:UnBind("timeout")
		server:Close()
	end
end)
```
