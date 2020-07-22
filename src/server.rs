use crate::config::{Config, OctaneConfig};
use crate::constants::*;
use crate::error::Error;
use crate::inject_method;
use crate::path::PathBuf;
use crate::request::{parse_without_body, Headers, Request, RequestLine, RequestMethod};
use crate::responder::Response;
use crate::router::{Closure, Flow, Route, Router, RouterResult};
use crate::util::find_in_slice;
use std::io::Result;
use std::marker::Unpin;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf as StdPathBuf;
use std::str;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{copy, AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::stream::StreamExt;

#[macro_use]
macro_rules! declare_error {
    ($stream: expr, $error_type: expr) => {
        Error::err($error_type).send($stream).await?;
    };
}

/// The octane server
///
/// # Example
/// ```rust,no_run
/// #[tokio::main]
/// async fn main() {
///     let mut app = Octane::new();
///     app.get(
///         "/",
///         route!(
///             |req, res| {
///                 res.send("Hello, World");
///             }
///         ),
///     );
///
///     app.listen(8080).await.expect("Cannot establish connection");
/// }
/// ```
pub struct Octane {
    settings: OctaneConfig,
    router: Router,
}

impl Route for Octane {
    fn options(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::Options);
        Ok(())
    }
    fn connect(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::Connect);
        Ok(())
    }
    fn head(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::Head);
        Ok(())
    }
    fn put(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::Put);
        Ok(())
    }
    fn get(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::Get);
        Ok(())
    }
    fn post(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::Post);
        Ok(())
    }
    fn all(&mut self, _path: &str, _closure: Closure) -> RouterResult {
        // TODO: Multiple inject_method! declarations here
        Ok(())
    }

    fn add(&mut self, closure: Closure) -> RouterResult {
        inject_method!(self.router, "/*", closure, &RequestMethod::All);
        Ok(())
    }
    fn add_route(&mut self, path: &str, closure: Closure) -> RouterResult {
        inject_method!(self.router, path, closure, &RequestMethod::All);
        Ok(())
    }
}

impl Config for Octane {
    fn set_keepalive(&mut self, duration: Duration) {
        self.settings.keep_alive = Some(duration);
    }
    fn add_static_dir(&mut self, loc: &'static str, dir_name: &'static str) {
        let loc_buf = StdPathBuf::from(loc);
        let dir_buf = StdPathBuf::from(dir_name);
        if let Some(paths) = self.settings.static_dir.get_mut(&loc_buf) {
            paths.push(dir_buf)
        } else {
            self.settings.static_dir.insert(loc_buf, vec![dir_buf]);
        }
    }
}
impl Octane {
    /// Creates a new server instance
    pub fn new() -> Self {
        Octane {
            settings: OctaneConfig::new(),
            router: Router::new(),
        }
    }
    /// **Appends** the router routes to the routes that
    /// the server instance holds, this allows you to
    /// independently add routes to a route Router structure
    /// and then use it with the server struct
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let mut app = Octane::new();
    /// let mut router = Router::new();
    /// router.get("/", route!(|req, res| { res.send("It's a get request!!") }));
    /// router.post("/", route!(|req, res| { res.send("It's a post request!!") }));
    /// app.use_router(router);
    /// ```
    ///
    /// Note that it appends, meaning if you have 3 routes in
    /// Router struct and 3 routes in the Octane struct,
    /// you'll have total 3 + 3 routes in the Octane struct.
    pub fn use_router(&mut self, _router: Router) {
        // FIXME: this function
        // self.router = router.append(self.router);
    }
    /// Appends the config of the Octane struct with a custom
    /// generated one. The Octane struct contains an OctaneConfig
    /// instance by default
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let mut app = Octane::new();
    /// let mut config = OctaneConfig::new();
    /// config.ssl.key("key.pem").cert("cert.pem"); // we supply some ssl certs and key in the config
    /// app.add_static_dir("/", "templates");
    /// app.with_config(config);
    /// ```
    ///
    /// **Note**: While it replaces properties that must be unique
    /// i.e which can only have one value at a time, so for
    /// static_dirs, it appends the locations defined in config
    /// with the settings that Octane struct already has
    pub fn with_config(&mut self, config: OctaneConfig) {
        self.settings.append(config);
    }

