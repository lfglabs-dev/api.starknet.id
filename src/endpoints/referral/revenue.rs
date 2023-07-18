use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::StreamExt;
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct Data {
    revenues: Vec<i64>,
}

#[derive(Deserialize)]
pub struct IdQuery {
    sponsor: String,
    since_date: i64,
    spacing: i64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let referral_revenues = state
        .db
        .collection::<mongodb::bson::Document>("referral_revenues");

    let mut output = Data { revenues: vec![] };
    let mut i = 0;
    loop {
        let start_time = DateTime::<Utc>::from_utc(
            NaiveDateTime::from_timestamp_opt(query.since_date + i * query.spacing, 0).unwrap(),
            Utc,
        );
        let end_time = DateTime::<Utc>::from_utc(
            NaiveDateTime::from_timestamp_opt(query.since_date + (i + 1) * query.spacing, 0)
                .unwrap(),
            Utc,
        );

        let documents = referral_revenues
            .find(
                doc! {
                    "sponsor_addr": &query.sponsor,
                    "amount": { "$gt": 0 },
                    "timestamp": {
                        "$gt": BsonDateTime::from_millis(start_time.timestamp() * 1000),
                        "$lt": BsonDateTime::from_millis(end_time.timestamp() * 1000)
                    },
                    "_chain.valid_to": Bson::Null,
                },
                None,
            )
            .await;

        let mut sum = 0;

        match documents {
            Ok(mut cursor) => {
                while let Some(doc) = cursor.next().await {
                    if let Ok(doc) = doc {
                        let amount = doc.get_i64("amount").unwrap_or_default().to_owned();
                        sum += amount;
                    }
                }
            }
            Err(e) => return get_error(format!("Error while fetching from database: {:?}", e)),
        }

        output.revenues.push(sum);

        if end_time.date_naive() >= Utc::now().date_naive() {
            break;
        }

        i += 1;
    }

    (StatusCode::OK, headers, Json(output)).into_response()
}
