//! The only door to the network. The plugin implements it with ureq; tests
//! use `fake::FakeHttp`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub etag: Option<String>,
}

pub trait Http: Send + Sync {
    /// GET `url`. `Err` is a transport failure (DNS, TLS, timeout); any HTTP
    /// status, including 404 and 5xx, is `Ok`.
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String>;
}

#[cfg(test)]
pub(crate) mod fake {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    use super::{Http, HttpResponse};

    type Call = (String, Vec<(String, String)>);

    #[derive(Default)]
    pub struct FakeHttp {
        routes: Mutex<HashMap<String, VecDeque<Result<HttpResponse, String>>>>,
        calls: Mutex<Vec<Call>>,
    }

    impl FakeHttp {
        pub fn new() -> Self {
            Self::default()
        }

        fn push(&self, url: &str, r: Result<HttpResponse, String>) -> &Self {
            self.routes.lock().unwrap().entry(url.to_string()).or_default().push_back(r);
            self
        }

        /// Queue a response. The last one queued for a URL repeats forever.
        pub fn on(&self, url: &str, status: u16, body: &str) -> &Self {
            self.push(url, Ok(HttpResponse { status, body: body.into(), etag: None }))
        }

        pub fn on_etag(&self, url: &str, status: u16, body: &str, etag: &str) -> &Self {
            self.push(url, Ok(HttpResponse { status, body: body.into(), etag: Some(etag.into()) }))
        }

        pub fn fail(&self, url: &str, err: &str) -> &Self {
            self.push(url, Err(err.into()))
        }

        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().iter().map(|(u, _)| u.clone()).collect()
        }

        pub fn header(&self, call: usize, name: &str) -> Option<String> {
            let calls = self.calls.lock().unwrap();
            calls.get(call)?.1.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
        }
    }

    impl Http for FakeHttp {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String> {
            let owned = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            self.calls.lock().unwrap().push((url.to_string(), owned));
            let mut routes = self.routes.lock().unwrap();
            match routes.get_mut(url) {
                Some(q) if q.len() > 1 => q.pop_front().unwrap(),
                Some(q) if !q.is_empty() => q[0].clone(),
                _ => Ok(HttpResponse { status: 404, body: String::new(), etag: None }),
            }
        }
    }
}
