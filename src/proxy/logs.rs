use hyper::{
    body::Bytes,
    Request,
    Response,
};

pub struct HttpRecord {
    request: Request<Bytes>,
    response: Response<Bytes>,
}

pub struct ProxyLogs {
    records: Vec<HttpRecord>,
}

impl ProxyLogs {
    pub fn new() -> Self {
        Self {
            records: Vec::with_capacity(1024),
        }
    }
}
