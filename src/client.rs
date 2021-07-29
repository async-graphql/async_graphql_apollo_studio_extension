// This client representation comes from an article read on:
// https://plume.benboeckel.net/~/JustAnotherBlog/designing-rust-bindings-for-rest-ap-is
use async_trait::async_trait;
use bytes::Bytes;
use http::{Request, Response};
use std::error::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AsyncClientErrors<E>
where
    E: Error + Send + Sync + 'static,
{
    #[error("unknown error: {}", source)]
    Unknown { source: E },
}
/// A trait representing a client which can communicate with a backend service via HTTP
#[async_trait]
pub trait AsyncClient {
    type Error: Error + Send + Sync + 'static;
    /// An async function to call the given Request
    async fn send_endpoint(
        &self,
        request: Request<Vec<u8>>,
    ) -> Result<Response<Bytes>, AsyncClientErrors<Self::Error>>;
}
