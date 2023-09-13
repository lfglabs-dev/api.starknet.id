use mongodb::{
    bson::{doc, Bson, Document},
    Collection,
};

pub async fn has_no_custom_resolver(domains: &Collection<Document>, domain: &str) -> bool {
    // Split the domain into parts
    let domain_parts: Vec<&str> = domain.split('.').collect();
    println!("domain: {}", domain);
    if domain_parts.len() <= 2 {
        // The domain itself is a root domain, so we can directly return true
        return true;
    }

    // Create an empty list of conditions
    let mut conditions = vec![];

    // Create the conditions for all parent domains (excluding the root domain)
    for i in 1..(domain_parts.len() - 1) {
        // Starting from 1 to exclude the sub-domain itself
        let domain_to_check = domain_parts[i..].join(".");
        conditions.push(doc! {
            "domain": domain_to_check,
            "$or": [
                { "resolver": Bson::Null },
                { "resolver": "0x0000000000000000000000000000000000000000000000000000000000000000" }
            ]
        });
    }

    // Combine conditions with $or
    let query = doc! { "$or": conditions };

    // Search the database
    match domains.find_one(query, None).await {
        Ok(doc) => {
            // Return false if a domain with a custom resolver was found, true otherwise
            doc.is_none()
        }
        Err(_) => {
            // Logging the error or handling it in some way might be a good idea here
            false
        }
    }
}
