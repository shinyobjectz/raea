//! The decision model, called one way for every caller (docs/ask.feature AK1b): a call's body and its answers
//! (`client`), the media cut to what it reads (`media`), the route it takes, the person's plan first (`route`), and the
//! answers kept by request (`cache`). `raea ask` (apps/ask) and `raea judge` call it.

pub mod cache;
pub mod client;
pub mod media;
pub mod route;

pub use client::{Answered, Call, Fail, Http, Media, Transport};
pub use route::Route;

/// One call, by the routes open on this computer.
pub fn ask(call: &Call) -> Result<Answered, Fail> {
    route::ask(&Http, call, &route::open())
}

#[cfg(test)]
mod tests;
