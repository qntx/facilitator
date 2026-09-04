//! Config schema tests for facilitator 2.0.

#![allow(
    unused_crate_dependencies,
    reason = "integration tests link the package graph"
)]
#![allow(
    clippy::tests_outside_test_module,
    reason = "integration test binaries put #[test] fns at file scope"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "idiomatic test-code patterns"
)]

use std::path::PathBuf;

use facilitator::{Network, parse_config_toml};

fn repo_file(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn evm_doc(extra_network: &str) -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"

[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
signers = ["evm_hot"]
schemes = ["exact"]
receipt_timeout_secs = 20
{extra_network}
"#
    )
}

#[test]
fn example_toml_parses_evm_exact_and_upto() {
    let cfg = parse_config_toml(&repo_file("config.example.toml")).expect("example parses");
    assert_eq!(cfg.networks.len(), 2, "base sepolia + base");
    let ids: Vec<String> = cfg
        .networks
        .iter()
        .map(|network| network.chain_id().to_string())
        .collect();
    assert_eq!(
        ids,
        ["eip155:84532".to_owned(), "eip155:8453".to_owned()],
        "TOML appearance order"
    );
    for network in &cfg.networks {
        assert_eq!(network.chain_id().namespace(), "eip155", "evm only");
        assert_eq!(
            network.schemes(),
            &["exact".to_owned(), "upto".to_owned()],
            "exact+upto"
        );
    }
}

#[test]
fn example_toml_has_no_live_solana_tables() {
    let raw = repo_file("config.example.toml");
    assert!(
        !raw.contains("[network.\"solana:"),
        "Solana live tables belong in config.example.full.toml"
    );
}

#[test]
fn full_example_parses_as_documentation() {
    let cfg = parse_config_toml(&repo_file("config.example.full.toml")).expect("full parses");
    let has_solana = cfg
        .networks
        .iter()
        .any(|net| net.chain_id().namespace() == "solana");
    assert!(has_solana, "full example includes SVM exact+upto");
    let svm = cfg.networks.iter().find_map(|net| {
        if let Network::Svm(svm) = net {
            Some(svm)
        } else {
            None
        }
    });
    let svm = svm.expect("solana network");
    assert_eq!(
        svm.schemes,
        ["exact".to_owned(), "upto".to_owned()],
        "SVM exact+upto"
    );
    assert_eq!(
        cfg.scheme.svm.upto.max_channel_lifetime_secs,
        Some(3_600),
        "full example sets SVM upto lifetime"
    );
    assert_eq!(svm.max_compute_unit_limit, 200_000, "network CU limit");
    assert_eq!(svm.max_compute_unit_price, None, "SDK default CU price");
    let evm_schemes = cfg.networks.iter().filter_map(|net| {
        if let Network::Evm(evm) = net {
            Some(evm.schemes.as_slice())
        } else {
            None
        }
    });
    for schemes in evm_schemes {
        assert_eq!(
            schemes,
            ["exact".to_owned(), "upto".to_owned()].as_slice(),
            "full example EVM is exact+upto"
        );
    }
}

#[test]
fn rejects_schemes_array() {
    let err = parse_config_toml("[[schemes]]\nid = \"eip155-exact\"\n").unwrap_err();
    assert!(err.to_string().contains("delete [[schemes]]"), "got {err}");
}

