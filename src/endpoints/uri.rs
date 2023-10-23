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
use futures::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
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

    let aggregation_pipeline = vec![
        doc! {
            "$match": {
                "id": to_hex(&query.id),
                "_cursor.to": null,
            }
        },
        doc! {
            "$lookup": {
                "from": "id_verifier_data",
                "let": { "token_id": "$id" },
                "pipeline": [
                    {
                        "$match": {
                            "$expr": {
                                "$eq": ["$id", "$$token_id"]
                            },
                            "$or": [
                                { "_cursor.to": null },
                                { "_cursor.to": { "$exists": false } }
                            ],
                            "verifier" : "0x070378cc131622ed8099a1e47adcd0c76341c206dea05917a8dcb6896b0c6601",
                            "field": "0x00000000000000000000000000000000006e66745f70705f636f6e7472616374",
                        }
                    }
                ],
                "as": "verifier_info"
            }
        },
    ];

    let options = AggregateOptions::default();
    let mut cursor = domains
        .aggregate(aggregation_pipeline, options)
        .await
        .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    if let Some(result) = cursor.next().await {
        match result {
            Ok(doc) => {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                let expiry = doc.get_i64("expiry").unwrap_or_default();
                let verifier_data: Option<VerifierData> = doc
                    .get_array("verifier_info")
                    .ok()
                    .and_then(|array| array.get(0))
                    .and_then(|first_entry| first_entry.as_document())
                    .map(|vi| {
                        let verifier = vi.get_str("verifier").unwrap_or_default().to_string();
                        let field = vi.get_str("field").unwrap_or_default().to_string();
                        VerifierData { verifier, field }
                    });

                println!("verifier_data: {:?}", verifier_data);

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
            Err(_) => get_error("Error while fetching from database".to_string()),
        }
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
