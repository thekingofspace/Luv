mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{main_script, run_with};
use luv::project::Project;
use mlua::Table;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve_http() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut buffer = [0u8; 4096];
                let (head, body_start) = loop {
                    let count = stream.read(&mut buffer).await.unwrap_or(0);
                    if count == 0 {
                        return;
                    }
                    data.extend_from_slice(&buffer[..count]);
                    if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                        break (String::from_utf8_lossy(&data[..end]).into_owned(), end + 4);
                    }
                };
                let length = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                while data.len() < body_start + length {
                    let count = stream.read(&mut buffer).await.unwrap_or(0);
                    if count == 0 {
                        break;
                    }
                    data.extend_from_slice(&buffer[..count]);
                }
                let body = String::from_utf8_lossy(&data[body_start..]).into_owned();
                let mut request_line = head.lines().next().unwrap_or_default().split_whitespace();
                let method = request_line.next().unwrap_or_default().to_owned();
                let path = request_line.next().unwrap_or_default().to_owned();
                let token = head
                    .lines()
                    .find_map(|line| line.strip_prefix("x-token: ").or_else(|| line.strip_prefix("X-Token: ")))
                    .unwrap_or("none")
                    .to_owned();
                let (status, reply) = match path.as_str() {
                    "/missing" => ("404 Not Found", "nope".to_owned()),
                    "/slow" => {
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        ("200 OK", "late".to_owned())
                    }
                    _ => ("200 OK", format!("{method} {path} {token} {body}")),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nX-Reply: yes\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    format!("http://{address}")
}

async fn run_script(source: &str, server: Option<String>) -> common::Outcome {
    let dir = main_script(source);
    let project = Project::load(dir.path()).unwrap();
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.setup(move |lua| match &server {
            Some(server) => lua.globals().set("server", server.as_str()),
            None => Ok(()),
        })
    })
    .await;
    drop(dir);
    outcome
}

#[tokio::test]
async fn http_requests_return_responses() {
    let server = serve_http().await;
    let outcome = run_script(
        r#"
local Net = import("Net")
local echo = Net.Request({
    Url = server .. "/echo",
    Method = "post",
    Headers = { ["X-Token"] = "secret" },
    Body = "hello body",
    Timeout = 5,
})
local missing = Net.Get(server .. "/missing")
local posted = Net.Post(server .. "/form", buffer.fromstring("raw"), { ["X-Token"] = 7 })
results = {
    status = echo.StatusCode,
    message = echo.StatusMessage,
    ok = echo.Ok,
    body = echo.Body,
    reply = echo.Headers["x-reply"],
    url = echo.Url,
    missingStatus = missing.StatusCode,
    missingOk = missing.Ok,
    missingBody = missing.Body,
    posted = posted.Body,
}
local function failure(action)
    local ok, message = pcall(action)
    assert(not ok, "expected a failure")
    return tostring(message)
end
local listener = Net.TcpListen(0, "127.0.0.1")
local closedPort = listener.Port
listener:Close()
errors = {
    noUrl = failure(function() Net.Request({}) end),
    badMethod = failure(function() Net.Request({ Url = server, Method = "NOT A METHOD" }) end),
    refused = failure(function() Net.Get(`http://127.0.0.1:{closedPort}/`) end),
    badBody = failure(function() Net.Request({ Url = server, Body = {} }) end),
}
"#,
        Some(server.clone()),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(results.get::<u16>("status").unwrap(), 200);
    assert_eq!(text("message"), "OK");
    assert!(results.get::<bool>("ok").unwrap());
    assert_eq!(text("body"), "POST /echo secret hello body");
    assert_eq!(text("reply"), "yes");
    assert_eq!(text("url"), format!("{server}/echo"));
    assert_eq!(results.get::<u16>("missingStatus").unwrap(), 404);
    assert!(!results.get::<bool>("missingOk").unwrap());
    assert_eq!(text("missingBody"), "nope");
    assert_eq!(text("posted"), "POST /form 7 raw");
    let errors: Table = outcome.global("errors");
    let expected = [
        ("noUrl", "HTTP requests need a Url"),
        ("badMethod", "is not an HTTP method"),
        ("refused", "failed"),
        ("badBody", "the request Body must be a string or buffer"),
    ];
    for (key, fragment) in expected {
        let message: String = errors.get(key).unwrap();
        assert!(message.contains(fragment), "{key}: {message:?} should contain {fragment:?}");
    }
}

#[tokio::test]
async fn http_requests_only_suspend_the_calling_coroutine() {
    let server = serve_http().await;
    let outcome = run_script(
        r#"
local Net = import("Net")
log = {}
coroutine.wrap(function()
    local response = Net.Get(server .. "/slow")
    table.insert(log, "response " .. response.Body)
end)()
table.insert(log, "main")
"#,
        Some(server),
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "response late"]);
}

