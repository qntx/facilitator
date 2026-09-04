//! Compose: EVM exact/upto and SVM exact/upto construction, `/supported` pass-through.

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

use facilitator::{build, parse_config_toml};
use r402_facilitator::Facilitator;
use r402_protocol::payment::SupportedResponse;

/// Anvil account 0. Construction-only; never broadcast in these tests.
const ANVIL_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_ADDR: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
/// Deterministic 64-byte Solana keypair (`Keypair::new_from_array([7u8; 32])`).
#[cfg(feature = "svm")]
const SVM_KEY: &str =
    "99eUso3aSbE9tqGSTXzo3TLfKb9RkMTURrHKQ1K7Zh3StnzFNUx8FKCPPPPpR479qsw5zv2WNBKmgiz7WqgAJfM";
#[cfg(feature = "svm")]
const SVM_ADDR: &str = "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
#[cfg(feature = "svm")]
const SVM_DEVNET: &str = "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1";

fn lookup(key: &str) -> Option<String> {
    match key {
        "FACILITATOR_EVM_KEY" => Some(ANVIL_KEY.to_owned()),
        #[cfg(feature = "svm")]
        "FACILITATOR_SVM_KEY" => Some(SVM_KEY.to_owned()),
        _ => None,
    }
}

fn evm_doc(scheme_evm: &str, extra_network: &str) -> String {
    evm_doc_with_schemes(scheme_evm, extra_network, r#"["exact"]"#)
}

fn evm_doc_with_schemes(scheme_evm: &str, extra_network: &str, schemes: &str) -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"

{scheme_evm}

[network."eip155:84532"]
rpc = ["http://127.0.0.1:1"]
signers = ["evm_hot"]
schemes = {schemes}
receipt_timeout_secs = 20
{extra_network}
"#
    )
}

