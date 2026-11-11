//! The one host capability a CLI Spore needs, behind a trait.
//!
//! Keeping the boundary this narrow means command logic is tested against a
//! fake transport, and each guest's implementation is the only code that knows
//! about `soil:http`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub method: Method,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait Http {
    fn fetch(&mut self, request: Request) -> Result<Response, String>;
}
