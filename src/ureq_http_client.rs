use std::{future::Future, io::Read, pin::Pin};

use gpui::http_client::{
    self, AsyncBody, HttpClient, Inner, Request, Response, Url, http::HeaderValue,
};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";

pub struct UreqHttpClient {
    agent: ureq::Agent,
    user_agent: HeaderValue,
}

impl UreqHttpClient {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .proxy(None)
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            user_agent: HeaderValue::from_static(USER_AGENT),
        }
    }
}

impl HttpClient for UreqHttpClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static>> {
        let agent = self.agent.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = match body.0 {
                Inner::Empty => Vec::new(),
                Inner::Bytes(mut bytes) => {
                    let mut body = Vec::new();
                    bytes.read_to_end(&mut body)?;
                    body
                }
                Inner::AsyncReader(_) => {
                    return Err(http_client::anyhow!(
                        "streaming request bodies are not supported"
                    ));
                }
            };
            let mut response = agent.run(Request::from_parts(parts, body))?;
            let body = response.body_mut().read_to_vec()?;
            Ok(response.map(|_| AsyncBody::from(body)))
        })
    }
}
