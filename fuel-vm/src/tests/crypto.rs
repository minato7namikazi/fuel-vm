#![allow(non_snake_case)]

use alloc::{
    vec,
    vec::Vec,
};
use test_case::test_case;

use fuel_asm::{
    GTFArgs,
    PanicReason::MemoryOverflow,
    RegId,
    op,
};
use fuel_crypto::{
    Hasher,
    PublicKey,
    SecretKey,
    Signature,
    secp256r1::encode_pubkey,
};
use fuel_tx::{
    ConsensusParameters,
    TransactionBuilder,
};
use fuel_types::ChainId;
use rand::{
    SeedableRng,
    rngs::StdRng,
};
use sha3::{
    Digest,
    Keccak256,
};

use crate::{
    prelude::*,
    tests::test_helpers::set_full_word,
    util::test_helpers::check_expected_reason_for_instructions,
};

#[cfg(feature = "std")]
use crate::checked_transaction::CheckPredicateParams;
#[cfg(feature = "std")]
use crate::tests::predicate::TokioWithRayon;

use super::test_helpers::{
    assert_success,
    run_script,
};

#[test]
fn secp256k1_recover() {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let secret = SecretKey::random(rng);
    let public = secret.public_key();

    let message = b"The gift of words is the gift of deception and illusion.";
    let message = Message::new(message);

    let signature = Signature::sign(&secret, &message);

    #[rustfmt::skip]
    let script = vec![
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, signature.as_ref().len() as Immediate12),
        op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        op::eck1(0x11, 0x20, 0x21),
        op::meq(0x12, 0x22, 0x11, 0x10),
        op::log(0x12, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    let script_data = signature
        .as_ref()
        .iter()
        .copied()
        .chain(message.as_ref().iter().copied())
        .chain(public.as_ref().iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));

    assert!(success);
}

#[test]
fn ecrecover_tx_id() {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let secret = SecretKey::random(rng);
    let public = secret.public_key();
    let chain_id = ChainId::default();

    #[rustfmt::skip]
    let script = vec![
        // 0x21 is a address of the singer of the witness
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::move_(0x21, 0x20),
        // 0x22 is a witness - signature
        op::gtf_args(0x22, 0x00, GTFArgs::WitnessData),
        // TxId is stored in the first 32 bytes of the memory
        // Store it into register 0x23
        op::movi(0x23, 0),
        // Allocate space for the recovered public key
        // 0x10 contains the size of the public key = PublicKey::LEN
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        // Recover public key into `0x11` from `0x22` signature and TxId `0x23`
        op::eck1(0x11, 0x22, 0x23),
        // Compare address `0x21` from script data with with recovered `0x11`
        // for length `0x10` = PublicKey::LEN
        op::meq(0x12, 0x21, 0x11, 0x10),
        op::ret(0x12),
    ].into_iter().collect();

    let script_data = public.as_ref().to_vec();

    let mut tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize();

    tx.sign_inputs(&secret, &chain_id);

    let consensus_params = ConsensusParameters::standard_with_id(chain_id);
    let tx = tx.into_checked(height, &consensus_params).unwrap();

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Return{ val, .. } if *val == 1));

    assert!(success);
}

