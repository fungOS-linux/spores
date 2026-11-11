//! The wasm guest: `soil:http` in, `run(args)` out.

wit_bindgen::generate!({ path: "wit", world: "spotify", generate_all });

use cli_http::{Http, Method, Request, Response};

struct Component;

export!(Component);

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<String, String> {
        crate::run(&args, &mut HostHttp)
    }
}

struct HostHttp;

impl Http for HostHttp {
    fn fetch(&mut self, request: Request) -> Result<Response, String> {
        use soil::http::http as abi;

        let method = match request.method {
            Method::Get => abi::Method::Get,
            Method::Put => abi::Method::Put,
            Method::Post => abi::Method::Post,
        };
        let response = abi::fetch(&abi::Request {
            url: request.url,
            method,
            headers: request
                .headers
                .into_iter()
                .map(|(name, value)| abi::Header { name, value })
                .collect(),
            body: request.body,
            // The host turns this handle into `Authorization: Bearer …`. The
            // token never enters the guest.
            credential: Some("default".into()),
            max_bytes: crate::MAX_BODY,
            timeout_ms: crate::TIMEOUT_MS,
        })
        .map_err(|error| format!("http: {error:?}"))?;

        Ok(Response {
            status: response.status,
            body: response.body,
        })
    }
}
