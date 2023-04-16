use std::{
    pin::Pin,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::{
    net::TcpListener,
    sync::{oneshot, oneshot::Sender},
};
use hyper::{
    Uri,
    Request,
    Response,
    Method,
    body::{Frame, Bytes},
    server::conn::http1,
    service::service_fn,
};
use http_body_util::{BodyExt, Full};
use tokio_openssl::SslStream;
use openssl::ssl::{SslAcceptor, SslMethod, Ssl};

pub mod certs;
use certs::Identity;

//  For now, proxy makes use of minimal custom client as hyper-util is not complete
pub mod client;
use client::Client;
pub mod logs;
use logs::*;


pub enum ProxyState {
    Serving,
    Stopped,
    Error,
}

#[derive(Debug, Clone)]
struct ProxyCtx {
    pub ca: Identity,
}

#[derive(Debug, Clone)]
pub struct ProxyHandle {
    addr: SocketAddr,
    shutdown: Arc<Mutex<Option<Sender<()>>>>,
    http_client: Client,
    ctx: ProxyCtx,
    is_serving: bool,
}

impl ProxyHandle {
    pub fn new(addr: SocketAddr, ca: Identity) -> Self {
        let client = Client::new();

        Self {
            addr: addr,
            shutdown: Arc::new(Mutex::new(None)),
            ctx: ProxyCtx {
                ca: ca
            },
            http_client: client,
            is_serving: false,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown.lock().unwrap().take() {
            let _ = tx.send(()).map_err(|_err| {
                    //error!("{e:?}")
                });
            self.is_serving = false;
        }
    }

    pub async fn serve(&mut self) {
        if self.is_serving { return }

        let (tx, rx) = oneshot::channel();
        let _ = self.shutdown.lock().unwrap().insert(tx);

        let listener = match TcpListener::bind(self.addr).await {
            Ok(listener) => listener,
            Err(err) => return,
        };

        self.is_serving = true;
        let ctx = self.ctx.clone();
        let client = self.http_client.clone();

        tokio::select! {
            _ = rx => {
                //info!("shutting down proxy service");
                }
            _ = async move {
                loop {
                    let (stream, sckaddr) = listener.accept()
                        .await
                        .unwrap();

                    let service = service_fn(|req| {
                        dispatch_req(req, ctx.clone(), client.clone())
                    });

                    //trace!("serving proxy to {sckaddr}");
                    http1::Builder::new()
                        .serve_connection(stream, service)
                        .with_upgrades()
                        .await
                        .map_err(|_err| {
                            //error!("Failed to serve connection: {err}")
                            });
                }
            } => {
                //error!("failed to accept connection, aborting instance");
                }
        };
    }
}

async fn dispatch_req(
    req: Request<hyper::body::Incoming>,
    ctx: ProxyCtx,
    client: Client,
) -> Result<Response<Full<Bytes>>, Infallible> {
    
    let uri = req.uri();
    if *req.method() == Method::CONNECT &&
        is_valid_https_upgrade_uri(uri) {

        let port = uri.port().unwrap();
        let host = uri.host().unwrap();
        let remote_addr = format!("{host}:{port}"); 

        let entity_certificate = Identity::entity_certificate(&ctx.ca, &remote_addr)
            .unwrap();

        tokio::task::spawn(async move {
            if let Err(_) = upgrade_to_tls(req, entity_certificate, remote_addr, client).await {
                //error!("failed to serve https");
            }
        });

        return Ok(Response::default())
    }   //  TODO: forwarding or bad request

    forward_http(req, None, client.clone()).await
}

async fn upgrade_to_tls(
    req: Request<hyper::body::Incoming>,
    entity_certificate: Identity,
    remote_addr: String,
    client: Client,
) -> Result<(), ()> {
    match hyper::upgrade::on(req).await {
        Ok(to_upgrade) => {
            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())
                .unwrap();
            acceptor
                .set_certificate(entity_certificate.cert_ref())
                .unwrap();
            acceptor
                .set_private_key(entity_certificate.key_ref())
                .unwrap();

            let acceptor = acceptor.build();
            let tls = Ssl::new(acceptor.context()).unwrap();
            let mut tls_stream = SslStream::new(tls, to_upgrade)
                .unwrap();

            Pin::new(&mut tls_stream).accept().await
                .map_err(|err| println!("{err}"))?;

            http1::Builder::new() 
                .serve_connection(tls_stream, service_fn(|req| {
                    forward_http(req, Some(&remote_addr), client.clone())
                }))
                .with_upgrades()
                .await
                .map_err(|err| println!("{err}"))?;
        }
        Err(err) => {
            println!("Failed to upgrade to https: {err}");
        }
    }

    Ok(())
}

fn is_valid_https_upgrade_uri(uri: &Uri) -> bool {
    uri.scheme().is_none() &&
    uri.port().is_some() &&
    uri.host().is_some() &&
    uri.path_and_query().is_none()
}

async fn forward_http(
    mut req: Request<hyper::body::Incoming>,
    remote_addr: Option<&str>,
    client: Client,
) -> Result<Response<Full<Bytes>>, Infallible> {
    //  Convert request for reqwest as uri is partial for https
    if let Some(addr) = remote_addr {
        let path_and_query = req.uri()
            .path_and_query()
            .map_or_else(|| "", |path| path.as_str());

        //  FIXME: 400 instead of unwrap
        *req.uri_mut() = format!("https://{addr}/{path_and_query}").parse().unwrap();
    }

    let (parts, body) = req.into_parts();

    //  TODO: clean ths code
    let body = body.map_frame(|frame| { Frame::data(frame.into_data().unwrap_or_default()) });

    //  Note: there's no plan on updating the basic http client, waiting hyper-util client
    //  for further functionnality support
    let req = Request::from_parts(parts, body.boxed());

    let resp = client.execute(req).await;

    resp
}
