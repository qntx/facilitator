//! Compose: EVM exact construction, `/supported` pass-through, startup errors.

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

fn lookup(key: &str) -> Option<String> {
    (key == "FACILITATOR_EVM_KEY").then(|| ANVIL_KEY.to_owned())
}

fn evm_doc(scheme_evm: &str, extra_network: &str) -> String {
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
schemes = ["exact"]
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
    let map = build(&cfg, &lookup).expect("construct");
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
    let map = build(&cfg, &lookup).expect("construct");
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
    let map = build(&cfg, &lookup).expect("construct");
    let supported = Facilitator::supported(&map).await.expect("supported");
    let extensions: Vec<&str> = supported.extensions.iter().map(AsRef::as_ref).collect();
    assert_eq!(
        extensions,
        ["erc20ApprovalGasSponsoring", "builder-code"],
        "config-driven identifiers the SDK does not put on kinds"
    );
}

#[cfg(feature = "evm")]
#[test]
fn listed_upto_without_constructor_is_startup_error() {
    let raw = evm_doc("", "").replace("schemes = [\"exact\"]", "schemes = [\"exact\", \"upto\"]");
    let cfg = parse_config_toml(&raw).expect("upto is a known EVM name");
    let err = build(&cfg, &lookup).expect_err("upto has no constructor in this PR");
    assert!(
        err.to_string()
            .contains("scheme 'upto' on eip155:84532 is not enabled in this build"),
        "got {err}"
    );
}

#[cfg(feature = "svm")]
#[test]
fn listed_svm_exact_without_constructor_is_startup_error() {
    let raw = r#"
[http]
listen = "127.0.0.1:8080"
settle_timeout = "30s"

[signer.svm_fee]
source = "env"
env = "FACILITATOR_SVM_KEY"

[network."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc = "http://127.0.0.1:1"
fee_payer = "svm_fee"
schemes = ["exact"]
"#;
    let cfg = parse_config_toml(raw).expect("svm tables parse");
    let err = build(&cfg, &|_| Some("unused".to_owned())).expect_err("svm exact not constructed");
    assert!(
        err.to_string().contains("is not enabled in this build"),
        "got {err}"
    );
}

#[cfg(feature = "evm")]
#[test]
fn missing_signer_secret_is_startup_error() {
    let cfg = parse_config_toml(&evm_doc("", "")).expect("parse");
    let err = build(&cfg, &|_| None).expect_err("missing key");
    assert!(err.to_string().contains("FACILITATOR_EVM_KEY"), "got {err}");
}

#[cfg(feature = "evm")]
#[test]
fn invalid_secp256k1_key_is_startup_error() {
    let cfg = parse_config_toml(&evm_doc("", "")).expect("parse");
    let err = build(&cfg, &|_| Some("not-a-key".to_owned())).expect_err("bad key");
    assert!(
        err.to_string()
            .contains("signer 'evm_hot' is not a valid secp256k1 hex key"),
        "got {err}"
    );
}

#[cfg(feature = "evm")]
#[test]
fn example_toml_stays_exact_only() {
    let raw = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.example.toml"),
    )
    .expect("example");
    assert!(
        !raw.contains("upto"),
        "config.example.toml must not list upto until PR 4"
    );
    let cfg = parse_config_toml(&raw).expect("example parses");
    for network in &cfg.networks {
        assert_eq!(network.schemes(), &["exact".to_owned()], "exact only");
    }
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
