use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use starknet::core::types::FieldElement;
use starknet::core::utils::cairo_short_string_to_felt;
use std::collections::HashMap;
use std::env;
use std::fs;

use crate::endpoints::crosschain::ethereum::text_records::HandlerType;
use crate::utils::to_hex;

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
    free_domains: Database,
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
    argent_multicall: FieldElement,
    free_domains: FieldElement,
});

pub_struct!(Clone, Deserialize; Paymaster {
    api_key: String,
    api_url: String,
});

pub_struct!(Clone, Deserialize; Starkscan {
    api_url: String,
    api_key: String,
});

pub_struct!(Clone, Deserialize; Solana {
    rpc_url: String,
    private_key: FieldElement,
});

pub_struct!(Clone, Debug, Deserialize; AltcoinData {
    address: FieldElement,
    min_price: u64,
    max_price: u64,
    decimals: u32,
    max_quote_validity: i64,
    auto_renew_contract: Option<FieldElement>,
});

#[derive(Debug, Deserialize)]
struct TempAltcoins {
    avnu_api: String,
    private_key: FieldElement,
    #[serde(flatten)]
    data: HashMap<String, AltcoinData>,
}

pub_struct!(Clone, Debug; Altcoins {
    avnu_api: String,
    private_key: FieldElement,
    data: HashMap<FieldElement, AltcoinData>,
});

pub_struct!(Clone, Debug, Deserialize; Variables {
    rpc_url: String,
    refresh_delay: f64,
    ipfs_gateway: String,
    discord_token: String,
    discord_api_url: String,
    twitter_api_key: String,
    twitter_api_url: String,
    github_api_url: String,
});

#[derive(Deserialize)]
struct TempOffchainResolver {
    root_domain: String,
    resolver_address: String,
    uri: Vec<String>,
}

pub_struct!(Clone, Debug, Deserialize; OffchainResolver {
    resolver_address: String,
    uri: Vec<String>,
});

pub_struct!(Clone, Debug, Deserialize; Evm {
    private_key: String,
});

#[derive(Debug, Clone)]
pub struct OffchainResolvers(HashMap<String, OffchainResolver>);

pub_struct!(Clone, Debug, Deserialize; EvmRecordVerifier {
    verifier_contracts: Vec<FieldElement>,
    field: String,
    handler: HandlerType,
});

pub_struct!(Clone, Debug, Deserialize; FreeDomains {
    priv_key: FieldElement,
});

#[derive(Deserialize)]
struct RawConfig {
    server: Server,
    databases: Databases,
    variables: Variables,
    contracts: Contracts,
    paymaster: Paymaster,
    starkscan: Starkscan,
    custom_resolvers: HashMap<String, Vec<String>>,
    solana: Solana,
    altcoins: Altcoins,
    offchain_resolvers: OffchainResolvers,
    evm: Evm,
    evm_networks: HashMap<String, u64>,
    evm_records_verifiers: HashMap<String, EvmRecordVerifier>,
    free_domains: FreeDomains,
    watchtower: Watchtower,
}

pub_struct!(Clone, Deserialize; Config {
    server: Server,
    databases: Databases,
    variables: Variables,
    contracts: Contracts,
    paymaster: Paymaster,
    starkscan: Starkscan,
    custom_resolvers: HashMap<String, Vec<String>>,
    reversed_resolvers: HashMap<String, String>,
    solana: Solana,
    altcoins: Altcoins,
    offchain_resolvers: OffchainResolvers,
    evm: Evm,
    evm_networks: HashMap<u64, FieldElement>,
    evm_records_verifiers: HashMap<String, EvmRecordVerifier>,
    subscription_to_altcoin: HashMap<FieldElement, String>,
    free_domains: FreeDomains,
    watchtower: Watchtower,
});

pub_struct!(Clone, Deserialize; Watchtower {
    enabled : bool,
    endpoint: String,
    app_id: String,
    token: String,
    types: WatchtowerTypes,
});

pub_struct!(Clone, Deserialize; WatchtowerTypes {
    info: String,
    warning: String,
    severe: String,
});

impl Altcoins {
    fn new(temp: TempAltcoins) -> Self {
        let data: HashMap<FieldElement, AltcoinData> = temp
            .data
            .into_values()
            .map(|val| {
                let altcoin_data = AltcoinData {
                    address: val.address,
                    min_price: val.min_price,
                    max_price: val.max_price,
                    decimals: val.decimals,
                    max_quote_validity: val.max_quote_validity,
                    auto_renew_contract: val.auto_renew_contract,
                };
                (val.address, altcoin_data)
            })
            .collect();

        Altcoins {
            avnu_api: temp.avnu_api,
            private_key: temp.private_key,
            data,
        }
    }
}

impl<'de> Deserialize<'de> for Altcoins {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let temp = TempAltcoins::deserialize(deserializer)?;
        Ok(Altcoins::new(temp))
    }
}

impl OffchainResolvers {
    pub fn get(&self, key: &str) -> Option<&OffchainResolver> {
        self.0.get(key)
    }
}

impl<'de> Deserialize<'de> for OffchainResolvers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OffchainResolversVisitor;

        impl<'de> Visitor<'de> for OffchainResolversVisitor {
            type Value = OffchainResolvers;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of resolver addresses to OffchainResolvers")
            }

            fn visit_map<V>(self, mut map: V) -> Result<OffchainResolvers, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut hash_map = HashMap::new();
                while let Some((_, temp_resolver)) =
                    map.next_entry::<String, TempOffchainResolver>()?
                {
                    let resolver = OffchainResolver {
                        resolver_address: temp_resolver.resolver_address,
                        uri: temp_resolver.uri,
                    };
                    hash_map.insert(temp_resolver.root_domain, resolver);
                }
                Ok(OffchainResolvers(hash_map))
            }
        }

        deserializer.deserialize_map(OffchainResolversVisitor)
    }
}

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        let mut reversed_resolvers = HashMap::new();
        for (key, values) in &raw.custom_resolvers {
            for value in values {
                reversed_resolvers.insert(value.clone(), key.clone());
            }
        }

        let mut reversed_evm_networks = HashMap::new();
        for (key, value) in &raw.evm_networks {
            let chain_name = cairo_short_string_to_felt(&key.clone()).unwrap();
            reversed_evm_networks.insert(*value, chain_name);
        }

        let mut subscription_to_altcoin = HashMap::new();
        for (key, value) in &raw.altcoins.data {
            if let Some(auto_renew_contract) = value.auto_renew_contract {
                subscription_to_altcoin.insert(auto_renew_contract, to_hex(&key.clone()));
            }
        }

        Config {
            server: raw.server,
            databases: raw.databases,
            variables: raw.variables,
            contracts: raw.contracts,
            paymaster: raw.paymaster,
            starkscan: raw.starkscan,
            custom_resolvers: raw.custom_resolvers,
            reversed_resolvers,
            solana: raw.solana,
            altcoins: raw.altcoins,
            offchain_resolvers: raw.offchain_resolvers,
            evm: raw.evm,
            evm_networks: reversed_evm_networks,
            evm_records_verifiers: raw.evm_records_verifiers,
            subscription_to_altcoin,
            free_domains: raw.free_domains,
            watchtower: raw.watchtower,
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
