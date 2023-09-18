use crate::{models::AppState, utils::get_error};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use futures::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct CountClubDomainsData {
    club: String,
    count: i32,
}

pub async fn handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    let domain_collection = state.db.collection::<mongodb::bson::Document>("domains");
    let current = (chrono::Utc::now().timestamp_millis() / 100_000) * 100;

    let pipeline = vec![
        doc! {
            "$match": {
                "_cursor.to": null,
                // "expiry": {
                //     "$lte": current,
                // }
            }
        },
        doc! {
        "$project": {
            "domain": "$domain",
            "club": {
                "$cond": [
                    { "$regexMatch": { "input": "$domain", "regex": r"^.\.stark$" }},
                    "single_letter",
                    { "$cond": [
                        { "$regexMatch": { "input": "$domain", "regex": r"^\d{2}\.stark$" }},
                        "99",
                        { "$cond": [
                            { "$regexMatch": { "input": "$domain", "regex": r"^.{2}\.stark$" }},
                            "two_letters",
                            { "$cond": [
                                { "$regexMatch": { "input": "$domain", "regex": r"^\d{3}\.stark$" }},
                                "999",
                                { "$cond": [
                                    { "$regexMatch": { "input": "$domain", "regex": r"^.{3}\.stark$" }},
                                    "three_letters",
                                    { "$cond": [
                                        { "$regexMatch": { "input": "$domain", "regex": r"^\d{4}\.stark" }},
                                        "10k",
                                        "none"
                                    ]}
                                ]}
                            ]}
                        ]}
                    ]}
                ]}
            }
        },
        doc! {
            "$match": {
                "club": { "$ne": "none" }
            }
        },
    ];

    let options = AggregateOptions::builder().build();
    let aggregate_cursor = domain_collection.aggregate(pipeline, options).await;

    match aggregate_cursor {
        Ok(mut cursor) => {
            let mut output = Vec::new();
            while let Some(result) = cursor.next().await {
                match result {
                    Ok(doc) => {
                        if let Ok(domain) = doc.get_str("domain") {
                            if let Ok(club) = doc.get_str("club") {
                                output.push(doc! { "domain": domain, "club": club });
                            }
                        }
                    }
                    Err(e) => println!("Error: {}", e),
                }
            }
            if output.is_empty() {
                return get_error("No documents found".to_string());
            }
            (StatusCode::OK, headers, Json(output)).into_response()
        }
        Err(e) => get_error(format!("Error while fetching from database: {:?}", e)),
    }
}
