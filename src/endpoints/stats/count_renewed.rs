use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use futures::StreamExt;
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct CountRenewedData {
    from: i64,
    count: i32,
}

#[derive(Deserialize)]
pub struct CountRenewedQuery {
    begin: i64,
    end: i64,
    segments: i64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CountRenewedQuery>,
) -> impl IntoResponse {
    let begin_time = query.begin;
    let end_time = query.end;
    let delta_time = ((end_time as f64 - begin_time as f64) / query.segments as f64).round() as i64;

    if delta_time > 3600 {
        let mut headers = HeaderMap::new();
        headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

        let domain_collection = state
            .starknetid_db
            .collection::<mongodb::bson::Document>("renewals");

        let pipeline = vec![
            doc! {
                "$match": {
                    "$or": [
                        { "_cursor.to": { "$exists": false } },
                        { "_cursor.to": Bson::Null },
                    ],
                    "timestamp": {
                        "$gte": begin_time,
                        "$lte": end_time
                    }
                }
            },
            doc! {
                "$group": {
                    "_id": {
                        "$floor": {
                            "$sum": [
                                {
                                    "$subtract": [
                                        {
                                            "$subtract": ["$timestamp", begin_time]
                                        },
                                        {
                                            "$mod": [
                                                {
                                                    "$subtract": ["$timestamp", begin_time]
                                                },
                                                delta_time
                                            ]
                                        }
                                    ]
                                },
                                begin_time
                            ]
                        }
                    },
                    "count": {
                        "$sum": 1
                    }
                }
            },
            doc! {
                "$project": {
                    "_id": 0,
                    "from": "$_id",
                    "count": "$count"
                }
            },
        ];

        let cursor = domain_collection.aggregate(pipeline, None).await.unwrap();
        let result: Vec<CountRenewedData> = cursor
            .map(|doc| {
                let doc = doc.unwrap();
                let from: i64 = doc.get_i64("from").unwrap();
                let count = doc.get_i32("count").unwrap();

                CountRenewedData { from, count }
            })
            .collect::<Vec<_>>()
            .await;

        (StatusCode::OK, headers, Json(result)).into_response()
    } else {
        get_error("delta must be greater than 3600 seconds".to_string())
    }
}