#[test]
fn rejects_old_signers_table() {
    let raw = r#"
[signers]
evm = ["$EVM_SIGNER_PRIVATE_KEY"]
[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
signers = ["evm_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(err.to_string().contains("delete [signers]"), "got {err}");
}

#[test]
fn rejects_settlement_mode() {
    let raw = r#"
settlement_mode = "sequential"
[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"
[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
signers = ["evm_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(err.to_string().contains("settlement_mode"), "got {err}");
}

#[test]
fn rejects_literal_hex_key() {
    let raw = r#"
[signer.evm_hot]
source = "env"
env = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
signers = ["evm_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(err.to_string().contains("private-key literal"), "got {err}");
}

#[test]
fn unknown_scheme_name_fails() {
    let raw = evm_doc("").replace("schemes = [\"exact\"]", "schemes = [\"not-a-scheme\"]");
    let err = parse_config_toml(&raw).unwrap_err();
    assert!(
        err.to_string().contains("unknown scheme 'not-a-scheme'"),
        "got {err}"
    );
}

#[test]
fn upto_scheme_name_is_known() {
    let raw = evm_doc("").replace("schemes = [\"exact\"]", "schemes = [\"exact\", \"upto\"]");
    parse_config_toml(&raw).expect("upto is a known EVM scheme name");
}

#[test]
fn omitted_schemes_fails() {
    let raw = r#"
[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"
[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
signers = ["evm_hot"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("schemes"),
        "omitted schemes must fail, got {err}"
    );
}

#[test]
fn empty_networks_fails() {
    let err = parse_config_toml("[http]\nlisten = \"127.0.0.1:8080\"\n").unwrap_err();
    assert!(err.to_string().contains("empty [network]"), "got {err}");
}

#[test]
fn unknown_namespace_fails() {
    let raw = r#"
[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"
[network."foo:bar"]
rpc = "https://example.com"
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("unknown CAIP-2 namespace"),
        "got {err}"
    );
}

#[cfg(not(feature = "near"))]
#[test]
fn compiled_out_near_fails() {
    let raw = r#"
[signer.near_relayer]
source = "env"
env = "NEAR_KEY"
[network."near:testnet"]
relayers = [{ account_id = "relayer.testnet", signer = "near_relayer" }]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'near'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features near"), "got {err}");
}

#[cfg(feature = "near")]
#[test]
fn near_optional_rpc_omit_ok() {
    let raw = r#"
[signer.near_relayer]
source = "env"
env = "NEAR_KEY"
[network."near:testnet"]
relayers = [{ account_id = "relayer.testnet", signer = "near_relayer" }]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit rpc");
    let near = cfg.networks.iter().find_map(|net| {
        if let Network::Near(near) = net {
            Some(near)
        } else {
            None
        }
    });
    let near = near.expect("near network");
    assert!(near.rpc.is_none(), "omit = SDK default");
    assert_eq!(
        near.relayer_signer_names,
        ["near_relayer".to_owned()],
        "relayer signer"
    );
}

#[cfg(feature = "near")]
#[test]
fn near_rpc_and_rpc_env_are_exclusive() {
    let raw = r#"
[signer.near_relayer]
source = "env"
env = "NEAR_KEY"
[network."near:testnet"]
rpc = "http://127.0.0.1:1"
rpc_env = "NEAR_RPC"
relayers = [{ account_id = "relayer.testnet", signer = "near_relayer" }]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("at most one of `rpc` or `rpc_env`"),
        "got {err}"
    );
}

#[cfg(not(feature = "xrpl"))]
#[test]
fn compiled_out_xrpl_fails() {
    let raw = r#"
[network."xrpl:1"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'xrpl'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features xrpl"), "got {err}");
}

#[cfg(feature = "xrpl")]
#[test]
fn xrpl_optional_rpc_omit_ok() {
    let raw = r#"
[network."xrpl:1"]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit rpc");
    let xrpl = cfg.networks.iter().find_map(|net| {
        if let Network::Xrpl(xrpl) = net {
            Some(xrpl)
        } else {
            None
        }
    });
    let xrpl = xrpl.expect("xrpl network");
    assert!(xrpl.rpc.is_none(), "omit = SDK default");
    assert!(
        cfg.networks
            .iter()
            .any(|net| net.chain_id().namespace() == "xrpl" && net.signer_names().is_empty()),
        "no hot wallet"
    );
}

#[cfg(feature = "xrpl")]
#[test]
fn xrpl_rpc_and_rpc_env_are_exclusive() {
    let raw = r#"
[network."xrpl:1"]
rpc = "http://127.0.0.1:1"
rpc_env = "XRPL_RPC"
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("at most one of `rpc` or `rpc_env`"),
        "got {err}"
    );
}

#[cfg(feature = "xrpl")]
#[test]
fn xrpl_rejects_signers() {
    let raw = r#"
[signer.xrpl_hot]
source = "env"
env = "XRPL_KEY"
[network."xrpl:1"]
signers = ["xrpl_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("XRPL has no hot wallet"),
        "got {err}"
    );
    assert!(err.to_string().contains("`signers`"), "got {err}");
}

#[cfg(feature = "xrpl")]
#[test]
fn xrpl_rejects_fee_payer() {
    let raw = r#"
[signer.xrpl_hot]
source = "env"
env = "XRPL_KEY"
[network."xrpl:1"]
fee_payer = "xrpl_hot"
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("XRPL has no hot wallet"),
        "got {err}"
    );
    assert!(err.to_string().contains("`fee_payer`"), "got {err}");
}

#[cfg(not(feature = "hedera"))]
#[test]
fn compiled_out_hedera_fails() {
    let raw = r#"
[signer.hedera_fee]
source = "env"
env = "HEDERA_KEY"
[network."hedera:testnet"]
fee_payers = [{ account_id = "0.0.5001", signer = "hedera_fee" }]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'hedera'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features hedera"), "got {err}");
}

#[cfg(feature = "hedera")]
#[test]
fn hedera_parses_fee_payers() {
    let raw = r#"
[signer.hedera_fee]
source = "env"
env = "HEDERA_KEY"
[network."hedera:testnet"]
fee_payers = [{ account_id = "0.0.5001", signer = "hedera_fee" }]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("hedera");
    let hedera = cfg.networks.iter().find_map(|net| {
        if let Network::Hedera(hedera) = net {
            Some(hedera)
        } else {
            None
        }
    });
    let hedera = hedera.expect("hedera network");
    assert_eq!(
        hedera.fee_payer_signer_names,
        ["hedera_fee".to_owned()],
        "fee payer"
    );
    assert_eq!(
        hedera.alias_policy,
        facilitator::HederaAliasPolicy::Reject,
        "default"
    );
    assert_eq!(hedera.node_url, None, "omit node_url");
}

#[cfg(feature = "hedera")]
#[test]
fn hedera_node_url_host_port_ok() {
    let raw = r#"
[signer.hedera_fee]
source = "env"
env = "HEDERA_KEY"
[network."hedera:testnet"]
fee_payers = [{ account_id = "0.0.5001", signer = "hedera_fee" }]
schemes = ["exact"]
node_url = "0.testnet.hedera.com:50211"
"#;
    let cfg = parse_config_toml(raw).expect("host:port");
    let hedera = cfg.networks.iter().find_map(|net| {
        if let Network::Hedera(hedera) = net {
            Some(hedera)
        } else {
            None
        }
    });
    let hedera = hedera.expect("hedera network");
    assert_eq!(
        hedera.node_url.as_deref(),
        Some("0.testnet.hedera.com:50211"),
        "gRPC host:port"
    );
}

#[cfg(feature = "hedera")]
#[test]
fn hedera_node_url_https_rejected() {
    let raw = r#"
[signer.hedera_fee]
source = "env"
env = "HEDERA_KEY"
[network."hedera:testnet"]
fee_payers = [{ account_id = "0.0.5001", signer = "hedera_fee" }]
schemes = ["exact"]
node_url = "https://testnet.hedera.com"
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("`node_url` must be host:port"),
        "got {err}"
    );
}

#[cfg(not(feature = "avm"))]
#[test]
fn compiled_out_avm_fails() {
    let raw = r#"
[signer.algo_hot]
source = "env"
env = "ALGORAND_KEY"
[network."algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe"]
signers = ["algo_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'algorand'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features avm"), "got {err}");
}

#[cfg(feature = "avm")]
#[test]
fn avm_optional_algod_omit_ok() {
    let raw = r#"
[signer.algo_hot]
source = "env"
env = "ALGORAND_KEY"
[network."algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe"]
signers = ["algo_hot"]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit algod");
    let avm = cfg.networks.iter().find_map(|net| {
        if let Network::Avm(avm) = net {
            Some(avm)
        } else {
            None
        }
    });
    let avm = avm.expect("avm network");
    assert!(avm.algod_url.is_none(), "omit algod_url");
    assert!(avm.algod_token_env.is_none(), "omit token");
}

#[cfg(not(feature = "aptos"))]
#[test]
fn compiled_out_aptos_names_feature() {
    let raw = r#"
[signer.aptos_hot]
source = "env"
env = "APTOS_KEY"
[network."aptos:1"]
fee_payers = ["aptos_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'aptos'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features aptos"), "got {err}");
}

#[cfg(feature = "aptos")]
#[test]
fn aptos_optional_rpc_omit_ok() {
    let raw = r#"
[signer.aptos_hot]
source = "env"
env = "APTOS_KEY"
[network."aptos:1"]
fee_payers = ["aptos_hot"]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit rpc");
    let aptos = cfg.networks.iter().find_map(|net| {
        if let Network::Aptos(aptos) = net {
            Some(aptos)
        } else {
            None
        }
    });
    let aptos = aptos.expect("aptos network");
    assert!(aptos.rpc.is_none(), "omit = SDK default");
    assert!(aptos.sponsor_transactions, "provider default true");
    assert_eq!(
        aptos.fee_payers,
        ["aptos_hot".to_owned()],
        "fee payer names"
    );
}

#[cfg(feature = "aptos")]
#[test]
fn aptos_rpc_and_rpc_env_are_exclusive() {
    let raw = r#"
[signer.aptos_hot]
source = "env"
env = "APTOS_KEY"
[network."aptos:1"]
rpc = "http://127.0.0.1:1"
rpc_env = "APTOS_RPC"
fee_payers = ["aptos_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("at most one of `rpc` or `rpc_env`"),
        "got {err}"
    );
}

#[cfg(not(feature = "keeta"))]
#[test]
fn compiled_out_keeta_names_feature() {
    let raw = r#"
[signer.keeta_hot]
source = "env"
env = "KEETA_KEY"
[network."keeta:1413829460"]
signer = "keeta_hot"
indices = [0]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'keeta'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features keeta"), "got {err}");
}

#[cfg(feature = "keeta")]
#[test]
fn keeta_parses_signer_and_indices() {
    let raw = r#"
[signer.keeta_hot]
source = "env"
env = "KEETA_KEY"
[network."keeta:1413829460"]
signer = "keeta_hot"
indices = [0, 1]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("keeta");
    let keeta = cfg.networks.iter().find_map(|net| {
        if let Network::Keeta(keeta) = net {
            Some(keeta)
        } else {
            None
        }
    });
    let keeta = keeta.expect("keeta network");
    assert_eq!(keeta.signer, "keeta_hot", "named signer");
    assert_eq!(keeta.indices, [0, 1], "derivation indices");
}

#[cfg(feature = "keeta")]
#[test]
fn keeta_rejects_rpc() {
    let raw = r#"
[signer.keeta_hot]
source = "env"
env = "KEETA_KEY"
[network."keeta:1413829460"]
rpc = "http://127.0.0.1:1"
signer = "keeta_hot"
indices = [0]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(err.to_string().contains("Keeta has no RPC"), "got {err}");
    assert!(err.to_string().contains("`rpc`"), "got {err}");
}

#[cfg(feature = "keeta")]
#[test]
fn keeta_rejects_rpc_env() {
    let raw = r#"
[signer.keeta_hot]
source = "env"
env = "KEETA_KEY"
[network."keeta:1413829460"]
rpc_env = "KEETA_RPC"
signer = "keeta_hot"
indices = [0]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(err.to_string().contains("Keeta has no RPC"), "got {err}");
    assert!(err.to_string().contains("`rpc_env`"), "got {err}");
}

#[cfg(not(feature = "tvm"))]
#[test]
fn compiled_out_tvm_names_feature() {
    let raw = r#"
[signer.tvm_hot]
source = "env"
env = "TVM_KEY"
[network."tvm:-3"]
signer = "tvm_hot"
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'tvm'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features tvm"), "got {err}");
}

#[cfg(feature = "tvm")]
#[test]
fn tvm_optional_url_omit_ok() {
    let raw = r#"
[signer.tvm_hot]
source = "env"
env = "TVM_KEY"
[network."tvm:-3"]
signer = "tvm_hot"
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit url");
    let tvm = cfg.networks.iter().find_map(|net| {
        if let Network::Tvm(tvm) = net {
            Some(tvm)
        } else {
            None
        }
    });
    let tvm = tvm.expect("tvm network");
    assert!(tvm.provider_base_url.is_none(), "omit = Toncenter default");
    assert_eq!(tvm.signer, "tvm_hot", "named signer");
}

#[cfg(feature = "tvm")]
#[test]
fn tvm_provider_base_url_and_rpc_are_exclusive() {
    let raw = r#"
[signer.tvm_hot]
source = "env"
env = "TVM_KEY"
[network."tvm:-3"]
provider_base_url = "http://127.0.0.1:1"
rpc = "http://127.0.0.1:2"
signer = "tvm_hot"
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("at most one of `provider_base_url` or `rpc`"),
        "got {err}"
    );
}

#[cfg(not(feature = "stellar"))]
#[test]
fn compiled_out_stellar_names_feature() {
    let raw = r#"
[signer.stellar_hot]
source = "env"
env = "STELLAR_KEY"
[network."stellar:testnet"]
signers = ["stellar_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'stellar'"),
        "got {err}"
    );
    assert!(err.to_string().contains("--features stellar"), "got {err}");
}

#[cfg(feature = "stellar")]
#[test]
fn stellar_testnet_optional_rpc_omit_ok() {
    let raw = r#"
[signer.stellar_hot]
source = "env"
env = "STELLAR_KEY"
[network."stellar:testnet"]
signers = ["stellar_hot"]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit rpc");
    let stellar = cfg.networks.iter().find_map(|net| {
        if let Network::Stellar(stellar) = net {
            Some(stellar)
        } else {
            None
        }
    });
    let stellar = stellar.expect("stellar network");
    assert!(stellar.rpc.is_none(), "omit = SDK default on testnet");
}

#[cfg(feature = "stellar")]
#[test]
fn stellar_pubnet_requires_rpc() {
    let raw = r#"
[signer.stellar_hot]
source = "env"
env = "STELLAR_KEY"
[network."stellar:pubnet"]
signers = ["stellar_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("requires `rpc` or `rpc_env`"),
        "got {err}"
    );
    assert!(err.to_string().contains("pubnet"), "got {err}");
}

#[cfg(feature = "stellar")]
#[test]
fn stellar_rpc_and_rpc_env_are_exclusive() {
    let raw = r#"
[signer.stellar_hot]
source = "env"
env = "STELLAR_KEY"
[network."stellar:testnet"]
rpc = "http://127.0.0.1:1"
rpc_env = "STELLAR_RPC"
signers = ["stellar_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("at most one of `rpc` or `rpc_env`"),
        "got {err}"
    );
}

#[cfg(not(feature = "concordium"))]
#[test]
fn compiled_out_ccd_names_feature() {
    let raw = r#"
[signer.ccd_hot]
source = "env"
env = "CCD_KEY"
[network."ccd:4221332d34e1694168c2a0c0b3fd0f27"]
signers = [{ address = "2xdTv8awN1BjgYEw8W1BVXVtiEwG2b29U8KoZQqJrDuEqddseE", signer = "ccd_hot" }]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string().contains("compiled-out family 'ccd'"),
        "got {err}"
    );
    assert!(
        err.to_string().contains("--features concordium"),
        "got {err}"
    );
}

