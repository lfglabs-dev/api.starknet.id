use crate::{
    models::AppState,
    utils::get_error,
};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::bson::{doc, Document};
use std::sync::Arc;
use serde::Serialize;

#[derive(Serialize)]
struct IdDetails {
    addr: String,
    domain: String,
}

#[derive(Serialize)]
pub struct ExpiringDomains {
    ids: Vec<IdDetails>,
}

#[route(get, "/get_expiring_domains", crate::endpoints::get_expiring_domains)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let collection = state.starknetid_db.collection::<Document>("domains");

    let current_time = chrono::Utc::now().timestamp();
    let one_week_later = current_time + 604800; // Add one week in seconds

    // Define the aggregation pipeline
    let pipeline = vec![
        doc! {
            "$match": {
                "expiry": { 
                    "$lt": one_week_later,
                    "$gt": current_time
             },
                "_cursor.to": null
            }
        },
        doc! {
            "$project": {
                "domain": 1,
                "legacy_address": 1,
            }
        },
    ];

    let mut ids: Vec<IdDetails> = Vec::new();

    match collection.aggregate(pipeline, None).await {
        Ok(mut cursor) => {
            while let Some(doc_result) = cursor.next().await {
                match doc_result {
                    Ok(doc) => {
                        if let (Ok(domain), Ok(address)) = (doc.get_str("domain"), doc.get_str("legacy_address")) {
                            ids.push(IdDetails { addr: address.to_string(), domain: domain.to_string() });
                        }
                    }
                    Err(_) => {
                        get_error("Error while parsing document".to_string());
                    }
                }
            }
            (StatusCode::OK, Json(ExpiringDomains { ids })).into_response()
        }
        Err(_) => get_error("Failed to retrive data from database".to_string()),
    }
}