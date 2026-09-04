//! Compose remaining exact families, including experimental-tron.
//! Concordium `connect` dials gRPC; CI tests parse and signer errors only.

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
    feature = "avm",
    feature = "aptos",
    feature = "keeta",
    feature = "tvm",
    feature = "stellar",
    feature = "concordium",
    feature = "experimental-tron"
))]
use facilitator::{build, parse_config_toml};
#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm",
    feature = "aptos",
    feature = "keeta",
    feature = "tvm",
    feature = "stellar",
    feature = "experimental-tron"
))]
use r402_facilitator::Facilitator;
#[cfg(any(feature = "near", feature = "hedera", feature = "aptos"))]
use r402_protocol::payment::SupportedResponse;

/// Dummy HTTP URL: `Provider::new` does not dial.
#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm",
    feature = "aptos",
    feature = "tvm",
    feature = "stellar",
    feature = "experimental-tron"
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
#[cfg(any(feature = "avm", feature = "keeta"))]
const AVM_KEY: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";
#[cfg(feature = "avm")]
const AVM_TESTNET: &str = "algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe";

/// 32-byte ed25519 private key as hex (`[2u8; 32]`).
#[cfg(feature = "aptos")]
const APTOS_KEY: &str = "0202020202020202020202020202020202020202020202020202020202020202";

/// 32-byte Highload seed as hex (`[7u8; 32]`).
#[cfg(feature = "tvm")]
const TVM_KEY: &str = "0707070707070707070707070707070707070707070707070707070707070707";

/// Oracle Stellar secret (`S…`).
#[cfg(feature = "stellar")]
const STELLAR_KEY: &str = "SCKB3ECHCPVM4HJPNCQWTQWJJ5XRL6UNKLTTCIH4B7TB22NKJ5GUFMIV";

/// Anvil account 0. Local signing only; never sent to a live chain.
#[cfg(feature = "experimental-tron")]
const TRON_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

/// 32-byte Concordium seed as hex.
#[cfg(feature = "concordium")]
const CCD_KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
#[cfg(feature = "concordium")]
const CCD_ADDRESS: &str = "2xdTv8awN1BjgYEw8W1BVXVtiEwG2b29U8KoZQqJrDuEqddseE";
#[cfg(feature = "concordium")]
const CCD_TESTNET: &str = "ccd:4221332d34e1694168c2a0c0b3fd0f27";

#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm",
    feature = "aptos",
    feature = "keeta",
    feature = "tvm",
    feature = "stellar",
    feature = "concordium",
    feature = "experimental-tron"
))]
fn lookup(key: &str) -> Option<String> {
    match key {
        #[cfg(feature = "near")]
        "FACILITATOR_NEAR_KEY" => Some(NEAR_KEY.to_owned()),
        #[cfg(feature = "hedera")]
        "FACILITATOR_HEDERA_KEY" => Some(HEDERA_KEY.to_owned()),
        #[cfg(feature = "avm")]
        "FACILITATOR_AVM_KEY" => Some(AVM_KEY.to_owned()),
        #[cfg(feature = "keeta")]
        "FACILITATOR_KEETA_KEY" => Some(AVM_KEY.to_owned()),
        #[cfg(feature = "aptos")]
        "FACILITATOR_APTOS_KEY" => Some(APTOS_KEY.to_owned()),
        #[cfg(feature = "tvm")]
        "FACILITATOR_TVM_KEY" => Some(TVM_KEY.to_owned()),
        #[cfg(feature = "stellar")]
        "FACILITATOR_STELLAR_KEY" => Some(STELLAR_KEY.to_owned()),
        #[cfg(feature = "concordium")]
        "FACILITATOR_CCD_KEY" => Some(CCD_KEY.to_owned()),
        #[cfg(feature = "experimental-tron")]
        "FACILITATOR_TRON_KEY" => Some(TRON_KEY.to_owned()),
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

#[cfg(feature = "aptos")]
#[tokio::test]
async fn aptos_exact_constructs() {
    let cfg = parse_config_toml(&aptos_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_exact_kind_at(&supported, 0, "aptos:1");
    assert!(
        supported.signers.contains_key("aptos:*"),
        "aptos:* signer key"
    );
}

#[cfg(feature = "aptos")]
#[tokio::test]
async fn aptos_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&aptos_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_APTOS_KEY").then(|| "not-an-aptos-key".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'aptos_hot' is not a valid Aptos private key"),
        "got {err}"
    );
}

