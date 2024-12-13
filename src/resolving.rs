use std::sync::Arc;
use crate::logger::Logger; 
use crate::config;

use futures::StreamExt;
use mongodb::{
    bson::{doc, Bson, Document},
    options::AggregateOptions,
    Collection,
};

use crate::{config::OffchainResolver, models::AppState, utils::clean_string};

pub async fn get_custom_resolver(domains: &Collection<Document>, domain: &str) -> Option<String> {

     let conf = config::load();
    let logger = Logger::new(&conf.watchtower);
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
            logger.severe(format!("err on custom_resolver: {}", err));
        }
    }

    // If no custom resolver found
    None
}

pub async fn update_offchain_resolvers(state: &Arc<AppState>) {
    let offchain_resolvers = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("offchain_resolvers");

        let conf = config::load();
        let logger = Logger::new(&conf.watchtower);

    let pipeline = [
        doc! {
            "$match": doc! {
                "_cursor.to": Bson::Null,
                "active": true
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
                            "_cursor.to": Bson::Null,
                        }
                    }
                ],
                "as": "domainData"
            }
        },
        doc! {
            "$project": doc! {
                "_id": 0,
                "resolver_contract": "$resolver_contract",
                "uri": "$uri",
                "active": "$active",
                "domains": "$domainData.domain",
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
                    let domains = doc.get_array("domains");
                    let domains = match domains {
                        Ok(domains) => domains,
                        Err(err) => {
                            logger.warning(format!("Error while getting array of domains: {}", err));
                            continue;
                        }
                    };
                    if domains.is_empty() {
                        continue;
                    }
                    for domain in domains {
                        let domain = match domain {
                            Bson::String(domain) => domain,
                            _ => {
                                logger.warning(format!("Error while getting domain: {:?}", domain));
                                continue;
                            }
                        };
                        // values in config file override onchain events
                        match (&state.conf).offchain_resolvers.get(domain) {
                            Some(_) => continue,
                            None => {
                                let mut resolver_map =
                                    state.dynamic_offchain_resolvers.lock().unwrap();
                                match resolver_map.get(domain) {
                                    Some(existing_resolvers) => {
                                        // there is already a resolver for this domain
                                        let new_uri =
                                            clean_string(doc.get_str("uri").unwrap_or_default());
                                        // we check the uri is not already in the list
                                        if !existing_resolvers.uri.contains(&new_uri) {
                                            if let Some(existing_resolver) =
                                                resolver_map.get_mut(domain)
                                            {
                                                existing_resolver.uri.push(new_uri);
                                            }
                                        }
                                    }
                                    None => {
                                        // there is no resolver for this domain yet
                                        let resolver = OffchainResolver {
                                            resolver_address: doc
                                                .get_str("resolver_contract")
                                                .unwrap_or_default()
                                                .to_owned(),
                                            uri: vec![clean_string(
                                                doc.get_str("uri").unwrap_or_default(),
                                            )],
                                        };
                                        resolver_map.insert(domain.to_owned(), resolver);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(err) => {
            logger.severe(format!("Error while building offchain_resolver hashmap from collection offchain_resolvers: {}", err));
        }
    }
}

pub fn get_offchain_resolver(
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