    /// Start listening on the port specified
    ///
    /// # Example
    /// ```rust,no_run
    /// let mut app = Octane::new();
    /// app.listen(8080).await.expect("Cannot establish connection");
    /// ```
    pub async fn listen(self, port: u16) -> std::io::Result<()> {
        let mut listener =
            TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port)).await?;
        let server = Arc::new(self);
        #[cfg(feature = "rustls")]
        {
            use tokio_rustls::{
                rustls::{NoClientAuth, ServerConfig},
                TlsAcceptor,
            };
            println!("{:?}", server.settings.get_key());
            let mut config = ServerConfig::new(NoClientAuth::new());
            // PANIC: Here
            config
                .set_single_cert(
                    server.settings.get_cert()?,
                    server.settings.get_key()?.remove(0),
                )
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            let acceptor = TlsAcceptor::from(Arc::new(config));

            while let Some(stream) = StreamExt::next(&mut listener).await {
                let server_clone = Arc::clone(&server);
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    match stream {
                        Ok(value) => {
                            let stream = acceptor.accept(value).await;
                            match stream {
                                Ok(stream_ssl) => {
                                    Self::catch_request(stream_ssl, server_clone).await;
                                }
                                Err(e) => panic!("{:?}", e),
                            }
                        }
                        Err(_e) => (),
                    };
                });
            }
        }
        #[cfg(feature = "openSSL")]
        {
            use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
            acceptor.set_private_key_file(&server.settings.ssl.key, SslFiletype::PEM)?;
            acceptor.set_certificate_chain_file(&server.settings.ssl.cert)?;
            let acceptor = acceptor.build();
            while let Some(stream) = StreamExt::next(&mut listener).await {
                let server_clone = Arc::clone(&server);
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    match stream {
                        Ok(value) => {
                            let stream = tokio_openssl::accept(&acceptor, value).await;
                            match stream {
                                Ok(stream_ssl) => {
                                    Self::catch_request(stream_ssl, server_clone).await;
                                }
                                Err(e) => panic!("{:?}", e),
                            }
                        }
                        Err(_e) => (),
                    };
                });
            }
        }
        #[cfg(not(any(feature = "openSSL", feature = "rustls")))]
        {
            while let Some(stream) = StreamExt::next(&mut listener).await {
                let server_clone = Arc::clone(&server);
                tokio::spawn(async move {
                    match stream {
                        Ok(value) => {
                            let _res = Self::catch_request(value, server_clone).await;
                        }
                        Err(_e) => (),
                    };
                });
            }
        }
        Ok(())
    }

    async fn catch_request<S>(mut stream_async: S, server: Arc<Octane>) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let mut data = Vec::<u8>::new();
        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let body: &[u8];
        let request_line: RequestLine;
        let headers: Headers;
        let body_remainder: &[u8];
        loop {
            let read = stream_async.read(&mut buf).await?;
            if read == 0 {
                declare_error!(stream_async, StatusCode::BadRequest);
                return Ok(());
            }
            let cur = &buf[..read];
            data.extend_from_slice(cur);
            if let Some(i) = find_in_slice(&data[..], b"\r\n\r\n") {
                let first = &data[..i];
                body_remainder = &data[i + 4..];
                if let Ok(decoded) = str::from_utf8(first) {
                    if let Some((rl, heads)) = parse_without_body(decoded) {
                        request_line = rl;
                        headers = heads;
                        break;
                    } else {
                        declare_error!(stream_async, StatusCode::BadRequest);
                        return Ok(());
                    }
                } else {
                    declare_error!(stream_async, StatusCode::BadRequest);
                    return Ok(());
                }
            }
        }
        let body_len = headers
            .get("content-length")
            .map(|s| s.parse().unwrap_or(0))
            .unwrap_or(0);
        let mut body_vec: Vec<u8>;
        if body_len > 0 {
            if body_remainder.len() < body_len {
                let mut temp: Vec<u8> = vec![0; body_len - body_remainder.len()];
                stream_async.read_exact(&mut temp[..]).await?;
                body_vec = Vec::with_capacity(body_len);
                body_vec.extend_from_slice(body_remainder);
                body_vec.extend_from_slice(&temp[..]);
                body = &body_vec[..];
            } else {
                body = body_remainder;
            }
        } else {
            body = &[];
        }
        if let Some(parsed_request) = Request::parse(request_line, headers, body) {
            // TODO: Apply keepalive when you don't have TLS support
            // #[cfg(not(any(feature = "rustls", feature = "openSSL")))]
            // {
            //     if let Some(connection_type) = parsed_request.headers.get("connection") {
            //         if connection_type.to_lowercase() == "keep-alive" {
            //             if parsed_request.request_line.version == HttpVersion::Http10 {
            //                 if let Some(keep_alive_header) =
            //                     parsed_request.headers.get("keep-alive")
            //                 {
            //                     let header_details = KeepAlive::parse(keep_alive_header);

            //                     stream_async.set_keepalive(Some(Duration::from_secs(
            //                         header_details.timeout.unwrap_or(0),
            //                     )))?;
            //                 }
            //             } else if parsed_request.request_line.version == HttpVersion::Http11 {
            //                 stream_async.set_keepalive(server.settings.keep_alive)?;
            //             }
            //         }
            //     }
            // }
            let mut res = Response::new(b"");
            let req = &parsed_request.request_line;
            if req.method.is_some() {
                let mut counter = Flow::Next;
                if let Some(functions) = server.router.paths.get(&req.method) {
                    if counter.should_continue() {
                        for matched in functions.get(&req.path).into_iter() {
                            counter = (matched.data.closure)(&parsed_request, &mut res).await;
                        }
                    }
                }
                // run RequestMethod::All regardless of the request method
                if counter.should_continue() {
                    if let Some(functions) = server.router.paths.get(&RequestMethod::All) {
                        for matched in functions.get(&req.path).into_iter() {
                            counter = (matched.data.closure)(&parsed_request, &mut res).await;
                        }
                    }
                    let mut parent_path = req.path.clone();
                    let poped = parent_path.chunks.pop();
                    for loc in server.settings.static_dir.iter() {
                        let mut matched = true;
                        for (i, chunks) in loc.0.iter().enumerate() {
                            if let Some(val) = parent_path.chunks.get(i) {
                                if val != chunks.to_str().unwrap_or("") {
                                    matched = false
                                }
                            }
                        }
                        if matched {
                            for dirs in loc.1.iter() {
                                if req.method == RequestMethod::Get {
                                    let mut dir_final = dirs.clone();
                                    dir_final.push(poped.clone().unwrap_or(String::new()));
                                    res.send_file(dir_final).await?;
                                }
                            }
                        }
                    }
                }

                Self::send_data(res.get_data(), stream_async).await?;
            } else {
                declare_error!(stream_async, StatusCode::NotImplemented);
            }
        } else {
            declare_error!(stream_async, StatusCode::BadRequest);
        }
        Ok(())
    }
    pub async fn send_data<S>(response: Vec<u8>, mut stream_async: S) -> std::io::Result<()>
    where
        S: AsyncRead + AsyncWrite + std::marker::Unpin,
    {
        copy(&mut &response[..], &mut stream_async).await?;
        Ok(())
    }
}

impl Default for Octane {
    fn default() -> Self {
        Self::new()
    }
}
