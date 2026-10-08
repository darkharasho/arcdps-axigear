//! ureq (rustls) implementation of core's `Http`; rustls avoids Wine's system TLS.

use std::time::Duration;

use axigear_core::http::{Http, HttpResponse};

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(8))
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
        let body = resp.into_string().map_err(|e| e.to_string())?;
        Ok(HttpResponse { status, body, etag })
    }
}
