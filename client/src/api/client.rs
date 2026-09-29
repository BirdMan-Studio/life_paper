#[derive(Clone)]
pub struct ApiClient {
    pub(crate) http: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: normalize_base_url(base_url.into()),
        }
    }

    pub fn set_base_url(&mut self, base_url: impl Into<String>) {
        self.base_url = normalize_base_url(base_url.into());
    }

    pub(crate) fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

fn normalize_base_url(base_url: String) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}
