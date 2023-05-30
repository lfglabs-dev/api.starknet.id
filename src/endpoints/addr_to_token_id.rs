use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct TokenIdData {
    token_id: String,
}

#[derive(Deserialize)]
pub struct TokenIdQuery {
    addr: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenIdQuery>,
) -> impl IntoResponse {
    let domains = state.db.collection::<mongodb::bson::Document>("domains");
    let addr = &query.addr;

    let document = domains
        .find_one(
            doc! {
                "addr": addr,
                "rev_addr": addr,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    match document {
        Ok(doc) => {
            let mut headers = HeaderMap::new();
            headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

            if let Some(doc) = doc {
                let token_id = doc.get_str("token_id").unwrap_or_default().to_owned();
                let data = TokenIdData { token_id };
                (StatusCode::OK, headers, Json(data)).into_response()
            } else {
                get_error("no main domain found for this address".to_string())
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
