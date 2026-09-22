# Net

Makes HTTP requests and opens TCP, UDP and WebSocket connections.

```luau
local Net = import("Net")
```

## Description

Every Net function yields the calling coroutine until it is done. The rest of the game keeps running.

Sockets and servers report events through [Signal](signal.md) objects. Each handler runs in its own coroutine and starts right away. An event that fires while no handler is bound and no coroutine waits is lost. So bind handlers before data can arrive.

Data you receive is always a string. It can hold any bytes. You can send a string or a buffer.

Open sockets and servers keep the game running until they are closed. Close each one with its `Close` method when you are done. `Destroy` also closes them, but their `Closed` signal does not fire. [Process.exit](process.md#exit) also ends the game while things are still open.

For a guide, see [Networking](../manual/networking.md).

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Request](#request)(request) | [HttpResponse](#httpresponse) | yes |
| [Get](#get)(url, headers) | [HttpResponse](#httpresponse) | yes |
| [Post](#post)(url, body, headers) | [HttpResponse](#httpresponse) | yes |
| [TcpConnect](#tcpconnect)(host, port) | [TcpSocket](tcpsocket.md) | yes |
| [TcpListen](#tcplisten)(port, host) | [TcpServer](tcpserver.md) | yes |
| [UdpBind](#udpbind)(port, host) | [UdpSocket](udpsocket.md) | yes |
| [WebSocketConnect](#websocketconnect)(url, headers) | [WebSocket](websocket.md) | yes |
| [WebSocketListen](#websocketlisten)(port, host) | [WebSocketServer](websocket.md#websocketserver) | yes |

## Function descriptions

### Request

```luau
Net.Request(request: HttpRequest): HttpResponse
```

Sends an HTTP or HTTPS request and returns an [HttpResponse](#httpresponse). The fields of the request are in [HttpRequest](#httprequest). This yields the calling coroutine until the whole body has arrived.

- A status like `404` or `500` is not an error. Check `Ok` or `StatusCode`.
- Redirects are followed. `Url` in the response is the last address.
- There is no time limit unless you set `Timeout`.
- luv sends no `User-Agent` header unless you set one.

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

It errors when:

| Message | Cause |
| --- | --- |
| `HTTP requests need a Url` | `Url` is missing. |
| `'<method>' is not an HTTP method` | `Method` is not a valid method name. |
| `the header '<name>' must be a string or number, got <type>` | A header value has another type. |
| `the request Body must be a string or buffer, got <type>` | `Body` has another type. |
| `Timeout must be a number of seconds greater than 0` | `Timeout` is not above `0`, or it is `0/0` or `math.huge`. |
| `the request to <url> failed: <reason>` | The server could not be reached, the connection broke or the time ran out. |
| `cannot read the response from <url>: <reason>` | The body could not be read. |

### Get

```luau
Net.Get(url: string, headers: { [string]: string | number }?): HttpResponse
```

Sends a `GET` request. It is the same as `Net.Request({ Url = url, Headers = headers })`.

```luau
local Net = import("Net")

local response = Net.Get("https://example.com/news.txt")
if response.Ok then
	print(response.Body)
end
```

### Post

```luau
Net.Post(url: string, body: string | buffer, headers: { [string]: string | number }?): HttpResponse
```

Sends a `POST` request with a body. It is the same as [Request](#request) with `Method = "POST"`.

### TcpConnect

```luau
Net.TcpConnect(host: string, port: number): TcpSocket
```

Connects to a TCP server and returns a [TcpSocket](tcpsocket.md). `host` can be an IP address or a host name. This yields the calling coroutine until the connection is open. It errors with `cannot connect to <host>:<port>: <reason>`.

### TcpListen

```luau
Net.TcpListen(port: number?, host: string?): TcpServer
```

Starts a TCP server and returns a [TcpServer](tcpserver.md). New clients arrive through its [Connected](tcpserver.md#connected) signal.

- `port` defaults to `0`. Then the system picks a free port. Read it from `Port`.
- `host` defaults to `"0.0.0.0"`. That accepts IPv4 connections from other computers. Use `"127.0.0.1"` to accept only this computer. Use `"::"` for IPv6.

It errors with `cannot listen on <host>:<port>: <reason>`, for example when the port is in use.

### UdpBind

```luau
Net.UdpBind(port: number?, host: string?): UdpSocket
```

Opens a [UdpSocket](udpsocket.md) on a port. `port` and `host` work like in [TcpListen](#tcplisten). It errors with `cannot bind a UdpSocket to <host>:<port>: <reason>`.

### WebSocketConnect

```luau
Net.WebSocketConnect(url: string, headers: { [string]: string | number }?): WebSocket
```

Connects to a WebSocket server and returns a [WebSocket](websocket.md). The url starts with `ws://` or `wss://`. `headers` are sent with the first request. This yields the calling coroutine until the connection is open.

| Message | Cause |
| --- | --- |
| `'<url>' is not a WebSocket url: <reason>` | The url is not valid. |
| `'<name>' is not a valid header name` | A header name is not valid. |
| `the value of '<name>' is not a valid header value` | A header value is not valid. |
| `cannot connect to <url>: <reason>` | The connection failed. |

### WebSocketListen

```luau
Net.WebSocketListen(port: number?, host: string?): WebSocketServer
```

Starts a WebSocket server and returns a [WebSocketServer](websocket.md#websocketserver). `port` and `host` work like in [TcpListen](#tcplisten). Its `Connected` signal gives a [WebSocket](websocket.md) for each new client. It errors with `cannot listen on <host>:<port>: <reason>`.

## HttpRequest

The table you pass to [Request](#request).

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Url` | `string` | required | The address. It starts with `http://` or `https://`. |
| `Method` | `string?` | `"GET"` | The HTTP method, like `"POST"` or `"PUT"`. Case does not matter. |
| `Headers` | `{ [string]: string | number }?` | none | Headers to send. Numbers and booleans are turned into text. |
| `Body` | `(string | buffer)?` | none | The body to send. |
| `Timeout` | `number?` | none | The most seconds the whole request may take. Fractions like `0.5` work. |

## HttpResponse

The table that [Request](#request), [Get](#get) and [Post](#post) return.

| Name | Type | Description |
| --- | --- | --- |
| `StatusCode` | `number` | The status code, like `200` or `404`. |
| `StatusMessage` | `string` | The standard text for the code, like `"OK"` or `"Not Found"`. It is `""` for unknown codes. |
| `Ok` | `boolean` | `true` when the code is from 200 to 299. |
| `Url` | `string` | The last address, after redirects. |
| `Headers` | `{ [string]: string }` | The headers of the response. The names are lowercase. When a header comes more than once, its values are joined with `", "`. |
| `Body` | `string` | The body. It can hold any bytes. |
