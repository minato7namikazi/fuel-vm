use fuel_tx::*;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

#[test]
fn coin() {
    let mut rng_base = StdRng::seed_from_u64(8586);
    let rng = &mut rng_base;

    Output::coin(Address::random(rng), rng.next_u64(), Color::random(rng))
        .validate(1, &[])
        .unwrap();
}

#[test]
fn contract() {
    let mut rng_base = StdRng::seed_from_u64(8586);
    let rng = &mut rng_base;

    Output::contract(1, Bytes32::random(rng), Bytes32::random(rng))
        .validate(
            2,
            &[
                Input::coin(
                    Bytes32::random(rng),
                    Address::random(rng),
                    rng.next_u64(),
                    Color::random(rng),
                    rng.next_u32().to_be_bytes()[0],
                    rng.next_u64(),
                    Witness::random(rng).into_inner(),
                    Witness::random(rng).into_inner(),
                ),
                Input::contract(
                    Bytes32::random(rng),
                    Bytes32::random(rng),
                    Bytes32::random(rng),
                    ContractId::random(rng),
                ),
            ],
        )
        .unwrap();

    let err = Output::contract(0, Bytes32::random(rng), Bytes32::random(rng))
        .validate(
            2,
            &[
                Input::coin(
                    Bytes32::random(rng),
                    Address::random(rng),
                    rng.next_u64(),
                    Color::random(rng),
                    rng.next_u32().to_be_bytes()[0],
                    rng.next_u64(),
                    Witness::random(rng).into_inner(),
                    Witness::random(rng).into_inner(),
                ),
                Input::contract(
                    Bytes32::random(rng),
                    Bytes32::random(rng),
                    Bytes32::random(rng),
                    ContractId::random(rng),
                ),
            ],
        )
        .err()
        .unwrap();
    assert_eq!(ValidationError::OutputContractInputIndex { index: 2 }, err);

    let err = Output::contract(2, Bytes32::random(rng), Bytes32::random(rng))
        .validate(
            2,
            &[
                Input::coin(
                    Bytes32::random(rng),
                    Address::random(rng),
                    rng.next_u64(),
                    Color::random(rng),
                    rng.next_u32().to_be_bytes()[0],
                    rng.next_u64(),
                    Witness::random(rng).into_inner(),
                    Witness::random(rng).into_inner(),
                ),
                Input::contract(
                    Bytes32::random(rng),
                    Bytes32::random(rng),
                    Bytes32::random(rng),
                    ContractId::random(rng),
                ),
            ],
        )
        .err()
        .unwrap();
    assert_eq!(ValidationError::OutputContractInputIndex { index: 2 }, err);
}

#[test]
fn withdrawal() {
    let mut rng_base = StdRng::seed_from_u64(8586);
    let rng = &mut rng_base;

    Output::withdrawal(Address::random(rng), rng.next_u64(), Color::random(rng))
        .validate(1, &[])
        .unwrap();
}

#[test]
fn change() {
    let mut rng_base = StdRng::seed_from_u64(8586);
    let rng = &mut rng_base;

    Output::change(Address::random(rng), rng.next_u64(), Color::random(rng))
        .validate(1, &[])
        .unwrap();
}

#[test]
fn variable() {
    let mut rng_base = StdRng::seed_from_u64(8586);
    let rng = &mut rng_base;

    Output::variable(Address::random(rng), rng.next_u64(), Color::random(rng))
        .validate(1, &[])
        .unwrap();
}

#[test]
fn contract_created() {
    let mut rng_base = StdRng::seed_from_u64(8586);
    let rng = &mut rng_base;

    Output::contract_created(ContractId::random(rng))
        .validate(1, &[])
        .unwrap();
}
