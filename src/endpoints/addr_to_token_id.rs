use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct TokenIdData {
    token_id: String,
}

#[derive(Deserialize)]
pub struct TokenIdQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenIdQuery>,
) -> impl IntoResponse {
    let domains = state.db.collection::<mongodb::bson::Document>("domains");
    let addr = to_hex(&query.addr);

    let document = domains
        .find_one(
            doc! {
                "legacy_address": &addr,
                "rev_address": &addr,
                "_cursor.to": Bson::Null,
            },
            None,
        )
        .await;

    match document {
        Ok(doc) => {
            let mut headers = HeaderMap::new();
            headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

            if let Some(doc) = doc {
                let id = FieldElement::from_hex_be(doc.get_str("id").unwrap_or_default())
                    .unwrap()
                    .to_string();
                let data = TokenIdData { token_id: id };
                (StatusCode::OK, headers, Json(data)).into_response()
            } else {
                get_error("no main domain found for this address".to_string())
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
