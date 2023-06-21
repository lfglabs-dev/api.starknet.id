use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorMessage {
    error: String,
}

pub fn get_error(error: String) -> Response {
    (StatusCode::BAD_REQUEST, error).into_response()
}
