use alloc::{
    vec,
    vec::Vec,
};
use hashbrown::HashMap;

use fuel_asm::op;
use fuel_tx::{
    ConsensusParameters,
    Script,
};
use fuel_types::{
    AssetId,
    ContractId,
};
use test_case::test_case;

use crate::{
    constraints::reg_key::{
        HP,
        Reg,
        RegMut,
        SP,
    },
    consts::*,
    storage::MemoryStorage,
};

use super::*;

#[test]
fn identity() {
    let a = Interpreter::<_, _, Script>::without_storage();
    let b = Interpreter::<_, _, Script>::without_storage();
    let diff = a.rollback_to(&b);
    assert!(diff.changes.is_empty());
    assert_eq!(a, b);
}

#[test]
fn reset_vm_state() {
    let desired = Interpreter::<_, _, Script>::with_memory_storage();
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    latest.set_gas(1_000_000);
    latest
        .instruction::<_, false>(op::addi(0x10, 0x11, 1))
        .unwrap();
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    assert_ne!(desired, latest);
    latest.reset_vm_state(&diff);
    assert_eq!(desired, latest);
}

use crate::interpreter::InterpreterParams;

#[test]
fn record_and_invert_storage() {
    let arb_gas_price = 1;
    let interpreter_params =
        InterpreterParams::new(arb_gas_price, ConsensusParameters::standard());

    let desired = Interpreter::<_, _, Script>::with_storage(
        crate::interpreter::MemoryInstance::new(),
        Record::new(MemoryStorage::default()),
        interpreter_params.clone(),
    );
    let mut latest = Interpreter::<_, _, Script>::with_storage(
        crate::interpreter::MemoryInstance::new(),
        Record::new(MemoryStorage::default()),
        interpreter_params,
    );

    <Record<_> as StorageMutate<ContractsAssets>>::insert(
        &mut latest.storage,
        &(&ContractId::default(), &AssetId::default()).into(),
        &1u64,
    )
    .unwrap();
    latest.set_gas(1_000_000);
    latest
        .instruction::<_, false>(op::addi(0x10, 0x11, 1))
        .unwrap();

    let storage_diff: Diff<InitialVmState> = latest.storage_diff().into();
    let mut diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    diff.changes.extend(storage_diff.changes);

    assert_ne!(desired, latest);
    latest.reset_vm_state(&diff);
    assert_eq!(desired, latest);

    let c = Interpreter::<_, _, Script>::with_memory_storage();
    let mut d = Interpreter::<_, _, Script>::with_memory_storage();

    <MemoryStorage as StorageMutate<ContractsAssets>>::insert(
        &mut d.storage,
        &(&ContractId::default(), &AssetId::default()).into(),
        &1u64,
    )
    .unwrap();
    d.set_gas(1_000_000);
    d.instruction::<_, false>(op::addi(0x10, 0x11, 1)).unwrap();

    assert_ne!(c, d);
    d.reset_vm_state(&diff);
    assert_eq!(c, d);
}

