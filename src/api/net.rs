use std::time::Duration;

use mlua::{Lua, Result, Table, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};

use crate::objects::{TcpServer, TcpSocket, UdpSocket, WebSocket};

const ANY_HOST: &str = "0.0.0.0";

struct HttpClient(reqwest::Client);

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        let text = inner.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = inner.source();
    }
    message
}

fn client(lua: &Lua) -> Result<reqwest::Client> {
    if let Some(client) = lua.app_data_ref::<HttpClient>() {
        return Ok(client.0.clone());
    }
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| runtime(format!("cannot start the HTTP client: {}", chain(&error))))?;
    lua.set_app_data(HttpClient(client.clone()));
    Ok(client)
}

fn header_text(name: &str, value: Value) -> Result<String> {
    match value {
        Value::String(text) => Ok(text.to_str()?.to_string()),
        Value::Integer(number) => Ok(number.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Boolean(flag) => Ok(flag.to_string()),
        other => Err(runtime(format!(
            "the header '{name}' must be a string or number, got {}",
            other.type_name()
        ))),
    }
}

fn headers(table: Option<Table>) -> Result<Vec<(String, String)>> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    table
        .pairs::<String, Value>()
        .map(|pair| {
            let (name, value) = pair?;
            let value = header_text(&name, value)?;
            Ok((name, value))
        })
        .collect()
}

async fn request(lua: Lua, options: Table) -> Result<Table> {
    let url: String = options
        .get::<Option<String>>("Url")?
        .ok_or_else(|| runtime("HTTP requests need a Url"))?;
    let method = options.get::<Option<String>>("Method")?.unwrap_or_else(|| "GET".to_owned());
    let method = reqwest::Method::from_bytes(method.to_ascii_uppercase().as_bytes())
        .map_err(|_| runtime(format!("'{method}' is not an HTTP method")))?;
    let mut builder = client(&lua)?.request(method, &url);
    for (name, value) in headers(options.get("Headers")?)? {
        builder = builder.header(name, value);
    }
    match options.get::<Value>("Body")? {
        Value::Nil => {}
        Value::String(text) => builder = builder.body(text.as_bytes().to_vec()),
        Value::Buffer(buffer) => builder = builder.body(buffer.to_vec()),
        other => {
            return Err(runtime(format!(
                "the request Body must be a string or buffer, got {}",
                other.type_name()
            )));
        }
    }
    if let Some(timeout) = options.get::<Option<f64>>("Timeout")? {
        if !(timeout.is_finite() && timeout > 0.0) {
            return Err(runtime("Timeout must be a number of seconds greater than 0"));
        }
        builder = builder.timeout(Duration::from_secs_f64(timeout));
    }
    let response = builder
        .send()
        .await
        .map_err(|error| runtime(format!("the request to {url} failed: {}", chain(&error))))?;

    let status = response.status();
    let result = lua.create_table()?;
    result.set("StatusCode", status.as_u16())?;
    result.set("StatusMessage", status.canonical_reason().unwrap_or_default())?;
    result.set("Ok", status.is_success())?;
    result.set("Url", response.url().to_string())?;
    let header_table = lua.create_table()?;
    for name in response.headers().keys() {
        let values: Vec<String> = response
            .headers()
            .get_all(name)
            .iter()
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
            .collect();
        header_table.set(name.as_str(), values.join(", "))?;
    }
    result.set("Headers", header_table)?;
    let body = response
        .bytes()
        .await
        .map_err(|error| runtime(format!("cannot read the response from {url}: {}", chain(&error))))?;
    result.set("Body", lua.create_string(&body)?)?;
    Ok(result)
}

fn endpoint(port: Option<u16>, host: Option<String>) -> (String, u16) {
    (host.unwrap_or_else(|| ANY_HOST.to_owned()), port.unwrap_or(0))
}

pub fn create(lua: &Lua) -> Result<Table> {
    let net = lua.create_table()?;

    net.set(
        "Request",
        lua.create_async_function(|lua, options: Table| request(lua, options))?,
    )?;
    net.set(
        "Get",
        lua.create_async_function(|lua, (url, headers): (String, Option<Table>)| async move {
            let options = lua.create_table()?;
            options.set("Url", url)?;
            options.set("Headers", headers)?;
            request(lua, options).await
        })?,
    )?;
    net.set(
        "Post",
        lua.create_async_function(|lua, (url, body, headers): (String, Value, Option<Table>)| async move {
            let options = lua.create_table()?;
            options.set("Url", url)?;
            options.set("Method", "POST")?;
            options.set("Body", body)?;
            options.set("Headers", headers)?;
            request(lua, options).await
        })?,
    )?;

    net.set(
        "TcpConnect",
        lua.create_async_function(|lua, (host, port): (String, u16)| async move {
            let stream = TcpStream::connect((host.as_str(), port))
                .await
                .map_err(|error| runtime(format!("cannot connect to {host}:{port}: {error}")))?;
            TcpSocket::start(&lua, stream)
        })?,
    )?;
    net.set(
        "TcpListen",
        lua.create_async_function(|lua, (port, host): (Option<u16>, Option<String>)| async move {
            let (host, port) = endpoint(port, host);
            let listener = TcpListener::bind((host.as_str(), port))
                .await
                .map_err(|error| runtime(format!("cannot listen on {host}:{port}: {error}")))?;
            TcpServer::start(&lua, listener, false)
        })?,
    )?;
    net.set(
        "UdpBind",
        lua.create_async_function(|lua, (port, host): (Option<u16>, Option<String>)| async move {
            let (host, port) = endpoint(port, host);
            let socket = tokio::net::UdpSocket::bind((host.as_str(), port))
                .await
                .map_err(|error| runtime(format!("cannot bind a UdpSocket to {host}:{port}: {error}")))?;
            UdpSocket::start(&lua, socket)
        })?,
    )?;
    net.set(
        "WebSocketConnect",
        lua.create_async_function(|lua, (url, extra): (String, Option<Table>)| async move {
            let mut request = url
                .as_str()
                .into_client_request()
                .map_err(|error| runtime(format!("'{url}' is not a WebSocket url: {error}")))?;
            for (name, value) in headers(extra)? {
                let name = HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| runtime(format!("'{name}' is not a valid header name")))?;
                let value = HeaderValue::from_str(&value)
                    .map_err(|_| runtime(format!("the value of '{name}' is not a valid header value")))?;
                request.headers_mut().insert(name, value);
            }
            let (stream, _) = tokio_tungstenite::connect_async(request)
                .await
                .map_err(|error| runtime(format!("cannot connect to {url}: {}", chain(&error))))?;
            WebSocket::start(&lua, stream)
        })?,
    )?;
    net.set(
        "WebSocketListen",
        lua.create_async_function(|lua, (port, host): (Option<u16>, Option<String>)| async move {
            let (host, port) = endpoint(port, host);
            let listener = TcpListener::bind((host.as_str(), port))
                .await
                .map_err(|error| runtime(format!("cannot listen on {host}:{port}: {error}")))?;
            TcpServer::start(&lua, listener, true)
        })?,
    )?;

    net.set_readonly(true);
    Ok(net)
}
