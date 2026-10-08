//! ureq (rustls) implementation of core's `Http`; rustls avoids Wine's system TLS.

use std::io::Read;
use std::time::Duration;

use axigear_core::http::{Http, HttpResponse, TOO_LARGE};

/// Largest body we will read. Current AxiForge comps are gzipped and small, but
/// older uncompressed ones that embed skill catalogs can reach ~20 MB.
const MAX_BODY: u64 = 32 * 1024 * 1024;

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            // A stalled read fails fast; a big download that keeps moving gets a minute.
            .timeout_read(Duration::from_secs(8))
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("arcdps_axigear/", env!("CARGO_PKG_VERSION")))
            .build();
        UreqHttp { agent }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for UreqHttp {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String> {
        let mut req = self.agent.get(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let resp = match req.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(e.to_string()),
        };
        let status = resp.status();
        let etag = resp.header("ETag").map(str::to_owned);
        let mut body = Vec::new();
        resp.into_reader().take(MAX_BODY + 1).read_to_end(&mut body).map_err(|e| e.to_string())?;
        if body.len() as u64 > MAX_BODY {
            return Err(TOO_LARGE.into());
        }
        Ok(HttpResponse { status, body, etag })
    }
}
