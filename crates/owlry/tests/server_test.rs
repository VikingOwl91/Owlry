use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;

use owlry::ipc::{Request, Response};
use owlry::server::Server;

/// Helper: send a JSON request line and read the JSON response line.
fn roundtrip(stream: &mut UnixStream, request: &Request) -> Response {
    let mut line = serde_json::to_string(request).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut buf = String::new();
    reader.read_line(&mut buf).unwrap();
    serde_json::from_str(buf.trim()).unwrap()
}

#[test]
fn test_server_responds_to_providers_request() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    // Spawn the server to handle exactly one connection
    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    // Connect as a client
    let mut stream = UnixStream::connect(&sock).unwrap();
    let resp = roundtrip(&mut stream, &Request::Providers);

    match resp {
        Response::Providers { list } => {
            // The default ProviderManager always has at least Application and Command
            assert!(
                list.len() >= 2,
                "expected at least 2 providers, got {}",
                list.len()
            );
            let ids: Vec<&str> = list.iter().map(|p| p.id.as_str()).collect();
            assert!(ids.contains(&"app"), "missing 'app' provider");
            assert!(ids.contains(&"cmd"), "missing 'cmd' provider");
        }
        other => panic!("expected Providers response, got: {:?}", other),
    }

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_handles_launch_request() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();
    let req = Request::Launch {
        item_id: "firefox.desktop".into(),
        provider: "app".into(),
    };
    let resp = roundtrip(&mut stream, &req);

    assert_eq!(resp, Response::Ack);

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_handles_query_request() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();
    let req = Request::Query {
        text: "nonexistent_query_xyz".into(),
        modes: None,
        prefix: None,
    };
    let resp = roundtrip(&mut stream, &req);

    match resp {
        Response::Results { items } => {
            // A nonsense query should return empty or very few results
            // (no items will fuzzy-match "nonexistent_query_xyz")
            assert!(
                items.len() <= 5,
                "expected few/no results for gibberish query"
            );
        }
        other => panic!("expected Results response, got: {:?}", other),
    }

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_handles_toggle_request() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();
    let resp = roundtrip(&mut stream, &Request::Toggle);

    assert_eq!(resp, Response::Ack);

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_handles_refresh_request() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();
    let req = Request::Refresh {
        provider: "app".into(),
    };
    let resp = roundtrip(&mut stream, &req);

    assert_eq!(resp, Response::Ack);

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_handles_submenu_for_unknown_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();
    let req = Request::Submenu {
        plugin_id: "nonexistent_plugin".into(),
        data: "some_data".into(),
    };
    let resp = roundtrip(&mut stream, &req);

    match resp {
        Response::Error { message } => {
            assert!(
                message.contains("nonexistent_plugin"),
                "error should mention the plugin id"
            );
        }
        other => panic!(
            "expected Error response for unknown plugin, got: {:?}",
            other
        ),
    }

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_handles_multiple_requests_per_connection() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();

    // Send Providers request
    let resp1 = roundtrip(&mut stream, &Request::Providers);
    assert!(matches!(resp1, Response::Providers { .. }));

    // Send Toggle request on same connection
    let resp2 = roundtrip(&mut stream, &Request::Toggle);
    assert_eq!(resp2, Response::Ack);

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn test_server_cleans_up_stale_socket() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("owlry-test.sock");

    // Create a stale socket file
    std::os::unix::net::UnixListener::bind(&sock).unwrap();
    assert!(sock.exists());

    // Server::bind should succeed by removing the stale socket
    let server = Server::bind(&sock).unwrap();

    let handle = thread::spawn(move || {
        server.handle_one_for_testing().unwrap();
    });

    let mut stream = UnixStream::connect(&sock).unwrap();
    let resp = roundtrip(&mut stream, &Request::Toggle);
    assert_eq!(resp, Response::Ack);

    drop(stream);
    handle.join().unwrap();
}