#[cfg(feature = "std")]
#[tokio::test]
async fn recover_tx_id_predicate() {
    use crate::{
        checked_transaction::EstimatePredicates,
        pool::DummyPool,
    };
    use rand::Rng;
    let rng = &mut StdRng::seed_from_u64(1234u64);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let secret = SecretKey::random(rng);
    let public = secret.public_key();

    let check_params = CheckPredicateParams::default();
    let consensus_params = ConsensusParameters::standard();

    #[rustfmt::skip]
    let predicate = vec![
        // 0x21 is a address of the singer of the witness
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::move_(0x21, 0x20),
        // 0x22 is a witness - signature
        op::gtf_args(0x22, 0x00, GTFArgs::WitnessData),
        // TxId is stored in the first 32 bytes of the memory
        // Store it into register 0x23
        op::movi(0x23, 0),
        // Allocate space for the recovered public key
        // 0x10 contains the size of the public key = PublicKey::LEN
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        // Recover public key into `0x11` from `0x22` signature and TxId `0x23`
        op::eck1(0x11, 0x22, 0x23),
        // Compare address `0x21` from script data with with recovered `0x11`
        // for length `0x10` = PublicKey::LEN
        op::meq(0x12, 0x21, 0x11, 0x10),
        op::ret(0x12),
    ].into_iter().collect();

    let script_data = public.as_ref().to_vec();

    let input = Input::coin_predicate(
        rng.r#gen(),
        Input::predicate_owner(&predicate),
        1000,
        rng.r#gen(),
        Default::default(),
        0,
        predicate,
        vec![],
    );

    let mut tx = TransactionBuilder::script(vec![], script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_input(input)
        .add_unsigned_coin_input(
            secret,
            rng.r#gen(),
            rng.r#gen(),
            rng.r#gen(),
            Default::default(),
        )
        .finalize();

    {
        // parallel version
        let mut tx_for_async = tx.clone();
        tx_for_async
            .estimate_predicates_async::<TokioWithRayon>(
                &check_params,
                &DummyPool,
                &crate::storage::predicate::EmptyStorage,
            )
            .await
            .expect("Should estimate predicate successfully");

        tx_for_async
            .into_checked(maturity, &consensus_params)
            .expect("Should check predicate successfully");
    }

    // sequential version
    tx.estimate_predicates(
        &check_params,
        MemoryInstance::new(),
        &crate::storage::predicate::EmptyStorage,
    )
    .expect("Should estimate predicate successfully");

    tx.into_checked(maturity, &consensus_params)
        .expect("Should check predicate successfully");
}

#[test]
fn secp256k1_recover_error() {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let secret = SecretKey::random(rng);

    let message = b"The gift of words is the gift of deception and illusion.";
    let message = Message::new(message);

    let signature = Signature::sign(&secret, &message);

    #[rustfmt::skip]
    let script = vec![
        // op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, signature.as_ref().len() as Immediate12),
        op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        op::eck1(0x11, 0x20, 0x21),
        op::log(RegId::ERR, RegId::ZERO, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];

    let receipts = run_script(script);
    assert_success(&receipts);

    let Some(Receipt::Log { ra, .. }) = receipts.first() else {
        panic!("Expected log receipt");
    };

    assert_eq!(*ra, 1, "Verification should have failed");
}

#[test]
fn secp256k1_recover__register_a_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 63),
        op::eck1(reg_a, reg_b, reg_b),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn secp256k1_recover__register_b_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 63),
        op::eck1(reg_b, reg_a, reg_b),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn secp256k1_recover__register_c_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 31),
        op::eck1(reg_b, reg_b, reg_a),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn secp256r1_recover() {
    use p256::ecdsa::SigningKey;

    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let message = b"The gift of words is the gift of deception and illusion.";
    let message = Message::new(message);

    let secret_key = SigningKey::random(rng);
    let (signature, _recovery_id) =
        secret_key.sign_prehash_recoverable(&*message).unwrap();
    let public_key = secret_key.verifying_key();

    #[rustfmt::skip]
    let script = vec![
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, signature.to_bytes().len() as Immediate12),
        op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
        op::movi(0x10, 64),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        op::ecr1(0x11, 0x20, 0x21),
        op::meq(0x12, 0x22, 0x11, 0x10),
        op::log(0x12, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    let script_data = signature
        .to_bytes()
        .iter()
        .copied()
        .chain(message.as_ref().iter().copied())
        .chain(encode_pubkey(*public_key))
        .collect();

    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));

    assert!(success);
}

