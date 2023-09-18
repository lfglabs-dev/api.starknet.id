use std::sync::Arc;

use crate::models::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use futures::StreamExt;
use mongodb::{bson::doc, bson::Document};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct EmailQuery {
    email: String,
}

#[derive(Serialize)]
pub struct SimpleResponse {
    result: &'static str,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<EmailQuery>,
) -> impl IntoResponse {
    // Get the metadata collection
    let metadata_collection = state.sales_db.collection::<Document>("metadata");

    // Define the aggregation pipeline with a lookup stage
    let pipeline = vec![
        doc! {
            "$match": {
                "email": &query.email
            }
        },
        doc! {
            "$lookup": {
                "from": "processed",
                "localField": "meta_hash",
                "foreignField": "meta_hash",
                "as": "matched_docs"
            }
        },
        doc! {
            "$match": {
                "matched_docs": {
                    "$ne": []
                }
            }
        },
    ];

    // Execute the aggregation pipeline
    let mut cursor = metadata_collection.aggregate(pipeline, None).await.unwrap();

    // Check if any results are returned
    if cursor.next().await.is_some() {
        let response = SimpleResponse {
            result: "0x00000000000000000000000000000001",
        };
        return (StatusCode::OK, Json(response)).into_response();
    } else {
        let response = SimpleResponse {
            result: "0x00000000000000000000000000000000",
        };
        return (StatusCode::OK, Json(response)).into_response();
    }
}