fn scheme_evm(body: &str) -> String {
    if body.is_empty() {
        String::new()
    } else {
        format!("[scheme.evm]\n{body}\n")
    }
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn exact_supported_passes_through_sdk_kinds_and_signers() {
    let cfg = parse_config_toml(&evm_doc("", "")).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_exact_kind(&supported, "eip155:84532");
    let signers = supported
        .signers
        .get("eip155:*")
        .expect("eip155:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [ANVIL_ADDR],
        "checksummed signer"
    );
    assert!(supported.extensions.is_empty(), "no extra extensions");
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn two_networks_concat_kinds_and_union_signers() {
    let extra = r#"
[network."eip155:8453"]
rpc = ["http://127.0.0.1:1"]
signers = ["evm_hot"]
schemes = ["exact"]
receipt_timeout_secs = 20
"#;
    let cfg = parse_config_toml(&evm_doc("", extra)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 2, "one kind per network");
    assert_exact_kind_at(&supported, 0, "eip155:84532");
    assert_exact_kind_at(&supported, 1, "eip155:8453");
    let signers = supported
        .signers
        .get("eip155:*")
        .expect("union under eip155:*");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [ANVIL_ADDR],
        "deduped address"
    );
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn erc20_and_builder_code_append_extension_ids() {
    let cfg = parse_config_toml(&evm_doc(
        &scheme_evm(
            r#"erc20_approval_gas_sponsoring = true
builder_code = { builder_code = "wallet", service_code = "svc" }"#,
        ),
        "",
    ))
    .expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    let extensions: Vec<&str> = supported.extensions.iter().map(AsRef::as_ref).collect();
    assert_eq!(
        extensions,
        ["erc20ApprovalGasSponsoring", "builder-code"],
        "config-driven identifiers the SDK does not put on kinds"
    );
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn exact_and_upto_concat_kinds_and_keep_upto_extra() {
    let cfg =
        parse_config_toml(&evm_doc_with_schemes("", "", r#"["exact", "upto"]"#)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 2, "exact then upto");
    assert_exact_kind_at(&supported, 0, "eip155:84532");
    assert_upto_kind_at(&supported, 1, "eip155:84532");
    let signers = supported
        .signers
        .get("eip155:*")
        .expect("union under eip155:*");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [ANVIL_ADDR],
        "deduped address"
    );
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn upto_only_supported_passes_through_extra() {
    let cfg = parse_config_toml(&evm_doc_with_schemes("", "", r#"["upto"]"#)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "upto only");
    assert_upto_kind_at(&supported, 0, "eip155:84532");
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn auth_capture_only_supported_passes_through_sdk_kind() {
    let cfg =
        parse_config_toml(&evm_doc_with_schemes("", "", r#"["auth-capture"]"#)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "auth-capture only");
    assert_auth_capture_kind_at(&supported, 0, "eip155:84532");
    let signers = supported
        .signers
        .get("eip155:*")
        .expect("eip155:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [ANVIL_ADDR],
        "checksummed signer"
    );
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn batch_settlement_supported_passes_through_sdk_kinds() {
    let cfg =
        parse_config_toml(&evm_doc_with_schemes("", "", r#"["batch-settlement"]"#)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "batch-settlement only");
    assert_batch_settlement_kind_at(&supported, 0, "eip155:84532");
    let signers = supported
        .signers
        .get("eip155:*")
        .expect("eip155:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [ANVIL_ADDR],
        "checksummed signer"
    );
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn exact_upto_and_auth_capture_concat_kinds() {
    let cfg = parse_config_toml(&evm_doc_with_schemes(
        "",
        "",
        r#"["exact", "upto", "auth-capture"]"#,
    ))
    .expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(
        supported.kinds.len(),
        3,
        "exact then upto then auth-capture"
    );
    assert_exact_kind_at(&supported, 0, "eip155:84532");
    assert_upto_kind_at(&supported, 1, "eip155:84532");
    assert_auth_capture_kind_at(&supported, 2, "eip155:84532");
    let signers = supported
        .signers
        .get("eip155:*")
        .expect("union under eip155:*");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [ANVIL_ADDR],
        "deduped address"
    );
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn exact_upto_and_batch_settlement_concat_kinds() {
    let cfg = parse_config_toml(&evm_doc_with_schemes(
        "",
        "",
        r#"["exact", "upto", "batch-settlement"]"#,
    ))
    .expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(
        supported.kinds.len(),
        3,
        "exact then upto then batch-settlement"
    );
    assert_exact_kind_at(&supported, 0, "eip155:84532");
    assert_upto_kind_at(&supported, 1, "eip155:84532");
    assert_batch_settlement_kind_at(&supported, 2, "eip155:84532");
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn svm_exact_supported_passes_through_feepayer_and_signers() {
    let cfg = parse_config_toml(&svm_doc("", "")).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    assert_svm_exact_kind_at(&supported, 0, SVM_DEVNET, SVM_ADDR);
    let signers = supported
        .signers
        .get("solana:*")
        .expect("solana:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [SVM_ADDR],
        "fee payer"
    );
    assert!(supported.extensions.is_empty(), "no extra extensions");
}

#[cfg(all(feature = "evm", feature = "svm"))]
#[tokio::test]
async fn evm_and_svm_concat_kinds_and_union_signer_keys() {
    let cfg = parse_config_toml(&evm_doc("", &svm_network_tables())).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 2, "evm exact then svm exact");
    assert_exact_kind_at(&supported, 0, "eip155:84532");
    assert_svm_exact_kind_at(&supported, 1, SVM_DEVNET, SVM_ADDR);
    assert!(supported.signers.contains_key("eip155:*"), "eip155:* union");
    assert!(supported.signers.contains_key("solana:*"), "solana:* union");
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn svm_upto_supported_passes_through_feepayer_and_signers() {
    let cfg = parse_config_toml(&svm_doc_with_schemes("", "", r#"["upto"]"#)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 1, "one kind");
    assert_svm_upto_kind_at(&supported, 0, SVM_DEVNET, SVM_ADDR);
    let signers = supported
        .signers
        .get("solana:*")
        .expect("solana:* signer key");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [SVM_ADDR],
        "fee payer"
    );
    assert!(supported.extensions.is_empty(), "no extra extensions");
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn svm_exact_and_upto_concat_kinds_and_keep_extra() {
    let cfg =
        parse_config_toml(&svm_doc_with_schemes("", "", r#"["exact", "upto"]"#)).expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_eq!(supported.kinds.len(), 2, "exact then upto");
    assert_svm_exact_kind_at(&supported, 0, SVM_DEVNET, SVM_ADDR);
    assert_svm_upto_kind_at(&supported, 1, SVM_DEVNET, SVM_ADDR);
    let signers = supported
        .signers
        .get("solana:*")
        .expect("union under solana:*");
    assert_eq!(
        signers.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        [SVM_ADDR],
        "deduped fee payer"
    );
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn svm_upto_scheme_table_and_network_overlay_construct() {
    let cfg = parse_config_toml(&svm_doc_with_schemes(
        "[scheme.svm.upto]\nmax_channel_lifetime_secs = 3600\n",
        "[network.\"solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1\".upto]\nmax_compute_units = 300000\n",
        r#"["upto"]"#,
    ))
    .expect("parse");
    let map = build(&cfg, &lookup).await.expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    assert_svm_upto_kind_at(&supported, 0, SVM_DEVNET, SVM_ADDR);
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn path2_enabled_is_startup_error() {
    let cfg = parse_config_toml(&svm_doc(
        "[scheme.svm.exact]\nenable_smart_wallet_verification = true\n",
        "",
    ))
    .expect("parse");
    let err = build(&cfg, &lookup)
        .await
        .expect_err("Path 2 has no shared-cache API");
    assert!(
        err.to_string()
            .contains("enable_smart_wallet_verification = true"),
        "got {err}"
    );
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn path2_network_overlay_is_startup_error() {
    let cfg = parse_config_toml(&svm_doc(
        "",
        "[network.\"solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1\".exact]\nenable_smart_wallet_verification = true\n",
    ))
    .expect("parse");
    let err = build(&cfg, &lookup)
        .await
        .expect_err("network overlay Path 2");
    assert!(
        err.to_string()
            .contains("enable_smart_wallet_verification = true"),
        "got {err}"
    );
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn invalid_svm_keypair_is_startup_error() {
    let cfg = parse_config_toml(&svm_doc("", "")).expect("parse");
    let err = build(&cfg, &|key| {
        (key == "FACILITATOR_SVM_KEY").then(|| "not-a-keypair".to_owned())
    })
    .await
    .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'svm_fee' is not a valid base58 Solana keypair"),
        "got {err}"
    );
}

#[cfg(feature = "svm")]
#[tokio::test]
async fn missing_svm_signer_secret_is_startup_error() {
    let cfg = parse_config_toml(&svm_doc("", "")).expect("parse");
    let err = build(&cfg, &|_| None).await.expect_err("missing key");
    assert!(err.to_string().contains("FACILITATOR_SVM_KEY"), "got {err}");
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn missing_signer_secret_is_startup_error() {
    let cfg = parse_config_toml(&evm_doc("", "")).expect("parse");
    let err = build(&cfg, &|_| None).await.expect_err("missing key");
    assert!(err.to_string().contains("FACILITATOR_EVM_KEY"), "got {err}");
}

#[cfg(feature = "evm")]
#[tokio::test]
async fn invalid_secp256k1_key_is_startup_error() {
    let cfg = parse_config_toml(&evm_doc("", "")).expect("parse");
    let err = build(&cfg, &|_| Some("not-a-key".to_owned()))
        .await
        .expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'evm_hot' is not a valid secp256k1 hex key"),
        "got {err}"
    );
}

#[cfg(feature = "evm")]
#[test]
fn example_toml_lists_exact_and_upto() {
    let raw = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.example.toml"),
    )
    .expect("example");
    assert!(
        raw.contains(r#"schemes = ["exact", "upto"]"#),
        "config.example.toml must list exact+upto"
    );
    let cfg = parse_config_toml(&raw).expect("example parses");
    for network in &cfg.networks {
        assert_eq!(
            network.schemes(),
            &["exact".to_owned(), "upto".to_owned()],
            "exact+upto"
        );
    }
}

#[cfg(feature = "svm")]
fn svm_doc(scheme_svm: &str, extra: &str) -> String {
    svm_doc_with_schemes(scheme_svm, extra, r#"["exact"]"#)
}

#[cfg(feature = "svm")]
fn svm_doc_with_schemes(scheme_svm: &str, extra: &str, schemes: &str) -> String {
    format!(
        r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.svm_fee]
source = "env"
env = "FACILITATOR_SVM_KEY"

{scheme_svm}

[network."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc = "http://127.0.0.1:1"
fee_payer = "svm_fee"
schemes = {schemes}
{extra}
"#
    )
}

#[cfg(all(feature = "evm", feature = "svm"))]
fn svm_network_tables() -> String {
    r#"
[signer.svm_fee]
source = "env"
env = "FACILITATOR_SVM_KEY"

[network."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc = "http://127.0.0.1:1"
fee_payer = "svm_fee"
schemes = ["exact"]
"#
    .to_owned()
}

fn assert_exact_kind(supported: &SupportedResponse, network: &str) {
    assert_eq!(supported.kinds.len(), 1, "one kind");
    assert_exact_kind_at(supported, 0, network);
}

fn assert_exact_kind_at(supported: &SupportedResponse, index: usize, network: &str) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), "exact", "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
    assert_eq!(kind.extra, None, "exact extra is absent");
}

#[cfg(feature = "svm")]
fn assert_svm_exact_kind_at(
    supported: &SupportedResponse,
    index: usize,
    network: &str,
    fee_payer: &str,
) {
    assert_svm_kind_at(supported, index, "exact", network, fee_payer);
}

#[cfg(feature = "svm")]
fn assert_svm_upto_kind_at(
    supported: &SupportedResponse,
    index: usize,
    network: &str,
    fee_payer: &str,
) {
    assert_svm_kind_at(supported, index, "upto", network, fee_payer);
}

#[cfg(feature = "svm")]
fn assert_svm_kind_at(
    supported: &SupportedResponse,
    index: usize,
    scheme: &str,
    network: &str,
    fee_payer: &str,
) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), scheme, "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
    let extra = kind
        .extra
        .as_ref()
        .expect("svm extra.feePayer must not be stripped");
    assert_eq!(extra["feePayer"], fee_payer, "feePayer pass-through");
    assert!(
        extra.get("features").is_none(),
        "Path 2 off: no smartWalletSupported"
    );
}

fn assert_upto_kind_at(supported: &SupportedResponse, index: usize, network: &str) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), "upto", "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
    let extra = kind
        .extra
        .as_ref()
        .expect("upto extra must not be stripped");
    assert_eq!(extra["assetTransferMethod"], "permit2", "Permit2 method");
    assert_eq!(extra["facilitatorAddress"], ANVIL_ADDR, "first signer");
}

fn assert_auth_capture_kind_at(supported: &SupportedResponse, index: usize, network: &str) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), "auth-capture", "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
    assert_eq!(kind.extra, None, "auth-capture extra is absent");
}

fn assert_batch_settlement_kind_at(supported: &SupportedResponse, index: usize, network: &str) {
    let kind = &supported.kinds[index];
    assert_eq!(kind.x402_version, 2, "V2");
    assert_eq!(kind.scheme.as_str(), "batch-settlement", "scheme");
    assert_eq!(kind.network.as_str(), network, "network");
    assert_eq!(kind.extra, None, "batch-settlement extra is absent");
}