#[test]
fn secp256r1_recover_error() {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let secret = SecretKey::random(rng);

    let message = b"The gift of words is the gift of deception and illusion.";
    let message = Message::new(message);

    let signature = Signature::sign(&secret, &message);

    #[rustfmt::skip]
    let script = vec![
        // op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, signature.as_ref().len() as Immediate12),
        op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        op::ecr1(0x11, 0x20, 0x21),
        op::log(RegId::ERR, RegId::ZERO, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];

    let receipts = run_script(script);
    assert_success(&receipts);

    let Some(Receipt::Log { ra, .. }) = receipts.first() else {
        panic!("Expected log receipt");
    };

    assert_eq!(*ra, 1, "Verification should have failed");
}

#[test]
fn secp256r1_recover__register_a_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 63),
        op::ecr1(reg_a, reg_b, reg_b),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn secp256r1_recover__register_b_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 63),
        op::ecr1(reg_b, reg_a, reg_b),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn secp256r1_recover__register_c_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 31),
        op::ecr1(reg_b, reg_b, reg_a),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn ed25519_verifies_message() {
    use ed25519_dalek::Signer;

    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let mut rng = rand::rngs::OsRng;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);

    let message = b"The gift of words is the gift of deception and illusion.";
    let signature = signing_key.sign(&message[..]);

    let mut script = set_full_word(0x23, message.len() as Word);
    script.extend([
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, signature.to_bytes().len() as Immediate12),
        op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::ed19(0x22, 0x20, 0x21, 0x23),
        op::log(RegId::ERR, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ]);

    let script: Vec<u8> = script.into_iter().collect();

    // Success case
    let script_data = signature
        .to_bytes()
        .iter()
        .copied()
        .chain(message.as_ref().iter().copied())
        .chain(signing_key.verifying_key().as_ref().iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script.clone(), script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 0));

    assert!(success);

    // If we alter the message, the verification should fail
    let altered_message = b"The gift of words is the gift of deception and illusion!";
    assert_eq!(message.len(), altered_message.len());

    let script_data = signature
        .to_bytes()
        .iter()
        .copied()
        .chain(altered_message.as_ref().iter().copied())
        .chain(signing_key.verifying_key().as_ref().iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script.clone(), script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let errors = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));

    assert!(errors);

    // And if we alter the signature, the verification should also fail
    let altered_signature = signing_key.sign(&altered_message[..]);

    let script_data = altered_signature
        .to_bytes()
        .iter()
        .copied()
        .chain(message.as_ref().iter().copied())
        .chain(signing_key.verifying_key().as_ref().iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let errors = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));

    assert!(errors);
}

#[test]
fn ed25519_zero_length_is_treated_as_32() {
    use ed25519_dalek::Signer;

    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let mut rng = rand::rngs::OsRng;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);

    let message = [1u8; 32];
    let signature = signing_key.sign(&message[..]);

    let script = vec![
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, signature.to_bytes().len() as Immediate12),
        op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
        op::movi(0x10, PublicKey::LEN as Immediate18),
        op::aloc(0x10),
        op::ed19(0x22, 0x20, 0x21, 0),
        op::log(RegId::ERR, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ];

    let script: Vec<u8> = script.into_iter().collect();

    let script_data = signature
        .to_bytes()
        .iter()
        .copied()
        .chain(message.as_ref().iter().copied())
        .chain(signing_key.verifying_key().as_ref().iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script.clone(), script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 0));

    assert!(success);
}

#[test]
fn ed25519_verify__register_a_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;
    let reg_c = 0x22;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 63),
        op::movi(reg_c, 32),
        op::ed19(reg_a, reg_b, reg_b, reg_c),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn ed25519_verify__register_b_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;
    let reg_c = 0x22;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, 63),
        op::movi(reg_c, 32),
        op::ed19(reg_b, reg_a, reg_b, reg_c),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test_case(31, 32 => (); "Just over the end with 32 bits")]
