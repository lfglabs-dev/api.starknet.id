use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct Data {
    counts: Vec<i64>,
}

#[derive(Deserialize)]
pub struct IdQuery {
    sponsor: String,
    since_day: i64,
    spacing: i64,
}

#[route(get, "/referral/click_count", crate::endpoints::referral::click_count)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let sponsor_usage = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("sponsor_usage");

    let mut output = Data { counts: vec![] };
    let mut i = 0;
    loop {
        let start_time = DateTime::from_timestamp(query.since_day + i * query.spacing, 0).unwrap();

        let end_time =
            DateTime::from_timestamp(query.since_day + (i + 1) * query.spacing, 0).unwrap();

        let documents = sponsor_usage
            .find(
                doc! {
                    "sponsor_addr": &query.sponsor,
                    "day": {
                        "$gt": BsonDateTime::from_millis(start_time.timestamp() * 1000),
                        "$lt": BsonDateTime::from_millis(end_time.timestamp() * 1000)
                    },
                    "_cursor.to": Bson::Null,
                },
                None,
            )
            .await;

        let mut count = 0;

        match documents {
            Ok(mut cursor) => {
                while let Some(doc) = cursor.next().await {
                    if let Ok(doc) = doc {
                        let clicks = doc.get_i32("clicks").unwrap_or_default().to_owned();
                        count += clicks as i64;
                    }
                }
            }
            Err(e) => {
                return get_error(format!("Error while fetching from database: {:?}", e));
            }
        }

        output.counts.push(count);

        if end_time.date_naive() >= Utc::now().date_naive() {
            break;
        }

        i += 1;
    }

    (StatusCode::OK, headers, Json(output)).into_response()
}
