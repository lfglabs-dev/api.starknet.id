use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use chrono::NaiveDateTime;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct TokenURI {
    name: String,
    description: String,
    image: String,
    expiry: Option<i64>,
    attributes: Option<Vec<Attribute>>,
}

#[derive(Serialize)]
pub struct Attribute {
    trait_type: String,
    value: Vec<String>,
}

#[derive(Deserialize)]
pub struct TokenIdQuery {
    id: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenIdQuery>,
) -> impl IntoResponse {
    let domains = state.starknetid_db.collection::<mongodb::bson::Document>("domains");

    let document = domains
        .find_one(
            doc! {
                "id": to_hex(&query.id),
                "_cursor.to": null,
            },
            None,
        )
        .await;

    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    match document {
        Ok(doc) => {
            if let Some(doc) = doc {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                let expiry = doc.get_i64("expiry").unwrap_or_default();
                let token_uri = TokenURI {
                    name: domain.clone(),
                    description: "This token represents an identity on StarkNet.".to_string(),
                    image: format!("https://starknet.id/api/identicons/{}", &query.id),
                    expiry: Some(expiry),
                    attributes: Some(vec![
                        Attribute {
                            trait_type: "Subdomain".to_string(),
                            value: vec![if domain.contains(".") { "yes" } else { "no" }.to_string()],
                        },
                        Attribute {
                            trait_type: "Domain expiry".to_string(),
                            value: vec![NaiveDateTime::from_timestamp_opt(expiry.into(), 0)
                                .map(|dt| dt.format("%b %d, %Y").to_string())
                                .unwrap_or_else(|| "Invalid date".into())],
                        },
                        Attribute {
                            trait_type: "Domain expiry timestamp".to_string(),
                            value: vec![expiry.to_string()],
                        },
                    ]),
                };
                (StatusCode::OK, headers, Json(token_uri)).into_response()
            } else {
                let token_uri = TokenURI {
                    name: format!("Starknet ID: {}", &query.id),
                    description: "This token represents an identity on StarkNet.".to_string(),
                    image: format!("https://starknet.id/api/identicons/{}", &query.id),
                    expiry: None,
                    attributes: None,
                };
                (StatusCode::OK, headers, Json(token_uri)).into_response()
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
