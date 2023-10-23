use crate::{models::AppState, utils::to_hex};
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

#[derive(Serialize, Debug)]
pub struct VerifierData {
    verifier: String,
    field: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenIdQuery>,
) -> impl IntoResponse {
    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let id_verifier_data = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_verifier_data");

    // Query the domains collection
    let domain_filter = doc! {
        "id": to_hex(&query.id),
        "_cursor.to": null
    };
    let domain_data = domains.find_one(domain_filter, None).await.unwrap();

    // Query the id_verifier_data collection
    let verifier_filter = doc! {
        "id": to_hex(&query.id),
        "$or": [
            { "_cursor.to": null },
            { "_cursor.to": { "$exists": false } }
        ],
        "verifier" : "0x070378cc131622ed8099a1e47adcd0c76341c206dea05917a8dcb6896b0c6601",
        "field": "0x00000000000000000000000000000000006e66745f70705f636f6e7472616374",
    };

    let verifier_data: Option<VerifierData> =
        match id_verifier_data.find_one(verifier_filter, None).await {
            Ok(Some(vi)) => {
                let verifier = vi.get_str("verifier").unwrap_or_default().to_string();
                let field = vi.get_str("field").unwrap_or_default().to_string();
                Some(VerifierData { verifier, field })
            }
            _ => None,
        };
    println!("verifier_data; {:?}", verifier_data);

    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    match domain_data {
        Some(doc) => {
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
        }
        None => {
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
}