#[cfg(feature = "concordium")]
#[test]
fn ccd_optional_grpc_omit_ok() {
    let raw = r#"
[signer.ccd_hot]
source = "env"
env = "CCD_KEY"
[network."ccd:4221332d34e1694168c2a0c0b3fd0f27"]
signers = [{ address = "2xdTv8awN1BjgYEw8W1BVXVtiEwG2b29U8KoZQqJrDuEqddseE", signer = "ccd_hot" }]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("omit grpc");
    let ccd = cfg.networks.iter().find_map(|net| {
        if let Network::Concordium(ccd) = net {
            Some(ccd)
        } else {
            None
        }
    });
    let ccd = ccd.expect("ccd network");
    assert!(ccd.grpc.is_none(), "omit = default_grpc_https");
    assert_eq!(ccd.signer_names, ["ccd_hot".to_owned()], "flattened signer");
    assert_eq!(
        ccd.signers[0].address, "2xdTv8awN1BjgYEw8W1BVXVtiEwG2b29U8KoZQqJrDuEqddseE",
        "address required"
    );
}

#[cfg(feature = "concordium")]
#[test]
fn ccd_grpc_and_grpc_env_are_exclusive() {
    let raw = r#"
[signer.ccd_hot]
source = "env"
env = "CCD_KEY"
[network."ccd:4221332d34e1694168c2a0c0b3fd0f27"]
grpc = "http://127.0.0.1:1"
grpc_env = "CCD_GRPC"
signers = [{ address = "2xdTv8awN1BjgYEw8W1BVXVtiEwG2b29U8KoZQqJrDuEqddseE", signer = "ccd_hot" }]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("at most one of `grpc` or `grpc_env`"),
        "got {err}"
    );
}

