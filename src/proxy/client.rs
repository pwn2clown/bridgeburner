use std::convert::Infallible;
use hyper::{
    Request,
    Response,
    body::Bytes,
    client::conn,
};
use http_body_util::{Full, BodyExt, combinators::BoxBody};
use http::uri::Scheme;
use tokio::net::TcpStream;

#[derive(Debug, Clone)]
pub struct Client {}

impl Client {
    pub fn new() -> Self {
        Self {}
    }

    //  Requires absolute URI for now, set sockaddr as param later
    //  Make use of BoxBody for forward compatibilty with upcoming hyper client
    pub async fn execute(&self, req: Request<BoxBody<Bytes, hyper::Error>>) -> Result<Response<Full<Bytes>>, Infallible> {
        //  TODO: Assert url is complete
        //  FIXME: bad request if user/password in authority: try with into sockaddr
        let uri = req.uri();
        let scheme = uri.scheme().unwrap();
        let port = uri.port_u16().unwrap_or_else(|| match scheme {
                HTTP => 80,
                HTTPS => 443,
            });

        let target_addr = format!("{}:{}", uri.host().unwrap(), port);
        let target_stream = TcpStream::connect(target_addr)
            .await
            .unwrap();  //  FIXME 503

        let (mut req_sender, conn) = conn::http1::handshake(target_stream)
            .await
            .unwrap();

        tokio::spawn(async move {
            if let Err(err) = conn.await {
                //trace{err}");
            }
        });

        let response = req_sender.send_request(req)
            .await
            .unwrap();

        let (parts, body) = response.into_parts();
        let body = Full::new(body.collect().await.unwrap().to_bytes());

        let resp = Response::from_parts(parts, body);
        Ok(resp)
    }
}
