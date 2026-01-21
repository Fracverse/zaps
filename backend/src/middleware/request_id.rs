use axum::{extract::Request, http::HeaderName, middleware::Next, response::Response};
use uuid::Uuid;

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub async fn request_id(mut req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get(&X_REQUEST_ID)
        .and_then(|id| id.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(request_id.clone());

    let mut res = next.run(req).await;

    res.headers_mut()
        .insert(X_REQUEST_ID.clone(), request_id.parse().unwrap());

    res
}