#[cfg(feature = "concordium")]
#[test]
fn ccd_requires_address_and_signer() {
    let raw = r#"
[signer.ccd_hot]
source = "env"
env = "CCD_KEY"
[network."ccd:4221332d34e1694168c2a0c0b3fd0f27"]
signers = [{ signer = "ccd_hot" }]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("invalid [network.\"ccd:4221332d34e1694168c2a0c0b3fd0f27\"]"),
        "got {err}"
    );
}

#[test]
fn casper_is_always_unhostable() {
    let raw = r#"
[network."casper:casper"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("casper exact cannot be hosted"), "got {err}");
    assert!(msg.contains("remote HTTP client"), "got {err}");
    assert!(
        !msg.contains("rebuild with --features"),
        "casper must not suggest a Cargo feature, got {err}"
    );
    assert!(!msg.contains("extra-casper"), "got {err}");
}

#[test]
fn rpc_and_rpc_env_are_exclusive() {
    let raw = r#"
[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"
[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
rpc_env = "BASE_SEPOLIA_RPC"
signers = ["evm_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("exactly one of `rpc` or `rpc_env`"),
        "got {err}"
    );
}

#[test]
fn rpc_object_unknown_field_fails() {
    let raw = r#"
[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"
[network."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org", rate_limt = 50 }]
signers = ["evm_hot"]
schemes = ["exact"]
"#;
    let err = parse_config_toml(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("invalid [network.\"eip155:84532\"]"),
        "got {err}"
    );
}

#[test]
fn empty_env_secret_fails() {
    let src = facilitator::SecretSource::Env {
        env: "FACILITATOR_EVM_KEY".to_owned(),
    };
    let err = src
        .resolve(&|_| Some("  \n".to_owned()))
        .expect_err("empty env");
    assert!(err.to_string().contains("is empty"), "got {err}");
}

#[test]
fn rpc_object_form_parses() {
    let raw = r#"
[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"
[network."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org", rate_limit = 50 }]
signers = ["evm_hot"]
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("object rpc");
    match cfg.networks.first() {
        Some(Network::Evm(evm)) => match &evm.rpc {
            facilitator::RpcConfig::Literal(endpoints) => {
                assert_eq!(endpoints.len(), 1, "one endpoint");
                assert_eq!(endpoints[0].rate_limit, Some(50), "rate limit");
            }
            facilitator::RpcConfig::Env(env) => panic!("expected literal rpc, got env {env}"),
        },
        other => panic!("expected evm, got {other:?}"),
    }
}

#[test]
fn resolve_secrets_reads_env_lookup() {
    let cfg = parse_config_toml(&repo_file("config.example.toml")).expect("parse");
    cfg.resolve_secrets(&|key| (key == "FACILITATOR_EVM_KEY").then(|| "not-logged".to_owned()))
        .expect("lookup supplies the env signer");
}

#[test]
fn resolve_secrets_missing_env_fails() {
    let cfg = parse_config_toml(&repo_file("config.example.toml")).expect("parse");
    let err = cfg.resolve_secrets(&|_| None).unwrap_err();
    assert!(err.to_string().contains("FACILITATOR_EVM_KEY"), "got {err}");
}

#[test]
fn discovery_enabled_fails() {
    let raw = evm_doc("[discovery]\nenabled = true\n");
    let err = parse_config_toml(&raw).unwrap_err();
    assert!(err.to_string().contains("discovery.enabled"), "got {err}");
}

#[test]
fn receipt_timeout_must_fit_settle() {
    let raw = evm_doc("").replace("receipt_timeout_secs = 20", "receipt_timeout_secs = 29");
    let err = parse_config_toml(&raw).unwrap_err();
    assert!(
        err.to_string().contains("receipt_timeout_secs"),
        "got {err}"
    );
}

#[test]
fn http_auth_missing_env_fails() {
    let raw = evm_doc("[http.auth]\nbearer_env = \"FACILITATOR_API_TOKEN\"\n");
    let cfg = parse_config_toml(&raw).expect("auth table parses");
    let err = cfg
        .resolve_secrets(&|key| (key == "FACILITATOR_EVM_KEY").then(|| "not-logged".to_owned()))
        .unwrap_err();
    assert!(
        err.to_string().contains("FACILITATOR_API_TOKEN"),
        "got {err}"
    );
}

#[test]
fn http_auth_empty_bearer_env_fails() {
    let raw = evm_doc("[http.auth]\nbearer_env = \"\"\n");
    let err = parse_config_toml(&raw).unwrap_err();
    assert!(err.to_string().contains("bearer_env"), "got {err}");
}

#[test]
fn http_auth_resolves_token() {
    let raw = evm_doc("[http.auth]\nbearer_env = \"FACILITATOR_API_TOKEN\"\n");
    let cfg = parse_config_toml(&raw).expect("parses");
    let token = cfg
        .resolve_http_auth(&|key| {
            (key == "FACILITATOR_API_TOKEN").then(|| " shared-token \n".to_owned())
        })
        .expect("resolves")
        .expect("present");
    assert_eq!(token, "shared-token", "trimmed");
}
