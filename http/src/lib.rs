//! Http utilities for the [`bevy_defer`] crate, based on the [`hyper`] crate.
//! 
//! # Runtime
//! 
//! * The executor is the `futures` single threaded `LocalExecutor`.
//! * `async_io` is used as the reactor.
//! 
//! # Features
//! 
//! - [x] Http client.
//! - [ ] Https client.
//! - [ ] Server.
//! - [ ] WASM support.


use std::{future::Future, net::TcpStream};
use async_io::Async;
use bevy_defer::spawn;
pub use smol_hyper::rt::FuturesIo;

/// [`hyper`] executor for [`bevy_defer`].
pub struct BevyDeferExecutor;

use hyper::{body::Body, header::HOST, Request};
use http_body_util::BodyExt;

use hyper::client::conn::http1::handshake;

pub trait HyperHttpClientExt {
    fn http_get(&self, uri: &str) -> impl Future<Output = Result<Vec<u8>, HttpError>> + Send;
    fn http_request<T: Body + 'static>(&self, request: hyper::Request<T>) -> impl Future<Output = Result<Vec<u8>, HttpError>> + Send 
        where T: Send + Sync, T::Data: Send + Sync, T::Error: std::error::Error + Send + Sync;
}

impl<F> hyper::rt::Executor<F> for BevyDeferExecutor where
        F: Future + Send + 'static,
        F::Output: Send + 'static,{
    fn execute(&self, fut: F) {
        bevy_defer::spawn_and_forget(fut);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    HyperError(#[from] hyper::Error),
    #[error("{0}")]
    HttpError(#[from] hyper::http::Error),
    #[error("{0}")]
    InvalidUri(#[from] hyper::http::uri::InvalidUri),
}

impl HyperHttpClientExt for bevy_defer::AsyncWorldMut {

    async fn http_get(&self, uri: &str) -> Result<Vec<u8>, HttpError> {
        let uri = uri.parse::<hyper::Uri>()?;
        let host = uri.host().expect("uri has no host");
        let port = uri.port_u16().unwrap_or(80);
        let address = format!("{}:{}", host, port);
        let stream = Async::<TcpStream>::new(TcpStream::connect(address)?)?;
            let auth = uri.authority().cloned();
            let (mut sender, conn) = handshake::<_, String>(FuturesIo::new(stream)).await.unwrap();
            let _conn = spawn(async move {
                if let Err(err) = conn.await {
                    println!("Connection failed: {:?}", err);
                }
        });
        let req = if let Some(auth) = auth {
            Request::builder()
                .uri(uri)
                .header(HOST, auth.as_str())
                .body(String::new())?
        } else {
            Request::builder()
                .uri(uri)
                .body(String::new())?
        };
        
        let mut resp = sender.send_request(req).await?;
        let mut buffer = Vec::new();
        while let Some(next) = resp.frame().await {
            let frame = next?;
            if let Some(chunk) = frame.data_ref() {
                buffer.extend(chunk);
            }
        }
        Ok(buffer)
    }

    async fn http_request<T: Body + 'static>(&self, request: hyper::Request<T>) -> Result<Vec<u8>, HttpError>
        where T: Send + Sync, T::Data: Send + Sync, T::Error: std::error::Error + Send + Sync
    {
        let host = request.uri().host().expect("uri has no host");
        let port = request.uri().port_u16().unwrap_or(80);
        let address = format!("{}:{}", host, port);
        let stream = Async::<TcpStream>::new(TcpStream::connect(address)?)?;
        let (mut sender, conn) = handshake::<_, T>(FuturesIo::new(stream)).await?;
        let _conn = spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });
        let mut resp = sender.send_request(request).await?;
        let mut buffer = Vec::new();
        while let Some(next) = resp.frame().await {
            let frame = next?;
            if let Some(chunk) = frame.data_ref() {
                buffer.extend(chunk);
            }
        }
        Ok(buffer)
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::AtomicBool;

    use bevy::{app::App, MinimalPlugins};
    use bevy_defer::{world, AsyncExtension, DefaultAsyncPlugin};

    use crate::HyperHttpClientExt;

    #[test]
    fn test() {
        static LOCK: AtomicBool = AtomicBool::new(false);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(DefaultAsyncPlugin);
        app.spawn_task(async {
            world().http_get("http://httpbin.org/ip").await.unwrap();
            LOCK.store(true, std::sync::atomic::Ordering::Relaxed);
            world().quit().await;
            Ok(())
        });
        app.run();
        assert!(LOCK.load(std::sync::atomic::Ordering::Relaxed))
    }
}