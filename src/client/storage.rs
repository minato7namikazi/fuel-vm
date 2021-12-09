use crate::contract::Contract;
use crate::crypto;
use crate::storage::InterpreterStorage;

use fuel_storage::{MerkleRoot, MerkleStorage, Storage};
use fuel_tx::crypto::Hasher;
use fuel_types::{Address, Bytes32, Color, ContractId, Salt, Word};
use itertools::Itertools;

use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct MemoryStorageInner {
    contracts: HashMap<ContractId, Contract>,
    balances: HashMap<(ContractId, Color), Word>,
    contract_state: HashMap<(ContractId, Bytes32), Bytes32>,
    contract_code_root: HashMap<ContractId, (Salt, Bytes32)>,
}

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    block_height: u32,
    coinbase: Address,
    memory: MemoryStorageInner,
    transacted: MemoryStorageInner,
    persisted: MemoryStorageInner,
}

impl MemoryStorage {
    pub fn new(block_height: u32, coinbase: Address) -> Self {
        Self {
            block_height,
            coinbase,
            memory: Default::default(),
            transacted: Default::default(),
            persisted: Default::default(),
        }
    }

    pub fn contract_state(&self, contract: &ContractId, key: &Bytes32) -> Cow<'_, Bytes32> {
        const DEFAULT_STATE: Bytes32 = Bytes32::zeroed();

        <Self as MerkleStorage<ContractId, Bytes32, Bytes32>>::get(self, contract, key)
            .expect("Infallible")
            .unwrap_or(Cow::Borrowed(&DEFAULT_STATE))
    }

    pub fn commit(&mut self) {
        self.transacted = self.memory.clone();
    }

    pub fn revert(&mut self) {
        self.memory = self.transacted.clone();
    }

    pub fn rollback(&mut self) {
        self.memory = self.persisted.clone();
        self.transacted = self.persisted.clone();
    }

    pub fn persist(&mut self) {
        self.memory = self.transacted.clone();
        self.persisted = self.transacted.clone();
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        let block_height = 1;
        let coinbase = Address::from(*Hasher::hash(b"coinbase"));

        Self::new(block_height, coinbase)
    }
}

impl Storage<ContractId, Contract> for MemoryStorage {
    type Error = Infallible;

    fn insert(&mut self, key: &ContractId, value: &Contract) -> Result<Option<Contract>, Infallible> {
        Ok(self.memory.contracts.insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Infallible> {
        Ok(self.memory.contracts.remove(key))
    }

    fn get(&self, key: &ContractId) -> Result<Option<Cow<'_, Contract>>, Infallible> {
        Ok(self.memory.contracts.get(key).map(Cow::Borrowed))
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Infallible> {
        Ok(self.memory.contracts.contains_key(key))
    }
}

impl Storage<ContractId, (Salt, Bytes32)> for MemoryStorage {
    type Error = Infallible;

    fn insert(&mut self, key: &ContractId, value: &(Salt, Bytes32)) -> Result<Option<(Salt, Bytes32)>, Infallible> {
        Ok(self.memory.contract_code_root.insert(*key, *value))
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<(Salt, Bytes32)>, Infallible> {
        Ok(self.memory.contract_code_root.remove(key))
    }

    fn get(&self, key: &ContractId) -> Result<Option<Cow<'_, (Salt, Bytes32)>>, Infallible> {
        Ok(self.memory.contract_code_root.get(key).map(Cow::Borrowed))
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Infallible> {
        Ok(self.memory.contract_code_root.contains_key(key))
    }
}

impl MerkleStorage<ContractId, Color, Word> for MemoryStorage {
    type Error = Infallible;

    fn insert(&mut self, parent: &ContractId, key: &Color, value: &Word) -> Result<Option<Word>, Infallible> {
        Ok(self.memory.balances.insert((*parent, *key), *value))
    }

    fn get(&self, parent: &ContractId, key: &Color) -> Result<Option<Cow<'_, Word>>, Infallible> {
        Ok(self.memory.balances.get(&(*parent, *key)).copied().map(Cow::Owned))
    }

    fn remove(&mut self, parent: &ContractId, key: &Color) -> Result<Option<Word>, Infallible> {
        Ok(self.memory.balances.remove(&(*parent, *key)))
    }

    fn contains_key(&self, parent: &ContractId, key: &Color) -> Result<bool, Infallible> {
        Ok(self.memory.balances.contains_key(&(*parent, *key)))
    }

    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Infallible> {
        let root = self
            .memory
            .balances
            .iter()
            .filter_map(|((contract, color), balance)| (contract == parent).then(|| (color, balance)))
            .sorted_by_key(|t| t.0)
            .map(|(_, &balance)| balance)
            .map(Word::to_be_bytes);

        Ok(crypto::ephemeral_merkle_root(root).into())
    }
}

impl MerkleStorage<ContractId, Bytes32, Bytes32> for MemoryStorage {
    type Error = Infallible;

    fn insert(&mut self, parent: &ContractId, key: &Bytes32, value: &Bytes32) -> Result<Option<Bytes32>, Infallible> {
        Ok(self.memory.contract_state.insert((*parent, *key), *value))
    }

    fn get(&self, parent: &ContractId, key: &Bytes32) -> Result<Option<Cow<'_, Bytes32>>, Infallible> {
        Ok(self.memory.contract_state.get(&(*parent, *key)).map(Cow::Borrowed))
    }

    fn remove(&mut self, parent: &ContractId, key: &Bytes32) -> Result<Option<Bytes32>, Infallible> {
        Ok(self.memory.contract_state.remove(&(*parent, *key)))
    }

    fn contains_key(&self, parent: &ContractId, key: &Bytes32) -> Result<bool, Infallible> {
        Ok(self.memory.contract_state.contains_key(&(*parent, *key)))
    }

    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Infallible> {
        let root = self
            .memory
            .contract_state
            .iter()
            .filter_map(|((contract, key), value)| (contract == parent).then(|| (key, value)))
            .sorted_by_key(|t| t.0)
            .map(|(_, value)| value);

        Ok(crypto::ephemeral_merkle_root(root).into())
    }
}

impl InterpreterStorage for MemoryStorage {
    type DataError = Infallible;

    fn block_height(&self) -> Result<u32, Infallible> {
        Ok(self.block_height)
    }

    fn block_hash(&self, block_height: u32) -> Result<Bytes32, Infallible> {
        Ok(Hasher::hash(&block_height.to_be_bytes()))
    }

    fn coinbase(&self) -> Result<Address, Infallible> {
        Ok(self.coinbase)
    }
}