#[test]
fn reset_vm_state_frame() {
    let desired = Interpreter::<_, _, Script>::with_memory_storage();
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    let frame = CallFrame::new(
        Default::default(),
        Default::default(),
        [0; VM_REGISTER_COUNT],
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    latest.frames.push(frame);
    assert_ne!(desired.frames, latest.frames);
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    latest.reset_vm_state(&diff);
    assert_eq!(desired.frames, latest.frames);
}

#[test]
fn reset_vm_state_receipts() {
    let desired = Interpreter::<_, _, Script>::with_memory_storage();
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    let receipt = Receipt::call(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    latest.receipts.push(receipt).expect("not full");
    assert_ne!(desired.receipts, latest.receipts);
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    latest.reset_vm_state(&diff);
    assert_eq!(desired.receipts, latest.receipts);
}

#[test_case(&[], &[] => it empty)]
#[test_case(&[1], &[] => vec![(0, Some(1), None)])]
#[test_case(&[1, 2], &[] => vec![(0, Some(1), None), (1, Some(2), None)])]
#[test_case(&[], &[1] => vec![(0, None, Some(1))])]
#[test_case(&[], &[1, 2] => vec![(0, None, Some(1)), (1, None, Some(2))])]
#[test_case(&[1], &[2] => vec![(0, Some(1), Some(2))])]
#[test_case(&[1, 3], &[2] => vec![(0, Some(1), Some(2)), (1, Some(3), None)])]
#[test_case(&[1], &[2, 4] => vec![(0, Some(1), Some(2)), (1, None, Some(4))])]
#[test_case(&[1, 3], &[2, 4] => vec![(0, Some(1), Some(2)), (1, Some(3), Some(4))])]
fn test_capture_vec_state(
    a: &[u32],
    b: &[u32],
) -> Vec<(usize, Option<u32>, Option<u32>)> {
    capture_vec_state_inner(a.iter(), b.iter()).collect()
}

#[test_case(&[], &[] => it empty)]
#[test_case(&[(12, 22)], &[] => vec![(12, Some(22), 12, None)])]
#[test_case(&[(12, 22), (13, 23)], &[] => vec![(12, Some(22), 12, None), (13, Some(23), 13, None)])]
#[test_case(&[], &[(12, 22)] => vec![(12, None, 12, Some(22))])]
#[test_case(&[], &[(12, 22), (13, 23)] => vec![(12, None, 12, Some(22)), (13, None, 13, Some(23))])]
#[test_case(&[(12, 22)], &[(13, 22)] => vec![(12, Some(22), 12, None), (13, None, 13, Some(22))])]
#[test_case(&[(12, 22), (13, 23)], &[(14, 24)] => vec![(12, Some(22), 12, None), (13, Some(23), 13, None), (14, None, 14, Some(24))])]
fn test_capture_map_state(
    a: &[(u32, u32)],
    b: &[(u32, u32)],
) -> Vec<(u32, Option<u32>, u32, Option<u32>)> {
    let a: HashMap<u32, u32> = a.iter().copied().collect();
    let a_keys = a.keys().collect();
    let b: HashMap<u32, u32> = b.iter().copied().collect();
    let b_keys = b.keys().collect();
    let mut v = capture_map_state_inner(&a, &a_keys, &b, &b_keys)
        .map(|d| (d.from.key, d.from.value, d.to.key, d.to.value))
        .collect::<Vec<_>>();
    v.sort_unstable_by_key(|k| k.0);
    v
}

#[test_case(&[], 0, None => it empty)]
#[test_case(&[12], 0, None => it empty)]
#[test_case(&[12, 13], 0, None => it empty)]
#[test_case(&[], 0, Some(1) => vec![1])]
#[test_case(&[12], 0, Some(1) => vec![1])]
#[test_case(&[12, 13], 0, Some(1) => vec![1, 13])]
#[test_case(&[12, 13], 1, Some(1) => vec![12, 1])]
#[test_case(&[12, 13], 3, Some(1) => vec![12, 13, 1, 1])]
#[test_case(&[], 3, Some(1) => vec![1, 1, 1, 1])]
#[test_case(&[], 3, None => it empty)]
#[test_case(&[12, 13, 14], 1, Some(1) => vec![12, 1, 14])]
#[test_case(&[12, 13, 14], 1, None => vec![12])]

fn test_invert_vec(v: &[u32], index: usize, value: Option<u32>) -> Vec<u32> {
    let mut v = v.to_vec();
    invert_vec(&mut v, &VecState { index, value });
    v
}

#[test_case(&[], 0, None => it empty)]
#[test_case(&[(12, 22)], 12, None => it empty)]
#[test_case(&[(12, 22), (15, 25)], 0, None => vec![(12, 22), (15, 25)])]
#[test_case(&[], 0, Some(1) => vec![(0, 1)])]
#[test_case(&[(12, 22)], 12, Some(1) => vec![(12, 1)])]
#[test_case(&[(12, 22), (13, 23)], 12, Some(1) => vec![(12, 1), (13, 23)])]
#[test_case(&[(12, 22), (13, 23)], 13, Some(1) => vec![(12, 22), (13, 1)])]
#[test_case(&[(12, 22), (13, 23)], 24, Some(1) => vec![(12, 22), (13, 23), (24, 1)])]
#[test_case(&[], 3, Some(1) => vec![(3, 1)])]
#[test_case(&[], 3, None => it empty)]
#[test_case(&[(12, 22), (13, 23), (14, 24)], 13, Some(1) => vec![(12, 22), (13, 1), (14, 24)])]
#[test_case(&[(12, 22), (13, 23), (14, 24)], 13, None => vec![(12, 22), (14, 24)])]
fn test_invert_map(v: &[(u32, u32)], key: u32, value: Option<u32>) -> Vec<(u32, u32)> {
    let mut v = v.iter().copied().collect();
    invert_map(&mut v, &MapState { key, value });
    let mut v: Vec<_> = v.into_iter().collect();
    v.sort_unstable_by_key(|(k, _)| *k);
    v
}

#[test]
fn reset_vm_memory_grow_stack() {
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    let desired = latest.clone();
    latest.memory_mut().grow_stack(132).unwrap();
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    assert_ne!(latest, desired);
    latest.reset_vm_state(&diff);
    assert_eq!(latest, desired);
}

#[test]
fn reset_vm_memory_grow_heap() {
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    let desired = latest.clone();
    let sp = 0;
    let mut hp = MEM_SIZE as u64;
    latest
        .memory_mut()
        .grow_heap_by(Reg::<SP>::new(&sp), RegMut::<HP>::new(&mut hp), 132)
        .unwrap();
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    assert_ne!(latest, desired);
    latest.reset_vm_state(&diff);
    assert_eq!(latest, desired);
}

#[test]
fn reset_vm_memory_range_write_stack() {
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    latest.memory_mut().grow_stack(132).unwrap();
    let desired = latest.clone();
    latest.memory_mut()[100..132].copy_from_slice(&[1u8; 32]);
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    assert_ne!(latest, desired);
    latest.reset_vm_state(&diff);
    assert_eq!(latest, desired);
}

#[test]
fn reset_vm_memory_range_write_heap() {
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    let sp = 0;
    let mut hp = MEM_SIZE as u64;
    latest
        .memory_mut()
        .grow_heap_by(Reg::<SP>::new(&sp), RegMut::<HP>::new(&mut hp), 132)
        .unwrap();
    let desired = latest.clone();
    latest.memory_mut()[MEM_SIZE - 32..MEM_SIZE].copy_from_slice(&[1u8; 32]);
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    assert_ne!(latest, desired);
    latest.reset_vm_state(&diff);
    assert_eq!(latest, desired);
}

#[test]
fn reset_vm_txns() {
    use fuel_tx::field::Outputs;
    let desired = Interpreter::<_, _, Script>::with_memory_storage();
    let mut latest = Interpreter::<_, _, Script>::with_memory_storage();
    latest
        .tx
        .outputs_mut()
        .push(fuel_tx::Output::ContractCreated {
            contract_id: Default::default(),
            state_root: Default::default(),
        });
    let diff: Diff<InitialVmState> = latest.rollback_to(&desired).into();
    assert_ne!(desired, latest);
    latest.reset_vm_state(&diff);
    assert_eq!(desired, latest);
}
