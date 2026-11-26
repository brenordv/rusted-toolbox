//! Test support: a local fake of the services with routes, failing servers, and clocks.

use crate::engine::Clock;
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::oneshot;
use url::Url;
use warp::Filter;
use warp::http::{HeaderMap, Method, Response, StatusCode};
use warp::hyper::body::Bytes;
use warp::path::FullPath;

/// The status a request with no route gets, so a test that forgot a route fails loudly.
pub const NO_ROUTE_STATUS: u16 = 599;

/// One request the server received.
#[derive(Debug, Clone)]
pub struct Received {
    pub method: String,
    pub path: String,
    /// The raw query string, empty when there was none.
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Received {
    /// The body as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("the request body is JSON")
    }

    /// The first value of header `name`, matched without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What a route answers.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Accept the request and answer nothing until the server is dropped.
    pub hold: bool,
}

impl Reply {
    /// An answer with `status` and `body`.
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            hold: false,
        }
    }

    /// A JSON answer with `status`.
    pub fn json(status: u16, body: &serde_json::Value) -> Self {
        Self::new(status, body.to_string()).with_header("content-type", "application/json")
    }

    /// An answer that never comes.
    pub fn hold() -> Self {
        Self {
            hold: true,
            ..Self::new(200, "")
        }
    }

    /// The same answer with one more header.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

type Routes = HashMap<(String, String), VecDeque<Reply>>;

/// A fake server on `127.0.0.1` at a port the OS picked, serving on its own thread with its
/// own runtime, so a caller that blocks cannot starve it. Dropping it stops the thread and
/// closes every open connection at once, answered or not.
pub struct MockServer {
    pub addr: SocketAddr,
    routes: Arc<Mutex<Routes>>,
    received: Arc<Mutex<Vec<Received>>>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    /// A server with no routes yet.
    pub fn start() -> Self {
        let std_listener = TcpListener::bind(("127.0.0.1", 0)).expect("a local port");
        std_listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let addr = std_listener.local_addr().expect("the listener's address");
        let routes: Arc<Mutex<Routes>> = Arc::new(Mutex::new(HashMap::new()));
        let received = Arc::new(Mutex::new(Vec::new()));
        let (shutdown, stopped) = oneshot::channel::<()>();

        let (routes_in, record) = (Arc::clone(&routes), Arc::clone(&received));
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a server runtime");
            runtime.block_on(async move {
                let filter = warp::method()
                    .and(warp::path::full())
                    .and(warp::query::raw().or(warp::any().map(String::new)).unify())
                    .and(warp::header::headers_cloned())
                    .and(warp::body::bytes())
                    .then(
                        move |method: Method,
                              path: FullPath,
                              query: String,
                              headers: HeaderMap,
                              body: Bytes| {
                            let (routes, record) = (Arc::clone(&routes_in), Arc::clone(&record));
                            async move {
                                let request = Received {
                                    method: method.to_string(),
                                    path: path.as_str().to_owned(),
                                    query,
                                    headers: headers
                                        .iter()
                                        .map(|(name, value)| {
                                            (
                                                name.as_str().to_owned(),
                                                String::from_utf8_lossy(value.as_bytes())
                                                    .into_owned(),
                                            )
                                        })
                                        .collect(),
                                    body: body.to_vec(),
                                };
                                let reply = next_reply(&routes, &request.method, &request.path);
                                record.lock().expect("the request log").push(request);
                                if reply.hold {
                                    std::future::pending::<()>().await;
                                }
                                answer(&reply)
                            }
                        },
                    );
                let listener =
                    tokio::net::TcpListener::from_std(std_listener).expect("a tokio listener");
                tokio::spawn(warp::serve(filter).incoming(listener).run());
                // A dropped sender also ends the wait. The runtime goes with the thread and takes
                // the server and every open connection with it, so an answer a client stopped
                // reading cannot hold up the stop.
                let _ = stopped.await;
            });
        });

        Self {
            addr,
            routes,
            received,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    /// Queues `reply` for `method` and `path` (no query). Replies to one route are used in
    /// order, and the last one repeats.
    pub fn route(&self, method: &str, path: &str, reply: Reply) {
        self.routes
            .lock()
            .expect("the routes")
            .entry((method.to_owned(), path.to_owned()))
            .or_default()
            .push_back(reply);
    }

    /// `http://127.0.0.1:<port>/`, host-only like the real bases.
    pub fn base(&self) -> Url {
        Url::parse(&format!("http://{}/", self.addr)).expect("the server's address")
    }

    /// Every request received so far, oldest first.
    pub fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the request log").clone()
    }

    /// The requests received for `path`, oldest first.
    pub fn received_at(&self, path: &str) -> Vec<Received> {
        self.received()
            .into_iter()
            .filter(|request| request.path == path)
            .collect()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the server thread");
        }
    }
}

