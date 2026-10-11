//! The endpoint: single instance, stale sockets, requests, events, closing.
#![allow(clippy::unwrap_used, clippy::panic, missing_docs)]

use std::thread;
use std::time::Duration;

use freekerf_ipc::{Client, Endpoint, IpcError, RemoteError, Server};
use serde_json::{Value, json};

/// A tiny server: welcomes, echoes `echo`, fails `fail`, sends an event on `event`.
fn echo_server(endpoint: &Endpoint) -> thread::JoinHandle<()> {
    let server = Server::bind(endpoint).unwrap();
    thread::spawn(move || {
        let mut conn = server.accept().unwrap();
        let hello = conn.reader.read().unwrap().unwrap();
        assert_eq!(hello["hello"]["token"], server.token());
        conn.writer.send(&json!({"welcome": {"ok": true}})).unwrap();
        while let Ok(Some(msg)) = conn.reader.read() {
            if msg.get("bye").is_some() {
                break;
            }
            let id = msg["id"].clone();
            let answer = match msg["method"].as_str().unwrap() {
                "echo" => json!({"id": id, "result": msg["params"]}),
                "event" => {
                    conn.writer.send(&json!({"event": {"n": 1}})).unwrap();
                    json!({"id": id, "result": null})
                }
                "close" => return,
                _ => json!({"id": id, "error": RemoteError::new("refused", "no").to_json()}),
            };
            conn.writer.send(&answer).unwrap();
        }
    })
}

#[test]
fn requests_answers_and_events() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::in_dir(dir.path());
    let server = echo_server(&endpoint);
    let c = Client::connect_retry(&endpoint, &json!({"client": "t"}), Duration::from_secs(5)).unwrap();
    assert_eq!(c.welcome()["ok"], true);
    assert_eq!(
        c.call("echo", json!({"a": [1, 2]})).unwrap(),
        json!({"a": [1, 2]})
    );
    let e = c.call("fail", Value::Null).unwrap_err();
    assert_eq!((e.kind.as_str(), e.message.as_str()), ("refused", "no"));
    c.call("event", Value::Null).unwrap();
    assert_eq!(
        c.events().recv_timeout(Duration::from_secs(5)).unwrap()["event"]["n"],
        1
    );
    // the server goes away: calls fail as closed
    assert_eq!(c.call("close", Value::Null).unwrap_err().kind, "closed");
    assert_eq!(c.call("echo", Value::Null).unwrap_err().kind, "closed");
    server.join().unwrap();
}

#[test]
fn a_dropped_server_leaves_no_instance() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::in_dir(dir.path());
    let server = Server::bind(&endpoint).unwrap();
    assert!(matches!(
        Server::bind(&endpoint),
        Err(IpcError::AlreadyRunning(_))
    ));
    assert_eq!(endpoint.read_token().unwrap(), server.token());
    drop(server);
    assert!(matches!(endpoint.read_token(), Err(IpcError::Token { .. })));
    assert!(matches!(
        Client::connect(&endpoint, json!({})),
        Err(IpcError::NotRunning(_))
    ));
    // a new instance can start
    let again = Server::bind(&endpoint).unwrap();
    assert_eq!(again.endpoint(), &endpoint);
}

#[cfg(unix)]
#[test]
fn a_stale_socket_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::in_dir(dir.path());
    std::fs::create_dir_all(&endpoint.dir).unwrap();
    // a crashed instance: the socket file stays, nothing accepts
    drop(std::os::unix::net::UnixListener::bind(&endpoint.name).unwrap());
    assert!(std::path::Path::new(&endpoint.name).exists());
    let server = Server::bind(&endpoint).unwrap();
    assert_eq!(endpoint.read_token().unwrap(), server.token());
}

#[test]
fn a_refused_hello_and_bad_lines() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::in_dir(dir.path());
    let server = Server::bind(&endpoint).unwrap();
    let t = thread::spawn(move || {
        let mut conn = server.accept().unwrap();
        conn.reader.read().unwrap();
        conn.writer
            .send(&json!({"error": {"kind": "unauthorized", "message": "wrong token"}}))
            .unwrap();
        let mut conn = server.accept().unwrap();
        conn.reader.read().unwrap();
        conn.writer.send(&json!({"something": "else"})).unwrap();
    });
    match Client::connect(&endpoint, json!({})) {
        Err(IpcError::Refused { kind, message }) => {
            assert_eq!(kind, "unauthorized");
            assert_eq!(message, "wrong token");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        Client::connect(&endpoint, json!({})),
        Err(IpcError::Protocol(_))
    ));
    t.join().unwrap();
}
