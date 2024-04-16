use std::sync::Arc;

use futures::StreamExt;
use mongodb::{
    bson::{doc, Bson, Document},
    options::AggregateOptions,
    Collection,
};

use crate::{config::OffchainResolver, models::AppState, utils::clean_string};

pub async fn get_custom_resolver(domains: &Collection<Document>, domain: &str) -> Option<String> {
    // Split the domain into parts
    let domain_parts: Vec<&str> = domain.split('.').collect();
    if domain_parts.len() <= 2 {
        // The domain itself is a root domain, so no custom resolver can exist for it
        return None;
    }

    // Using the $or operator to match any of the conditions
    let filter = doc! {
        "$or": (1..domain_parts.len() - 1)
        .rev()
        .map(|i| domain_parts[i..].join("."))
        .map(|domain_to_check| {
            doc! {
                "domain": domain_to_check,
                "_cursor.to" : null,
            }
        })
        .collect::<Vec<_>>()
    };

    // Instead of looping through conditions, just query once using the filter
    match domains.find_one(filter, None).await {
        Ok(doc) => {
            if let Some(document) = doc {
                // If the resolver field exists, is not null, and is not 0x000... then return it
                if let Some(resolver) = document.get_str("resolver").ok() {
                    if resolver
                        != "0x0000000000000000000000000000000000000000000000000000000000000000"
                        && !resolver.is_empty()
                    {
                        return Some(resolver.to_string());
                    }
                }
            }
        }
        Err(err) => {
            println!("err on custom_resolver: {}", err);
        }
    }

    // If no custom resolver found
    None
}

pub async fn update_offchain_resolvers(state: &Arc<AppState>) {
    let offchain_resolvers = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("offchain_resolvers");

    let pipeline = [
        doc! {
            "$match": doc! {
                "_cursor.to": Bson::Null
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "domains",
                "let": doc! {
                    "local_resolver_contract": "$resolver_contract"
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "$expr": doc! {
                                "$eq": [
                                    "$resolver",
                                    "$$local_resolver_contract"
                                ]
                            },
                            "_cursor.to": Bson::Null
                        }
                    }
                ],
                "as": "domainData"
            }
        },
        doc! {
            "$unwind": doc! {
                "path": "$domainData",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$project": doc! {
                "_id": 0,
                "resolver_contract": "$resolver_contract",
                "uri": "$uri",
                "domain": "$domainData.domain",
            }
        },
    ];
    let aggregate_options = AggregateOptions::default();
    let cursor = offchain_resolvers
        .aggregate(pipeline, aggregate_options)
        .await;
    match cursor {
        Ok(mut cursor) => {
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let domain = doc.get_str("domain").unwrap_or_default();
                    if domain.is_empty() {
                        continue;
                    }
                    // values in config file override onchain events
                    match (&state.conf).offchain_resolvers.get(domain) {
                        Some(_) => continue,
                        None => {
                            let resolver = OffchainResolver {
                                resolver_address: doc
                                    .get_str("resolver_contract")
                                    .unwrap_or_default()
                                    .to_owned(),
                                uri: vec![clean_string(doc.get_str("uri").unwrap_or_default())],
                            };
                            state
                                .dynamic_offchain_resolvers
                                .lock()
                                .unwrap()
                                .insert(domain.to_owned(), resolver);
                        }
                    }
                }
            }
        }
        Err(err) => {
            println!("Error while building offchain_resolver hashmap from collection offchain_resolvers: {}", err);
        }
    }
}

pub fn is_offchain_resolver(
    prefix: String,
    root_domain: String,
    state: &Arc<AppState>,
) -> Option<OffchainResolver> {
    if prefix.is_empty() {
        return None;
    }
    state
        .conf
        .offchain_resolvers
        .get(&root_domain)
        .cloned()
        .or_else(|| {
            state
                .dynamic_offchain_resolvers
                .lock()
                .unwrap()
                .get(&root_domain)
                .cloned()
        })
}