fn next_reply(routes: &Mutex<Routes>, method: &str, path: &str) -> Reply {
    let mut routes = routes.lock().expect("the routes");
    match routes.get_mut(&(method.to_owned(), path.to_owned())) {
        Some(queue) if queue.len() > 1 => queue.pop_front().expect("a queued reply"),
        Some(queue) => queue
            .front()
            .cloned()
            .unwrap_or_else(|| Reply::new(NO_ROUTE_STATUS, "")),
        None => Reply::new(NO_ROUTE_STATUS, ""),
    }
}

fn answer(reply: &Reply) -> Response<Vec<u8>> {
    let mut response =
        Response::builder().status(StatusCode::from_u16(reply.status).expect("a valid status"));
    for (name, value) in &reply.headers {
        response = response.header(name, value);
    }
    response.body(reply.body.clone()).expect("a response")
}

/// Serves connections on a port the OS picked: reads each request, answers 200 promising
/// twice the bytes it sends, sends `sent`, and closes. Returns the host-only base to call.
pub fn cut_short(sent: &'static [u8]) -> Url {
    use std::io::{Read, Write};

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a local port");
    let addr = listener.local_addr().expect("the listener's address");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            let mut request = [0u8; 8192];
            let _request_bytes = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                sent.len() * 2
            );
            let _head = stream.write_all(head.as_bytes());
            let _body = stream.write_all(sent);
        }
    });
    Url::parse(&format!("http://{addr}/")).expect("the server's address")
}

/// A host-only base nothing can answer: port 0 on the loopback. A connection to port 0 fails in
/// the connect call itself on Windows, Linux, and macOS, before any packet is sent, so the
/// failure is immediate and no test server can ever hold the port.
pub fn closed_port() -> Url {
    Url::parse("http://127.0.0.1:0/").expect("the closed port's address")
}

/// A clock that always reads `at`.
pub fn fixed_clock(at: DateTime<Utc>) -> Clock {
    Arc::new(move || at)
}

/// A clock that reads `start`, then `start + step`, `start + 2 * step`, and so on.
pub fn stepping_clock(start: DateTime<Utc>, step: TimeDelta) -> Clock {
    let reads = AtomicI32::new(0);
    Arc::new(move || {
        let read = reads.fetch_add(1, Ordering::SeqCst);
        start + step * read
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[tokio::test]
    async fn the_server_stops_while_a_client_holds_an_answer_it_has_not_read() {
        let server = MockServer::start();
        server.route(
            "GET",
            "/large",
            Reply::new(200, vec![b' '; 32 * 1024 * 1024]),
        );
        let unread = reqwest::get(server.base().join("large").unwrap())
            .await
            .unwrap();
        let (stopped, stop) = mpsc::channel();

        std::thread::spawn(move || {
            drop(server);
            stopped.send(()).unwrap();
        });

        assert_eq!(stop.recv_timeout(Duration::from_secs(10)), Ok(()));
        drop(unread);
    }
}