#[test_case(63, 64 => (); "Just over the end with 64 bits")]
#[test_case(31, 0 => (); "Zero defaults to 32")]
#[test_case(31, 100 => (); "Way over the end")]
#[test_case(0, 32 => (); "Empty range, goes over it")]
fn ed25519_verify__message_overflows_ram(offset: u16, len: u32) {
    let reg_a = 0x20;
    let reg_b = 0x21;
    let reg_c = 0x22;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::subi(reg_a, reg_a, offset),
        op::movi(reg_c, len),
        op::ed19(reg_b, reg_b, reg_a, reg_c),
        op::ret(RegId::ONE),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn sha256() {
    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let message = b"I say let the world go to hell, but I should always have my tea.";
    let hash = Hasher::hash(message);

    #[rustfmt::skip]
    let script = vec![
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, message.len() as Immediate12),
        op::movi(0x10, Bytes32::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        op::movi(0x12, message.len() as Immediate18),
        op::s256(0x11, 0x20, 0x12),
        op::meq(0x13, 0x11, 0x21, 0x10),
        op::log(0x13, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    let script_data = message
        .iter()
        .copied()
        .chain(hash.as_ref().iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));

    assert!(success);
}

#[test]
fn s256__register_a_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::s256(reg_a, reg_b, reg_b),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn s256__register_c_overflows() {
    let reg_a = 0x20;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::s256(RegId::ZERO, RegId::ZERO, reg_a),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn s256___register_b_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::s256(reg_b, reg_a, reg_b),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn keccak256() {
    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    let message = b"...and, moreover, I consider it my duty to warn you that the cat is an ancient, inviolable animal.";

    let mut hasher = Keccak256::new();
    hasher.update(message);
    let hash = hasher.finalize();

    #[rustfmt::skip]
    let script = vec![
        op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
        op::addi(0x21, 0x20, message.len() as Immediate12),
        op::movi(0x10, Bytes32::LEN as Immediate18),
        op::aloc(0x10),
        op::move_(0x11, RegId::HP),
        op::movi(0x12, message.len() as Immediate18),
        op::k256(0x11, 0x20, 0x12),
        op::meq(0x13, 0x11, 0x21, 0x10),
        op::log(0x13, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    let script_data = message
        .iter()
        .copied()
        .chain(hash.iter().copied())
        .collect();

    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));

    assert!(success);
}

#[test]
fn k256__register_a_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::k256(reg_a, reg_b, reg_b),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn k256_c_gt_mem_max() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::k256(reg_b, reg_b, reg_a),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn k256__register_b_overflows() {
    let reg_a = 0x20;
    let reg_b = 0x21;

    #[rustfmt::skip]
    let script = vec![
        op::not(reg_a, RegId::ZERO),
        op::k256(reg_b, reg_a, reg_b),
    ];

    check_expected_reason_for_instructions(script, MemoryOverflow);
}

#[test]
fn ecop__addition__works() {
    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    // Given
    #[rustfmt::skip]
    let script = vec![
        // Get the points and expected result from the script data
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        // Store the expect result pointer to 0x11 reg (add 128 (points len) to the script data pointer)
        op::addi(0x11, 0x10, 0x80),
        // Store 64 bytes to allocate for the result
        op::movi(0x12, 0x40),
        // Allocate 64 bytes for the result
        op::aloc(0x12),
        // Store the result pointer to 0x12 reg
        op::move_(0x12, RegId::HP),
        // Perform addition of the two points
        op::ecop(0x12, RegId::ZERO, RegId::ZERO, 0x10),
        // Store the len of result in 0x13
        op::movi(0x13, 0x40),
        // Compare the result with the expected result and store 0 or 1 in 0x13
        op::meq(0x13, 0x11, 0x12, 0x13),
        // Log the result of the comparison
        op::log(0x13, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    // Point 1 + Point 2 + Result
    let mut script_data = Vec::new();
    // Point 1
    script_data.extend(
        hex::decode(
            "\
        18b18acfb4c2c30276db5411368e7185b311dd124691610c5d3b74034e093dc9\
        063c909c4720840cb5134cb9f59fa749755796819658d32efc0d288198f37266",
        )
        .unwrap(),
    );
    // Point 2
    script_data.extend(
        hex::decode(
            "\
        07c2b7f58a84bd6145f00c9c2bc0bb1a187f20ff2c92963a88019e7c6a014eed\
        06614e20c147e940f2d70da3f74c9a17df361706a4485c742bd6788478fa17d7",
        )
        .unwrap(),
    );
    // Result
    script_data.extend(
        hex::decode(
            "\
        2243525c5efd4b9c3d3c45ac0ca3fe4dd85e830a4ce6b65fa1eeaee202839703\
        301d1d33be6da8e509df21cc35964723180eed7532537db9ae5e7d48f195c915",
        )
        .unwrap(),
    );

    // When
    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);

    // Then
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));
    assert!(success);
}

#[test]
fn ecop__multiplication__works() {
    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    // Given
    #[rustfmt::skip]
    let script = vec![
        // Get the point, scalar and expected result from the script data
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        // Store the expect result pointer to 0x11 reg (add 96 (point + scalar len) to the script data pointer)
        op::addi(0x11, 0x10, 0x60),
        // Store 64 bytes to allocate for the result
        op::movi(0x12, 0x40),
        // Allocate 64 bytes for the result
        op::aloc(0x12),
        // Store the result pointer to 0x12 reg
        op::move_(0x12, RegId::HP),
        // Perform multiplication of the two points
        op::ecop(0x12, RegId::ZERO, RegId::ONE, 0x10),
        // Store the len of result in 0x13
        op::movi(0x13, 0x40),
        // Compare the result with the expected result and store 0 or 1 in 0x13
        op::meq(0x13, 0x11, 0x12, 0x13),
        // Log the result of the comparison
        op::log(0x13, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    // Point 1 + Scalar + Result
    let mut script_data = Vec::new();
    // Point 1
    script_data.extend(
        hex::decode(
            "\
        2bd3e6d0f3b142924f5ca7b49ce5b9d54c4703d7ae5648e61d02268b1a0a9fb7\
        21611ce0a6af85915e2f1d70300909ce2e49dfad4a4619c8390cae66cefdb204",
        )
        .unwrap(),
    );
    // Scalar
    script_data.extend(
        hex::decode(
            "\
        00000000000000000000000000000000000000000000000011138ce750fa15c2",
        )
        .unwrap(),
    );
    // Result
    script_data.extend(
        hex::decode(
            "\
        070a8d6a982153cae4be29d434e8faef8a47b274a053f5a4ee2a6c9c13c31e5c\
        031b8ce914eba3a9ffb989f9cdd5b0f01943074bf4f0f315690ec3cec6981afc",
        )
        .unwrap(),
    );

    // When
    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);

    // Then
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));
    assert!(success);
}

#[test]
fn epar__works() {
    let mut client = MemoryClient::default();

    let gas_limit = 1_000_000;
    let maturity = Default::default();
    let height = Default::default();

    // Given
    #[rustfmt::skip]
    let script = vec![
        // Get the point, scalar and expected result from the script data
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        // Store the number of batchs in 0x11
        op::movi(0x11, 0x02),
        // Perform multiplication of the two points
        op::epar(0x12, RegId::ZERO, 0x11, 0x10),
        // Log the result of the comparison
        op::log(0x12, 0x00, 0x00, 0x00),
        op::ret(RegId::ONE),
    ].into_iter().collect();

    // Batch 1 + Batch 2
    let mut script_data = Vec::new();
    // Batch 1
    script_data.extend(
        hex::decode(
            "\
        1c76476f4def4bb94541d57ebba1193381ffa7aa76ada664dd31c16024c43f59\
        3034dd2920f673e204fee2811c678745fc819b55d3e9d294e45c9b03a76aef41\
        209dd15ebff5d46c4bd888e51a93cf99a7329636c63514396b4a452003a35bf7\
        04bf11ca01483bfa8b34b43561848d28905960114c8ac04049af4b6315a41678\
        2bb8324af6cfc93537a2ad1a445cfd0ca2a71acd7ac41fadbf933c2a51be344d\
        120a2a4cf30c1bf9845f20c6fe39e07ea2cce61f0c9bb048165fe5e4de877550",
        )
        .unwrap(),
    );
    // Batch 2
    script_data.extend(
        hex::decode(
            "\
        111e129f1cf1097710d41c4ac70fcdfa5ba2023c6ff1cbeac322de49d1b6df7c\
        2032c61a830e3c17286de9462bf242fca2883585b93870a73853face6a6bf411\
        198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c2\
        1800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed\
        090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b\
        12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa",
        )
        .unwrap(),
    );

    // When
    let tx = TransactionBuilder::script(script, script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_checked(height);

    let receipts = client.transact(tx);

    // Then
    let success = receipts
        .iter()
        .any(|r| matches!(r, Receipt::Log{ ra, .. } if *ra == 1));
    assert!(success);
}
