use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_auto_routes::route;
use chrono::Utc;
use mongodb::{
    bson::{doc, DateTime as BsonDateTime},
    options::UpdateOptions,
};
use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AddClickQuery {
    sponsor_addr: FieldElement,
}

#[route(post, "/referral/add_click", crate::endpoints::referral::add_click)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<AddClickQuery>,
) -> impl IntoResponse {
    let sponsor_usage = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("sponsor_usage");
    let update_options = UpdateOptions::builder().upsert(true).build();

    let today = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let today_bson = BsonDateTime::from_millis(today.and_utc().timestamp() * 1000);

    let result = sponsor_usage
        .update_one(
            doc! {
                "sponsor_addr": query.sponsor_addr.to_string(),
                "day": today_bson,
            },
            doc! {
                "$inc": { "clicks": 1 },
                "$setOnInsert": { "day": today_bson },
            },
            update_options,
        )
        .await;

    match result {
        Ok(_) => (
            StatusCode::OK,
            Json("Sponsor usage updated successfully".to_string()),
        )
            .into_response(),
        Err(_) => get_error("Error while updating database".to_string()),
    }
}
