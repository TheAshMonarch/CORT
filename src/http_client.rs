use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{AppError, AppResult};

/// Performs an HTTP GET request to a remote Cloud URL and deserializes the JSON response.
pub async fn get_json<R: DeserializeOwned>(url: &str) -> AppResult<R> {
    let client: Client<HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let uri: hyper::Uri = url
        .parse()
        .map_err(|e| AppError::BadGateway(format!("Invalid cloud URL '{url}': {e}")))?;

    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("accept", "application/json")
        .body(Full::new(Bytes::new()))
        .map_err(|e| AppError::BadGateway(format!("Failed to build GET request: {e}")))?;

    let resp = client
        .request(req)
        .await
        .map_err(|e| AppError::BadGateway(format!("Cloud GET request to '{url}' failed: {e}")))?;

    let status = resp.status();
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| AppError::BadGateway(format!("Failed reading Cloud response body: {e}")))?
        .to_bytes();

    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&body_bytes);
        return Err(AppError::BadGateway(format!(
            "Cloud server returned HTTP {status}: {body_text}"
        )));
    }

    let parsed = serde_json::from_slice::<R>(&body_bytes)?;
    Ok(parsed)
}

/// Performs an HTTP POST request with a JSON payload to a remote Cloud URL and deserializes the JSON response.
pub async fn post_json<B: Serialize, R: DeserializeOwned>(url: &str, payload: &B) -> AppResult<R> {
    let client: Client<HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let uri: hyper::Uri = url
        .parse()
        .map_err(|e| AppError::BadGateway(format!("Invalid cloud URL '{url}': {e}")))?;

    let payload_bytes = serde_json::to_vec(payload)?;

    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .body(Full::new(Bytes::from(payload_bytes)))
        .map_err(|e| AppError::BadGateway(format!("Failed to build POST request: {e}")))?;

    let resp = client
        .request(req)
        .await
        .map_err(|e| AppError::BadGateway(format!("Cloud POST request to '{url}' failed: {e}")))?;

    let status = resp.status();
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| AppError::BadGateway(format!("Failed reading Cloud response body: {e}")))?
        .to_bytes();

    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&body_bytes);
        return Err(AppError::BadGateway(format!(
            "Cloud server returned HTTP {status}: {body_text}"
        )));
    }

    let parsed = serde_json::from_slice::<R>(&body_bytes)?;
    Ok(parsed)
}
