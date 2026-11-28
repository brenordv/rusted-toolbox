//! Test support: a local HTTP listener that records what it receives and answers with one
//! fixed reply.

use crate::config::SpeakConfig;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::oneshot;
use warp::Filter;
use warp::http::{Method, Response, StatusCode};
use warp::hyper::body::Bytes;

/// One request the listener received.
#[derive(Debug, Clone)]
pub struct Received {
    pub method: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Received {
    /// The body as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("the request body is JSON")
    }
}

/// What the listener answers every request with.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub location: Option<String>,
}

impl Reply {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type,
            body: body.into(),
            location: None,
        }
    }

    /// A 200 answer carrying `audio` as `audio/wav`.
    pub fn audio(audio: Vec<u8>) -> Self {
        Self::new(200, "audio/wav", audio)
    }

    /// A JSON answer with `status`.
    pub fn json(status: u16, body: &serde_json::Value) -> Self {
        Self::new(status, "application/json", body.to_string())
    }

    /// A 302 answer pointing at `location`.
    pub fn redirect(location: String) -> Self {
        Self {
            location: Some(location),
            ..Self::new(302, "text/plain", "")
        }
    }
}

/// A listener on `127.0.0.1` at a port the OS picked, serving on its own thread so a caller
/// that blocks cannot starve it. Dropping it stops the thread and closes every open connection
/// at once, answered or not.
pub struct Listener {
    pub addr: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// A listener answering every request with `reply`.
    pub fn start(reply: Reply) -> Self {
        Self::spawn(reply, false)
    }

    /// A listener that records each request and then answers nothing until it is dropped.
    pub fn start_holding() -> Self {
        Self::spawn(Reply::new(200, "text/plain", ""), true)
    }

    fn spawn(reply: Reply, hold: bool) -> Self {
        let std_listener = TcpListener::bind(("127.0.0.1", 0)).expect("a local port");
        std_listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let addr = std_listener.local_addr().expect("the listener's address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let (shutdown, stopped) = oneshot::channel::<()>();

        let record = Arc::clone(&received);
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a listener runtime");
            runtime.block_on(async move {
                let routes = warp::any()
                    .and(warp::method())
                    .and(warp::header::optional::<String>("content-type"))
                    .and(warp::body::bytes())
                    .then(
                        move |method: Method, content_type: Option<String>, body: Bytes| {
                            let (record, reply) = (Arc::clone(&record), reply.clone());
                            async move {
                                record.lock().expect("the request log").push(Received {
                                    method: method.to_string(),
                                    content_type,
                                    body: body.to_vec(),
                                });
                                if hold {
                                    std::future::pending::<()>().await;
                                }
                                answer(&reply)
                            }
                        },
                    );
                let listener =
                    tokio::net::TcpListener::from_std(std_listener).expect("a tokio listener");
                tokio::spawn(warp::serve(routes).incoming(listener).run());
                // A dropped sender also ends the wait. The runtime goes with the thread and takes
                // the server and every open connection with it, so an answer a client stopped
                // reading cannot hold up the stop.
                let _ = stopped.await;
            });
        });

        Self {
            addr,
            received,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    /// The speech URL of this listener, with `suffix` (a query, say) appended.
    pub fn url(&self, suffix: &str) -> String {
        format!("http://{}/v1/audio/speech{suffix}", self.addr)
    }

    /// The default settings pointed at this listener.
    pub fn config(&self) -> SpeakConfig {
        SpeakConfig {
            url: self.url(""),
            ..SpeakConfig::default()
        }
    }

    /// Every request received so far, oldest first.
    pub fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the request log").clone()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the listener thread");
        }
    }
}

fn answer(reply: &Reply) -> Response<Vec<u8>> {
    let mut response = Response::builder()
        .status(StatusCode::from_u16(reply.status).expect("a valid status"))
        .header("content-type", reply.content_type);
    if let Some(location) = &reply.location {
        response = response.header("location", location);
    }
    response.body(reply.body.clone()).expect("a response")
}

/// Serves one connection on a port the OS picked: reads the request, answers 200 promising
/// `promised` body bytes, sends only `sent`, and closes. Returns the speech URL to call.
pub fn cut_short_server(promised: usize, sent: &'static [u8]) -> String {
    use std::io::{Read, Write};

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a local port");
    let addr = listener.local_addr().expect("the listener's address");
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("one connection");
        let mut request = [0u8; 4096];
        let _request_bytes = stream.read(&mut request).expect("the request");
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: audio/wav\r\ncontent-length: {promised}\r\n\r\n"
        );
        stream.write_all(head.as_bytes()).expect("the head");
        stream.write_all(sent).expect("the body's start");
    });
    format!("http://{addr}/v1/audio/speech")
}

/// A local port nothing listens on.
pub fn closed_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a local port");
    listener.local_addr().expect("the port's address").port()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[tokio::test]
    async fn the_listener_stops_while_a_client_holds_an_answer_it_has_not_read() {
        let listener = Listener::start(Reply::audio(vec![0; 32 * 1024 * 1024]));
        let unread = reqwest::Client::new()
            .post(listener.url(""))
            .send()
            .await
            .unwrap();
        let (stopped, stop) = mpsc::channel();

        std::thread::spawn(move || {
            drop(listener);
            stopped.send(()).unwrap();
        });

        assert_eq!(stop.recv_timeout(Duration::from_secs(10)), Ok(()));
        drop(unread);
    }
}
