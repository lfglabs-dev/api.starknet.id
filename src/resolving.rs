use mongodb::{
    bson::{doc, Document},
    Collection,
};

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
