use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::collections::HashMap;
use std::env;
use std::fs;

macro_rules! pub_struct {
    ($($derive:path),*; $name:ident {$($field:ident: $t:ty),* $(,)?}) => {
        #[derive($($derive),*)]
        pub struct $name {
            $(pub $field: $t),*
        }
    }
}

pub_struct!(Clone, Deserialize; Server { port: u16 });

pub_struct!(Clone, Deserialize; Databases {
    starknetid: Database,
    sales: Database,
});

pub_struct!(Clone, Deserialize; Database {
    name: String,
    connection_string: String,
});

pub_struct!(Clone, Deserialize; Contracts {
    starknetid: FieldElement,
    naming: FieldElement,
    verifiers: Vec<FieldElement>,
    old_verifier: FieldElement,
    pop_verifier: FieldElement,
    pp_verifier: FieldElement,
});

pub_struct!(Clone, Deserialize; Starkscan {
    api_url: String,
    api_key: String,
});

pub_struct!(Clone, Deserialize; Solana {
    rpc_url: String,
    private_key: FieldElement,
});

pub_struct!(Clone, Deserialize; TokenSupport {
    avnu_api: String,
    whitelisted_tokens: Vec<FieldElement>,
    max_validity: i64,
    private_key: FieldElement
});

#[derive(Deserialize)]
struct RawConfig {
    server: Server,
    databases: Databases,
    contracts: Contracts,
    starkscan: Starkscan,
    custom_resolvers: HashMap<String, Vec<String>>,
    solana: Solana,
    token_support: TokenSupport,
}

pub_struct!(Clone, Deserialize; Config {
    server: Server,
    databases: Databases,
    contracts: Contracts,
    starkscan: Starkscan,
    custom_resolvers: HashMap<String, Vec<String>>,
    reversed_resolvers: HashMap<String, String>,
    solana: Solana,
    token_support: TokenSupport,
});

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        let mut reversed_resolvers = HashMap::new();
        for (key, values) in &raw.custom_resolvers {
            for value in values {
                reversed_resolvers.insert(value.clone(), key.clone());
            }
        }
        Config {
            server: raw.server,
            databases: raw.databases,
            contracts: raw.contracts,
            starkscan: raw.starkscan,
            custom_resolvers: raw.custom_resolvers,
            reversed_resolvers,
            solana: raw.solana,
            token_support: raw.token_support,
        }
    }
}

pub fn load() -> Config {
    let args: Vec<String> = env::args().collect();
    let config_path = if args.len() <= 1 {
        "config.toml"
    } else {
        args.get(1).unwrap()
    };
    let file_contents = fs::read_to_string(config_path);
    if file_contents.is_err() {
        panic!("error: unable to read file with path \"{}\"", config_path);
    }

    let raw_config: RawConfig = match toml::from_str(&file_contents.unwrap()) {
        Ok(loaded) => loaded,
        Err(err) => panic!("error: unable to deserialize config. {}", err),
    };

    raw_config.into()
}