#[tokio::test]
async fn tcp_sockets_talk_through_signals() {
    let outcome = run_script(
        r#"
local Net = import("Net")
log = {}
local server = Net.TcpListen(0, "127.0.0.1")
server.Connected:BindHandler("echo", function(client)
    table.insert(log, `server got a client from {client.RemoteHost}`)
    client.Received:BindHandler("echo", function(data)
        client:Send("echo:" .. data)
    end)
    client.Closed:BindHandler("log", function(reason)
        table.insert(log, "server side closed")
        server:Close()
    end)
end)
server.Closed:BindHandler("log", function()
    table.insert(log, "server closed")
end)
local socket = Net.TcpConnect("127.0.0.1", server.Port)
details = {
    className = socket.ClassName,
    serverClass = server.ClassName,
    remotePort = socket.RemotePort == server.Port,
    open = socket.IsOpen,
}
socket:Send("hello")
local reply = socket.Received:Wait()
table.insert(log, "client got " .. reply)
socket:Close()
local reason = socket.Closed:Wait()
table.insert(log, "client closed: " .. reason)
details.openAfterClose = socket.IsOpen
local ok, message = pcall(function() socket:Send("late") end)
details.sendAfterClose = tostring(message)
"#,
        None,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(
        log,
        [
            "server got a client from 127.0.0.1",
            "client got echo:hello",
            "client closed: closed",
            "server side closed",
            "server closed",
        ]
    );
    let details: Table = outcome.global("details");
    assert_eq!(details.get::<String>("className").unwrap(), "TcpSocket");
    assert_eq!(details.get::<String>("serverClass").unwrap(), "TcpServer");
    assert!(details.get::<bool>("remotePort").unwrap());
    assert!(details.get::<bool>("open").unwrap());
    assert!(!details.get::<bool>("openAfterClose").unwrap());
    assert!(details.get::<String>("sendAfterClose").unwrap().contains("the TcpSocket is closed"));
}

#[tokio::test]
async fn udp_sockets_exchange_datagrams() {
    let outcome = run_script(
        r#"
local Net = import("Net")
local first = Net.UdpBind(0, "127.0.0.1")
local second = Net.UdpBind(0, "127.0.0.1")
second.Received:BindHandler("reply", function(data, host, port)
    second:Send("pong:" .. data, host, port)
end)
first:Send(buffer.fromstring("ping"), "127.0.0.1", second.Port)
local data, host, port = first.Received:Wait()
results = { data = data, host = host, fromSecond = port == second.Port, className = first.ClassName }
first:Close()
second:Close()
"#,
        None,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("data").unwrap(), "pong:ping");
    assert_eq!(results.get::<String>("host").unwrap(), "127.0.0.1");
    assert!(results.get::<bool>("fromSecond").unwrap());
    assert_eq!(results.get::<String>("className").unwrap(), "UdpSocket");
}

#[tokio::test]
async fn websockets_send_text_and_binary_messages() {
    let outcome = run_script(
        r#"
local Net = import("Net")
log = {}
local server = Net.WebSocketListen(0, "127.0.0.1")
server.Connected:BindHandler("echo", function(client)
    client.Received:BindHandler("echo", function(message, binary)
        client:Send(if binary then "binary:" .. message else "text:" .. message)
    end)
    client.Closed:BindHandler("log", function(code, reason)
        table.insert(log, `server saw close {code} {reason}`)
        server:Close()
    end)
end)
local socket = Net.WebSocketConnect(`ws://127.0.0.1:{server.Port}`, { ["X-Player"] = "one" })
socket:Send("hi")
local first, firstBinary = socket.Received:Wait()
table.insert(log, `{first} {firstBinary}`)
socket:Send(buffer.fromstring("raw"))
local second = socket.Received:Wait()
table.insert(log, second)
socket:Close(4000, "bye")
local code, reason = socket.Closed:Wait()
table.insert(log, `client closed {code} {reason}`)
classes = { socket.ClassName, server.ClassName }
"#,
        None,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(
        log,
        [
            "text:hi false",
            "binary:raw",
            "client closed 4000 bye",
            "server saw close 4000 bye",
        ]
    );
    let classes: Vec<String> = outcome.global("classes");
    assert_eq!(classes, ["WebSocket", "WebSocketServer"]);
}

#[tokio::test]
async fn open_servers_keep_the_game_running_until_closed() {
    let started = Instant::now();
    let outcome = run_script(
        r#"
local Net = import("Net")
local server = Net.TcpListen(0, "127.0.0.1")
coroutine.wrap(function()
    sleep(150)
    server:Close()
end)()
server.Closed:BindHandler("log", function()
    closed = true
end)
"#,
        None,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("closed"));
    assert!(started.elapsed() >= Duration::from_millis(150));
}

#[tokio::test]
async fn destroying_open_sockets_and_servers_reports_nothing() {
    let outcome = run_script(
        r#"
local Net = import("Net")
local server = Net.TcpListen(0, "127.0.0.1")
server.Closed:BindHandler("log", function() end)
local client = Net.TcpConnect("127.0.0.1", server.Port)
client.Closed:BindHandler("log", function() end)
local udp = Net.UdpBind(0, "127.0.0.1")
udp.Closed:BindHandler("log", function() end)
local web = Net.WebSocketListen(0, "127.0.0.1")
web.Closed:BindHandler("log", function() end)
client:Destroy()
server:Destroy()
udp:Destroy()
web:Destroy()
sleep(100)
finished = true
"#,
        None,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("finished"));
}