#[cfg(feature = "keeta")]
#[tokio::test]
async fn keeta_exact_constructs() {
    let cfg = parse_config_toml(&keeta_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    let kind = &supported.kinds[0];
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), "keeta:1413829460", "network");
    assert!(
        supported.signers.contains_key("keeta:*"),
        "keeta:* signer key"
    );
}

#[cfg(feature = "keeta")]
#[tokio::test]
async fn keeta_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&keeta_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_KEETA_KEY").then(|| "not-base64".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'keeta_hot' is not a valid Keeta base64 seed"),
        "got {err}"
    );
}

#[cfg(feature = "tvm")]
#[tokio::test]
async fn tvm_exact_constructs() {
    let cfg = parse_config_toml(&tvm_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    let kind = &supported.kinds[0];
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), "tvm:-3", "network");
    assert!(supported.signers.contains_key("tvm:*"), "tvm:* signer key");
}

#[cfg(feature = "tvm")]
#[tokio::test]
async fn tvm_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&tvm_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_TVM_KEY").then(|| "not-a-tvm-key".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'tvm_hot' is not a valid TVM Highload V3 private key"),
        "got {err}"
    );
}

#[cfg(feature = "stellar")]
#[tokio::test]
async fn stellar_exact_constructs() {
    let cfg = parse_config_toml(&stellar_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    let kind = &supported.kinds[0];
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), "stellar:testnet", "network");
    assert!(
        supported.signers.contains_key("stellar:*"),
        "stellar:* signer key"
    );
}

#[cfg(feature = "stellar")]
#[tokio::test]
async fn stellar_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&stellar_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_STELLAR_KEY").then(|| "not-a-stellar-key".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'stellar_hot' is not a valid Stellar secret key"),
        "got {err}"
    );
}

#[cfg(feature = "concordium")]
#[tokio::test]
async fn concordium_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&ccd_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_CCD_KEY").then(|| "not-hex".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'ccd_hot' is not a valid Concordium address+seed"),
        "got {err}"
    );
}

#[cfg(feature = "experimental-tron")]
#[tokio::test]
async fn tron_exact_constructs() {
    let cfg = parse_config_toml(&tron_doc()).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    let kind = &supported.kinds[0];
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), "tron:0x2b6653dc", "network");
    assert!(
        supported.signers.contains_key("tron:*"),
        "tron:* signer key"
    );
}

#[cfg(feature = "experimental-tron")]
#[tokio::test]
async fn tron_invalid_key_is_startup_error() {
    let cfg = parse_config_toml(&tron_doc()).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_TRON_KEY").then(|| "not-a-secp256k1-key".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'tron_hot' is not a valid secp256k1 hex key"),
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

#[cfg(feature = "aptos")]
fn aptos_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.aptos_hot]
source = "env"
env = "FACILITATOR_APTOS_KEY"

[network."aptos:1"]
rpc = "{DUMMY_RPC}"
fee_payers = ["aptos_hot"]
schemes = ["exact"]
"#
    )
}

#[cfg(feature = "keeta")]
fn keeta_doc() -> String {
    r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.keeta_hot]
source = "env"
env = "FACILITATOR_KEETA_KEY"

[network."keeta:1413829460"]
signer = "keeta_hot"
indices = [0]
schemes = ["exact"]
"#
    .to_owned()
}

#[cfg(feature = "tvm")]
fn tvm_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.tvm_hot]
source = "env"
env = "FACILITATOR_TVM_KEY"

[network."tvm:-3"]
rpc = "{DUMMY_RPC}"
signer = "tvm_hot"
schemes = ["exact"]
"#
    )
}

#[cfg(feature = "stellar")]
fn stellar_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.stellar_hot]
source = "env"
env = "FACILITATOR_STELLAR_KEY"

[network."stellar:testnet"]
rpc = "{DUMMY_RPC}"
signers = ["stellar_hot"]
schemes = ["exact"]
"#
    )
}

#[cfg(feature = "concordium")]
fn ccd_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.ccd_hot]
source = "env"
env = "FACILITATOR_CCD_KEY"

[network."{CCD_TESTNET}"]
signers = [{{ address = "{CCD_ADDRESS}", signer = "ccd_hot" }}]
schemes = ["exact"]
"#
    )
}

#[cfg(feature = "experimental-tron")]
fn tron_doc() -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.tron_hot]
source = "env"
env = "FACILITATOR_TRON_KEY"

[network."tron:0x2b6653dc"]
rpc = "{DUMMY_RPC}"
signer = "tron_hot"
schemes = ["exact"]
"#
    )
}

#[cfg(any(feature = "near", feature = "hedera", feature = "aptos"))]
fn assert_exact_kind_at(supported: &SupportedResponse, index: usize, network: &str) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
}
