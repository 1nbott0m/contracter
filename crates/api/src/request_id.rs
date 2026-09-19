use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_LENGTH, CONTENT_TYPE},
    },
    middleware::Next,
    response::Response,
};
use serde_json::{Value, json};
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// The current request's correlation id, available to any handler via the
/// `RequestId` extractor once `request_id_middleware` has run.
#[derive(Debug, Clone, Copy)]
pub struct RequestId(pub Uuid);

/// Outermost middleware: assigns (or trusts an already-valid incoming)
/// correlation id, makes it available to handlers via request extensions,
/// stamps every response with the `x-request-id` header, and -- for any
/// 4xx/5xx response, regardless of which inner layer produced it (a
/// handler's `ApiError`, a timeout, a panic, a routing 404) -- guarantees
/// the JSON body carries the same id and matches the stable error
/// envelope shape.
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let incoming = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok());
    let id = incoming.unwrap_or_else(Uuid::new_v4);
    request.extensions_mut().insert(RequestId(id));

    let response = next.run(request).await;
    stamp_response(response, id).await
}

async fn stamp_response(response: Response, id: Uuid) -> Response {
    let (mut parts, body) = response.into_parts();
    parts.headers.insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&id.to_string()).expect("a UUID is always a valid header value"),
    );

    if !parts.status.is_client_error() && !parts.status.is_server_error() {
        return Response::from_parts(parts, body);
    }

    // The envelope's supported codes are the fixed 400/401/403/404/409/
    // 422/429/500/503 set. A status outside that set (e.g. 413 from
    // tower_http's RequestBodyLimit, which this middleware does not
    // control) is normalized here so the reported status and JSON code
    // never disagree.
    let normalized_status = normalize_status(parts.status);
    parts.status = normalized_status;

    // A body that fails to read (exceeds MAX_ERROR_BODY_BYTES, or errors
    // mid-stream) still gets a correct, fresh envelope here rather than
    // an empty body left under the original response's stale
    // Content-Length -- that combination is an HTTP framing mismatch,
    // not just a cosmetic gap.
    let envelope = match to_bytes(body, MAX_ERROR_BODY_BYTES).await {
        Ok(bytes) => patch_or_rebuild_envelope(&parts.headers, &bytes, normalized_status, id),
        Err(_) => fallback_envelope(normalized_status, id),
    };
    let payload = serde_json::to_vec(&envelope).unwrap_or_default();

    parts
        .headers
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    parts.headers.remove(CONTENT_LENGTH);

    Response::from_parts(parts, Body::from(payload))
}

const SUPPORTED_STATUSES: [StatusCode; 9] = [
    StatusCode::BAD_REQUEST,
    StatusCode::UNAUTHORIZED,
    StatusCode::FORBIDDEN,
    StatusCode::NOT_FOUND,
    StatusCode::CONFLICT,
    StatusCode::UNPROCESSABLE_ENTITY,
    StatusCode::TOO_MANY_REQUESTS,
    StatusCode::INTERNAL_SERVER_ERROR,
    StatusCode::SERVICE_UNAVAILABLE,
];

fn normalize_status(status: StatusCode) -> StatusCode {
    if SUPPORTED_STATUSES.contains(&status) {
        return status;
    }
    match status {
        StatusCode::PAYLOAD_TOO_LARGE => StatusCode::BAD_REQUEST,
        _ if status.is_server_error() => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    }
}

fn patch_or_rebuild_envelope(
    headers: &axum::http::HeaderMap,
    body: &[u8],
    status: StatusCode,
    id: Uuid,
) -> Value {
    let is_json = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));

    if is_json
        && let Ok(Value::Object(mut root)) = serde_json::from_slice::<Value>(body)
        && let Some(Value::Object(error)) = root.get_mut("error")
        && error.get("code").is_some_and(Value::is_string)
        && error.get("message").is_some_and(Value::is_string)
    {
        error.insert("request_id".to_owned(), json!(id));
        return Value::Object(root);
    }

    fallback_envelope(status, id)
}

fn fallback_envelope(status: StatusCode, id: Uuid) -> Value {
    let code = match status {
        StatusCode::BAD_REQUEST => "BAD_REQUEST",
        StatusCode::UNAUTHORIZED => "UNAUTHORIZED",
        StatusCode::FORBIDDEN => "FORBIDDEN",
        StatusCode::NOT_FOUND => "NOT_FOUND",
        StatusCode::CONFLICT => "CONFLICT",
        StatusCode::UNPROCESSABLE_ENTITY => "UNPROCESSABLE_ENTITY",
        StatusCode::TOO_MANY_REQUESTS => "TOO_MANY_REQUESTS",
        StatusCode::SERVICE_UNAVAILABLE => "SERVICE_UNAVAILABLE",
        _ => "INTERNAL_SERVER_ERROR",
    };
    let message = if status.is_server_error() {
        "Internal server error"
    } else {
        "Request could not be processed"
    };
    json!({ "error": { "code": code, "message": message, "request_id": id } })
}
