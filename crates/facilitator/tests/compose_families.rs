//! Compose: NEAR / XRPL / Hedera / AVM exact construction.

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

#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm"
))]
use facilitator::{build, parse_config_toml};
#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm"
))]
use r402_facilitator::Facilitator;
#[cfg(any(feature = "near", feature = "hedera"))]
use r402_protocol::payment::SupportedResponse;

/// Dummy HTTP URL: `Provider::new` does not dial.
#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm"
))]
const DUMMY_RPC: &str = "http://127.0.0.1:1";

/// near-crypto test vector (`ed25519:` + base58 seed||pubkey).
#[cfg(feature = "near")]
const NEAR_KEY: &str = "ed25519:3D4YudUahN1nawWogh8pAKSj92sUNMdbZGjn7kERKzYoTy8tnFQuwoGUC51DowKqorvkr2pytJSnwuSbsNVfqygr";
#[cfg(feature = "near")]
const NEAR_ACCOUNT: &str = "relayer.testnet";

/// Hiero DER-hex ed25519 test key.
#[cfg(feature = "hedera")]
const HEDERA_KEY: &str = "302e020100300506032b65700422042098aa82d6125b5efa04bf8372be7931d05cd77f5ef3330b97d6ee7c006eaaf312";
#[cfg(feature = "hedera")]
const HEDERA_ACCOUNT: &str = "0.0.5001";

/// 32-byte ed25519 seed `[2u8; 32]` as standard base64.
#[cfg(feature = "avm")]
const AVM_KEY: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";
#[cfg(feature = "avm")]
const AVM_TESTNET: &str = "algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe";

#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm"
))]
fn lookup(key: &str) -> Option<String> {
    match key {
        #[cfg(feature = "near")]
        "FACILITATOR_NEAR_KEY" => Some(NEAR_KEY.to_owned()),
        #[cfg(feature = "hedera")]
        "FACILITATOR_HEDERA_KEY" => Some(HEDERA_KEY.to_owned()),
        #[cfg(feature = "avm")]
        "FACILITATOR_AVM_KEY" => Some(AVM_KEY.to_owned()),
        _ => None,
    }
}

#[cfg(feature = "near")]
#[tokio::test]
async fn near_exact_constructs() {
    let cfg = parse_config_toml(&near_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_exact_kind_at(&supported, 0, "near:testnet");
    let signers = supported.signers.get("near:*").expect("near:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [NEAR_ACCOUNT],
        "relayer account"
    );
}

#[cfg(feature = "near")]
#[tokio::test]
async fn near_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&near_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_NEAR_KEY").then(|| "not-a-near-key".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'near_relayer' is not a valid NEAR secret key"),
        "got {err}"
    );
}

#[cfg(feature = "xrpl")]
#[tokio::test]
async fn xrpl_exact_constructs() {
    let cfg = parse_config_toml(&xrpl_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    let kind = &supported.kinds[0];
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), "xrpl:1", "network");
    let extra = kind.extra.as_ref().expect("xrpl extra");
    assert_eq!(extra["areFeesSponsored"], false, "payer pays fees");
    let signers = supported.signers.get("xrpl:*").expect("xrpl:* signer key");
    assert!(signers.is_empty(), "no hot wallet");
}

#[cfg(feature = "hedera")]
#[tokio::test]
async fn hedera_exact_constructs() {
    let cfg = parse_config_toml(&hedera_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_exact_kind_at(&supported, 0, "hedera:testnet");
    let kind = &supported.kinds[0];
    let extra = kind.extra.as_ref().expect("hedera extra");
    assert_eq!(extra["feePayer"], HEDERA_ACCOUNT, "fee payer extra");
    let signers = supported
        .signers
        .get("hedera:*")
        .expect("hedera:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [HEDERA_ACCOUNT],
        "fee payer"
    );
}

#[cfg(feature = "hedera")]
#[tokio::test]
async fn hedera_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&hedera_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_HEDERA_KEY").then(|| "not-a-hedera-key".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'hedera_fee' is not a valid Hedera private key"),
        "got {err}"
    );
}

#[cfg(feature = "avm")]
#[tokio::test]
async fn avm_exact_constructs() {
    let cfg = parse_config_toml(&avm_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    let kind = &supported.kinds[0];
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), AVM_TESTNET, "network");
    assert!(
        kind.extra
            .as_ref()
            .is_some_and(|extra| extra.get("feePayer").is_some()),
        "feePayer extra"
    );
    assert!(
        supported.signers.contains_key("algorand:*"),
        "algorand:* signer key"
    );
}

#[cfg(feature = "avm")]
#[tokio::test]
async fn avm_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&avm_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_AVM_KEY").then(|| "not-base64".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'algo_hot' is not a valid Algorand base64 seed"),
        "got {err}"
    );
}

#[cfg(feature = "near")]
fn near_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.near_relayer]
source = "env"
env = "FACILITATOR_NEAR_KEY"

[network."near:testnet"]
rpc = "{DUMMY_RPC}"
relayers = [{{ account_id = "{NEAR_ACCOUNT}", signer = "near_relayer" }}]
schemes = ["exact"]
"#
    )
}

#[cfg(feature = "xrpl")]
fn xrpl_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[network."xrpl:1"]
rpc = "{DUMMY_RPC}"
schemes = ["exact"]
"#
    )
}

#[cfg(feature = "hedera")]
fn hedera_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.hedera_fee]
source = "env"
env = "FACILITATOR_HEDERA_KEY"

[network."hedera:testnet"]
fee_payers = [{{ account_id = "{HEDERA_ACCOUNT}", signer = "hedera_fee" }}]
schemes = ["exact"]
mirror_url = "{DUMMY_RPC}"
"#
    )
}

#[cfg(feature = "avm")]
fn avm_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.algo_hot]
source = "env"
env = "FACILITATOR_AVM_KEY"

[network."{AVM_TESTNET}"]
signers = ["algo_hot"]
schemes = ["exact"]
algod_url = "{DUMMY_RPC}"
"#
    )
}

#[cfg(any(feature = "near", feature = "hedera"))]
fn assert_exact_kind_at(supported: &SupportedResponse, index: usize, network: &str) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
}
