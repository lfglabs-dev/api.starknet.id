use crate::models::AppState;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_auto_routes::route;
use futures::TryStreamExt;
use mongodb::bson::{self, doc, Bson};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct CountClubDomainsData {
    club: String,
    count: i32,
}

#[derive(Deserialize)]
pub struct CountClubDomainsQuery {
    since: i64,
}

#[route(get, "/stats/count_club_domains", crate::endpoints::stats::count_club_domains)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CountClubDomainsQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    let domain_collection = state.starknetid_db.collection::<mongodb::bson::Document>("domains");
    let subdomain_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("custom_resolutions");

    let subdomain_output = subdomain_collection
        .aggregate(
            vec![
                doc! {
                    "$match": {
                        "$or": [
                            { "_cursor.to": { "$exists": false } },
                            { "_cursor.to": Bson::Null },
                        ],
                        // todo: uncomment when there is a creation_date in the collection custom_resolutions
                        // "creation_date": {
                        //     "$gte": query.since,
                        // }
                    }
                },
                doc! {
                    "$group": {
                        "_id": {
                            "$cond": [
                                {
                                    "$eq": ["$resolver", "0x0660b2cd3c93528d4edf790610404414ba3f03e0d45c814d686d628583cb34de"]
                                },
                                "braavos",
                                {
                                    "$cond": [
                                        {
                                            "$eq": ["$resolver", "0x4942ebdc9fc996a42adb4a825e9070737fe68cef32a64a616ba5528d457812e"]
                                        },
                                        "xplorer",
                                        "none"
                                    ]
                                }
                            ]
                        },
                        "count": {
                            "$sum": 1
                        }
                    }
                },
                doc! {
                    "$project": {
                        "_id": 0,
                        "club": "$_id",
                        "count": "$count"
                    }
                },
            ],
            None,
        )
        .await
        .unwrap()
        .try_collect::<Vec<bson::Document>>()
        .await
        .unwrap();

    let db_output = domain_collection.aggregate(vec![
            doc! {
                "$match": {
                    "creation_date": {
                        "$gte": query.since,
                    },
                    "$or": [
                        { "_cursor.to": { "$exists": false } },
                        { "_cursor.to": Bson::Null },
                    ],
                }
            },
            doc! {
                "$group": {
                    "_id": {
                        "$cond": [
                            {"$regexMatch": {"input": "$domain", "regex": r"^.\.stark$"}},
                            "single_letter",
                            { "$cond": [
                                {"$regexMatch": {"input": "$domain", "regex": r"^\d{2}\.stark$"}},
                                "99",
                                { "$cond": [
                                    {"$regexMatch": {"input": "$domain", "regex": r"^.{2}\.stark$"}},
                                    "two_letters",
                                    {"$cond": [
                                        { "$regexMatch": {"input": "$domain", "regex": r"^\d{3}\.stark$"}},
                                        "999",
                                        {"$cond": [
                                            {"$regexMatch": { "input": "$domain", "regex": r"^.{3}\.stark$"}},
                                            "three_letters",
                                            {"$cond": [
                                                { "$regexMatch": { "input": "$domain", "regex": r"^\d{4}\.stark$" }},
                                                "10k",
                                                {"$cond": [
                                                    {"$regexMatch": {"input": "$domain", "regex": r"^.{4}\.stark$"}},
                                                    "four_letters",
                                                    {"$cond": [
                                                        { "$regexMatch": {"input": "$domain", "regex": r"^.*\.vip\.stark$"}},
                                                        "og",
                                                        {"$cond": [
                                                            {"$regexMatch": {"input": "$domain", "regex": r"^.*\.everai\.stark$"}},
                                                            "everai",
                                                            { "$cond": [
                                                                {"$regexMatch": { "input": "$domain","regex": r"^.*\.onsheet\.stark$" }},
                                                                "onsheet",
                                                                "none",
                                                            ]},
                                                        ]},
                                                    ]},
                                                ]},
                                            ]}
                                        ]},
                                    ]},
                                ]},
                            ]},
                        ], 
                    },
                    "count": {
                        "$sum": 1
                    }
                }
            },
            doc! {
                "$project": {
                    "_id": 0,
                    "club": "$_id",
                    "count": "$count"
                }
            }
        ], None).await.unwrap().try_collect::<Vec<bson::Document>>().await.unwrap();

        let mut count_99 = 0;
        let mut count_999 = 0;
        let mut count_10k = 0;
    
        let mut output: Vec<HashMap<String, i32>> = Vec::new();
        let mut output_map: HashMap<String, i32> = HashMap::new();

        for doc in &db_output {
            if let Ok(club) = doc.get_str("club") {
                match club {
                    "99" => count_99 = doc.get_i32("count").unwrap_or_default(),
                    "999" => count_999 = doc.get_i32("count").unwrap_or_default(),
                    "10k" => count_10k = doc.get_i32("count").unwrap_or_default(),
                    _ => (),
                }
            }
        }

        for doc in db_output {
            if let Ok(club) = doc.get_str("club") {
                match club {
                    "two_letters" => {
                        output_map.insert(club.to_string(), doc.get_i32("count").unwrap_or_default() + count_99);
                    }
                    "three_letters" => {
                        output_map.insert(club.to_string(), doc.get_i32("count").unwrap_or_default() + count_999);
                    }
                    "four_letters" => {
                        output_map.insert(club.to_string(), doc.get_i32("count").unwrap_or_default() + count_10k);
                    }
                    _ => {
                        output_map.insert(club.to_string(), doc.get_i32("count").unwrap_or_default());
                    }
                }
            }
            output.push(output_map.clone());
            output_map.clear();
        }

        for doc in subdomain_output {
            output_map.insert(doc.get_str("club").unwrap_or_default().to_string(), doc.get_i32("count").unwrap_or_default());
            output_map.insert(doc.get_str("club").unwrap_or_default().to_string(), doc.get_i32("count").unwrap_or_default());
            output.push(output_map.clone());
            output_map.clear();
        }

        (StatusCode::OK, headers, Json(output)).into_response()
}
