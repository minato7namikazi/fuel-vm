use crate::{
    binary::{
        Node,
        Primitive,
        empty_sum,
        in_memory::NodesTable,
    },
    common::{
        Bytes32,
        Position,
        ProofSet,
        StorageMap,
    },
    storage::{
        Mappable,
        StorageInspect,
        StorageInspectInfallible,
        StorageMutate,
        StorageMutateInfallible,
    },
};

use alloc::vec::Vec;
use core::{
    convert::Infallible,
    marker::PhantomData,
};

use super::root_calculator::{
    MerkleRootCalculator,
    NodeStackPushError,
};

#[derive(Debug, Clone, derive_more::Display, PartialEq, Eq)]
pub enum MerkleTreeError<StorageError> {
    #[display(fmt = "proof index {_0} is not valid")]
    InvalidProofIndex(u64),

    #[display(fmt = "cannot load node with key {_0}; the key is not found in storage")]
    LoadError(u64),

    #[display(fmt = "{}", _0)]
    StorageError(StorageError),

    #[display(fmt = "the tree is too large")]
    TooLarge,
}

impl<StorageError> From<StorageError> for MerkleTreeError<StorageError> {
    fn from(err: StorageError) -> MerkleTreeError<StorageError> {
        MerkleTreeError::StorageError(err)
    }
}

#[derive(Debug, Clone)]
pub struct MerkleTree<TableType, StorageType> {
    storage: StorageType,
    nodes: MerkleRootCalculator,
    leaves_count: u64,
    phantom_table: PhantomData<TableType>,
}

impl<TableType, StorageType> MerkleTree<TableType, StorageType> {
    pub const fn empty_root() -> &'static Bytes32 {
        empty_sum()
    }

    pub fn root(&self) -> Bytes32 {
        let mut scratch_storage = StorageMap::<NodesTable>::new();
        let root_node = self
            .root_node::<Infallible>(&mut scratch_storage)
            .expect("The type doesn't allow constructing invalid trees.");
        match root_node {
            None => *Self::empty_root(),
            Some(ref node) => *node.hash(),
        }
    }

    pub fn leaves_count(&self) -> u64 {
        self.leaves_count
    }

    /// The root node is generated by joining all MMR peaks, where a peak is
    /// defined as the head of a balanced subtree. A tree can be composed of a
    /// single balanced subtree, in which case the tree is itself balanced, or
    /// several balanced subtrees, in which case the tree is imbalanced. Only
    /// nodes at the head of a balanced tree are persisted in storage; any node,
    /// including the root node, whose child is an imbalanced child subtree will
    /// not be saved in persistent storage. This is because node data for such
    /// nodes is liable to change as more leaves are pushed to the tree.
    /// Instead, intermediate nodes must be held in a temporary storage space.
    ///
    /// When calling `root_node`, callees must pass a mutable reference to a
    /// temporary storage space that will be used to hold any intermediate nodes
    /// that are created during root node calculation. At the end of the method
    /// call, this temporary storage space will contain all intermediate nodes
    /// not held in persistent storage, and these nodes will be available to the
    /// callee.
    ///
    /// Returns `None` if the tree is empty, and the root node otherwise.
    fn root_node<E>(
        &self,
        scratch_storage: &mut StorageMap<NodesTable>,
    ) -> Result<Option<Node>, MerkleTreeError<E>> {
        let mut nodes = self.nodes.stack().iter().rev();
        let Some(mut head) = nodes.next().cloned() else {
            return Ok(None); // Empty tree
        };

        for node in nodes {
            let parent = node
                .position()
                .parent()
                .map_err(|_| MerkleTreeError::TooLarge)?;
            head = Node::create_node(parent, node, &head);
            StorageMutateInfallible::insert(
                scratch_storage,
                &head.key(),
                &(&head).into(),
            );
        }

        Ok(Some(head))
    }
}

impl<TableType, StorageType, StorageError> MerkleTree<TableType, StorageType>
where
    TableType: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
    StorageType: StorageInspect<TableType, Error = StorageError>,
{
    pub fn new(storage: StorageType) -> Self {
        Self {
            storage,
            nodes: MerkleRootCalculator::new(),
            leaves_count: 0,
            phantom_table: Default::default(),
        }
    }

    /// A binary Merkle tree can be built from a collection of Merkle Mountain
    /// Range (MMR) peaks. The MMR structure can be accurately defined by the
    /// number of leaves in the leaf row.
    ///
    /// Consider a binary Merkle tree with seven leaves, producing the following
    /// MMR structure:
    ///
    /// ```text
    ///       03
    ///      /  \
    ///     /    \
    ///   01      05      09
    ///  /  \    /  \    /  \
    /// 00  02  04  06  08  10  12
    /// ```
    ///
    /// We observe that the tree has three peaks at positions `03`, `09`, and
    /// `12`. These peak positions are recorded in the order that they appear,
    /// reading left to right in the tree structure, and only descend in height.
    /// These peak positions communicate everything needed to determine the
    /// remaining internal nodes building upwards to the root position:
    ///
    /// ```text
    ///            07
    ///           /  \
    ///          /    \
    ///         /      \
    ///        /        \
    ///       /          \
    ///      /            \
    ///    03              11
    ///   /  \            /  \
    /// ...  ...         /    \
    ///                09      \
    ///               /  \      \
    ///             ...  ...    12
    /// ```
    ///
    /// No additional intermediate nodes or leaves are required to calculate
    /// the root position.
    ///
    /// The positions of the MMR peaks can be deterministically calculated as a
    /// function of `n + 1` where `n` is the number of leaves in the tree. By
    /// appending an additional leaf node to the tree, we generate a new tree
    /// structure with additional internal nodes (N.B.: this may also change the
    /// root position if the tree is already balanced).
    ///
    /// In our example, we add an additional leaf at leaf index `7` (in-order
    /// index `14`):
    ///
    /// ```text
    ///            07
    ///           /  \
    ///          /    \
    ///         /      \
    ///        /        \
    ///       /          \
    ///      /            \
    ///    03              11
    ///   /  \            /  \
    /// ...  ...         /    \
    ///                09      13
    ///               /  \    /  \
    ///             ...  ... 12  14
    /// ```
    ///
    /// We observe that the path from the root position to our new leaf position
    /// yields a set of side positions that includes our original peak
    /// positions (see [Path Iterator](crate::common::path_iterator::PathIter)):
    ///
    /// | Path position | Side position |
    /// |---------------|---------------|
    /// |            07 |            07 |
    /// |            11 |            03 |
    /// |            13 |            09 |
    /// |            14 |            12 |
    ///
    /// By excluding the root position `07`, we have established the set of
    /// side positions `03`, `09`, and `12`, matching our set of MMR peaks.
    pub fn load(
        storage: StorageType,
        leaves_count: u64,
    ) -> Result<Self, MerkleTreeError<StorageError>> {
        let peaks = peak_positions(leaves_count).ok_or(MerkleTreeError::TooLarge)?;
        let mut nodes = Vec::with_capacity(peaks.len());
        for peak in peaks.iter() {
            let key = peak.in_order_index();
            let node = storage
                .get(&key)?
                .ok_or(MerkleTreeError::LoadError(key))?
                .into_owned()
                .into();
            nodes.push(node);
        }

        Ok(Self {
            storage,
            nodes: MerkleRootCalculator::new_with_stack(nodes),
            leaves_count,
            phantom_table: Default::default(),
        })
    }

    pub fn prove(
        &self,
        proof_index: u64,
    ) -> Result<(Bytes32, ProofSet), MerkleTreeError<StorageError>> {
        if proof_index >= self.leaves_count {
            return Err(MerkleTreeError::InvalidProofIndex(proof_index))
        }

        let root_position = root_position(self.leaves_count)
            .expect("This tree is too large, but push should have prevented this");
        let leaf_position = Position::from_leaf_index(proof_index)
            .expect("leaves_count is valid, and this is less than leaves_count");
        let (_, mut side_positions): (Vec<_>, Vec<_>) = root_position
            .path(&leaf_position, self.leaves_count)
            .iter()
            .unzip();
        side_positions.reverse(); // Reorder side positions from leaf to root.
        side_positions.pop(); // The last side position is the root; remove it.

        // Allocate scratch storage to store temporary nodes when building the
        // root.
        let mut scratch_storage = StorageMap::<NodesTable>::new();
        let root_node = self
            .root_node(&mut scratch_storage)?
            .expect("Root node must be present, as leaves_count is nonzero");

        // Get side nodes. First, we check the scratch storage. If the side node
        // is not found in scratch storage, we then check main storage. Finally,
        // if the side node is not found in main storage, we exit with a load
        // error.
        let mut proof_set = ProofSet::new();
        for side_position in side_positions {
            let key = side_position.in_order_index();
            let primitive = StorageInspectInfallible::get(&scratch_storage, &key)
                .or(StorageInspect::get(&self.storage, &key)?)
                .ok_or(MerkleTreeError::LoadError(key))?
                .into_owned();
            let node = Node::from(primitive);
            proof_set.push(*node.hash());
        }

        let root = *root_node.hash();
        Ok((root, proof_set))
    }

    pub fn reset(&mut self) {
        self.nodes.clear();
    }
}

impl<TableType, StorageType, StorageError> MerkleTree<TableType, StorageType>
where
    TableType: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
    StorageType: StorageMutate<TableType, Error = StorageError>,
{
    /// Adds a new leaf node to the tree.
    /// # WARNING
    /// This code might modify the storage, and then return an error.
    /// TODO: fix this issue
    pub fn push(&mut self, data: &[u8]) -> Result<(), MerkleTreeError<StorageError>> {
        let new_node = Node::create_leaf(self.leaves_count, data)
            .ok_or(MerkleTreeError::TooLarge)?;

        // u64 cannot overflow, as memory is finite
        #[allow(clippy::arithmetic_side_effects)]
        {
            self.leaves_count += 1;
        }

        self.nodes
            .push_with_callback(new_node, |node| {
                self.storage
                    .insert(&node.key(), &node.into())
                    .map_err(MerkleTreeError::StorageError)
                    .map(|_| ())
            })
            .map_err(|err| match err {
                NodeStackPushError::Callback(err) => err,
                NodeStackPushError::TooLarge => MerkleTreeError::TooLarge,
            })
    }
}

/// Calculcate root position from leaf count.
/// Returns `None` if the tree is too large.
fn root_position(leaves_count: u64) -> Option<Position> {
    // The root position of a tree will always have an in-order index equal
    // to N' - 1, where N is the leaves count and N' is N rounded (or equal)
    // to the next power of 2.
    #[allow(clippy::arithmetic_side_effects)] // next_power_of_two() > 0
    Some(Position::from_in_order_index(
        leaves_count.checked_add(1)?.next_power_of_two() - 1,
    ))
}

/// Calculcate peak positons for given leaf count.
/// Returns `None` if the tree is too large.
fn peak_positions(leaves_count: u64) -> Option<Vec<Position>> {
    let leaf_position = Position::from_leaf_index(leaves_count)?;
    let root_position = root_position(leaves_count)?;

    // Checked by root_position
    #[allow(clippy::arithmetic_side_effects)]
    let next_leaves_count = leaves_count + 1;

    let mut peaks_itr = root_position.path(&leaf_position, next_leaves_count).iter();
    peaks_itr.next(); // Omit the root

    let (_, peaks): (Vec<_>, Vec<_>) = peaks_itr.unzip();

    Some(peaks)
}

#[cfg(test)]
mod test {
    use super::{
        MerkleTree,
        MerkleTreeError,
    };
    use crate::{
        binary::{
            Node,
            Primitive,
            empty_sum,
            leaf_sum,
            node_sum,
        },
        common::StorageMap,
    };
    use fuel_merkle_test_helpers::TEST_DATA;
    use fuel_storage::{
        Mappable,
        StorageInspect,
        StorageMutate,
    };

    use alloc::vec::Vec;

    #[derive(Debug)]
    struct TestTable;

    impl Mappable for TestTable {
        type Key = Self::OwnedKey;
        type OwnedKey = u64;
        type OwnedValue = Primitive;
        type Value = Self::OwnedValue;
    }

    #[test]
    fn test_push_builds_internal_tree_structure() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..7]; // 7 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        //               07
        //              /  \
        //             /    \
        //            /      \
        //           /        \
        //          /          \
        //         /            \
        //       03              11
        //      /  \            /  \
        //     /    \          /    \
        //   01      05      09      \
        //  /  \    /  \    /  \      \
        // 00  02  04  06  08  10     12
        // 00  01  02  03  04  05     06

        let leaf_0 = leaf_sum(data[0]);
        let leaf_1 = leaf_sum(data[1]);
        let leaf_2 = leaf_sum(data[2]);
        let leaf_3 = leaf_sum(data[3]);
        let leaf_4 = leaf_sum(data[4]);
        let leaf_5 = leaf_sum(data[5]);
        let leaf_6 = leaf_sum(data[6]);
        let node_1 = node_sum(&leaf_0, &leaf_1);
        let node_5 = node_sum(&leaf_2, &leaf_3);
        let node_3 = node_sum(&node_1, &node_5);
        let node_9 = node_sum(&leaf_4, &leaf_5);

        let s_leaf_0 = storage_map.get(&0).unwrap().unwrap();
        let s_leaf_1 = storage_map.get(&2).unwrap().unwrap();
        let s_leaf_2 = storage_map.get(&4).unwrap().unwrap();
        let s_leaf_3 = storage_map.get(&6).unwrap().unwrap();
        let s_leaf_4 = storage_map.get(&8).unwrap().unwrap();
        let s_leaf_5 = storage_map.get(&10).unwrap().unwrap();
        let s_leaf_6 = storage_map.get(&12).unwrap().unwrap();
        let s_node_1 = storage_map.get(&1).unwrap().unwrap();
        let s_node_5 = storage_map.get(&5).unwrap().unwrap();
        let s_node_9 = storage_map.get(&9).unwrap().unwrap();
        let s_node_3 = storage_map.get(&3).unwrap().unwrap();

        assert_eq!(*Node::from(s_leaf_0.into_owned()).hash(), leaf_0);
        assert_eq!(*Node::from(s_leaf_1.into_owned()).hash(), leaf_1);
        assert_eq!(*Node::from(s_leaf_2.into_owned()).hash(), leaf_2);
        assert_eq!(*Node::from(s_leaf_3.into_owned()).hash(), leaf_3);
        assert_eq!(*Node::from(s_leaf_4.into_owned()).hash(), leaf_4);
        assert_eq!(*Node::from(s_leaf_5.into_owned()).hash(), leaf_5);
        assert_eq!(*Node::from(s_leaf_6.into_owned()).hash(), leaf_6);
        assert_eq!(*Node::from(s_node_1.into_owned()).hash(), node_1);
        assert_eq!(*Node::from(s_node_5.into_owned()).hash(), node_5);
        assert_eq!(*Node::from(s_node_9.into_owned()).hash(), node_9);
        assert_eq!(*Node::from(s_node_3.into_owned()).hash(), node_3);
    }

    #[test]
    fn load_returns_a_valid_tree() {
        const LEAVES_COUNT: u64 = 2u64.pow(16) - 1;

        let mut storage_map = StorageMap::<TestTable>::new();

        let expected_root = {
            let mut tree = MerkleTree::new(&mut storage_map);
            let data = (0u64..LEAVES_COUNT)
                .map(|i| i.to_be_bytes())
                .collect::<Vec<_>>();
            for datum in data.iter() {
                let _ = tree.push(datum);
            }
            tree.root()
        };

        let root = {
            let tree = MerkleTree::load(&mut storage_map, LEAVES_COUNT).unwrap();
            tree.root()
        };

        assert_eq!(expected_root, root);
    }

    #[test]
    fn load_returns_empty_tree_for_0_leaves() {
        const LEAVES_COUNT: u64 = 0;

        let expected_root = *MerkleTree::<(), ()>::empty_root();

        let root = {
            let mut storage_map = StorageMap::<TestTable>::new();
            let tree = MerkleTree::load(&mut storage_map, LEAVES_COUNT).unwrap();
            tree.root()
        };

        assert_eq!(expected_root, root);
    }

    #[test]
    fn load_returns_a_load_error_if_the_storage_is_not_valid_for_the_leaves_count() {
        const LEAVES_COUNT: u64 = 5;

        let mut storage_map = StorageMap::<TestTable>::new();

        let mut tree = MerkleTree::new(&mut storage_map);
        let data = (0u64..LEAVES_COUNT)
            .map(|i| i.to_be_bytes())
            .collect::<Vec<_>>();
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        let err = MerkleTree::load(&mut storage_map, LEAVES_COUNT * 2)
            .expect_err("Expected load() to return Error; got Ok");
        assert!(matches!(err, MerkleTreeError::LoadError(_)));
    }

    #[test]
    fn root_returns_the_empty_root_for_0_leaves() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let tree = MerkleTree::new(&mut storage_map);

        let root = tree.root();
        assert_eq!(root, empty_sum().clone());
    }

    #[test]
    fn root_returns_the_merkle_root_for_1_leaf() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..1]; // 1 leaf
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        let leaf_0 = leaf_sum(data[0]);

        let root = tree.root();
        assert_eq!(root, leaf_0);
    }

    #[test]
    fn root_returns_the_merkle_root_for_7_leaves() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..7]; // 7 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        //               07
        //              /  \
        //             /    \
        //            /      \
        //           /        \
        //          /          \
        //         /            \
        //       03              11
        //      /  \            /  \
        //     /    \          /    \
        //   01      05      09      \
        //  /  \    /  \    /  \      \
        // 00  02  04  06  08  10     12
        // 00  01  02  03  04  05     06

        let leaf_0 = leaf_sum(data[0]);
        let leaf_1 = leaf_sum(data[1]);
        let leaf_2 = leaf_sum(data[2]);
        let leaf_3 = leaf_sum(data[3]);
        let leaf_4 = leaf_sum(data[4]);
        let leaf_5 = leaf_sum(data[5]);
        let leaf_6 = leaf_sum(data[6]);

        let node_1 = node_sum(&leaf_0, &leaf_1);
        let node_5 = node_sum(&leaf_2, &leaf_3);
        let node_3 = node_sum(&node_1, &node_5);
        let node_9 = node_sum(&leaf_4, &leaf_5);
        let node_11 = node_sum(&node_9, &leaf_6);
        let node_7 = node_sum(&node_3, &node_11);

        let root = tree.root();
        assert_eq!(root, node_7);
    }

    #[test]
    fn prove_returns_invalid_proof_index_error_for_0_leaves() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let tree = MerkleTree::new(&mut storage_map);

        let err = tree
            .prove(0)
            .expect_err("Expected prove() to return Error; got Ok");
        assert!(matches!(err, MerkleTreeError::InvalidProofIndex(0)));
    }

    #[test]
    fn prove_returns_invalid_proof_index_error_when_index_is_greater_than_number_of_leaves()
     {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..5]; // 5 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        let err = tree
            .prove(10)
            .expect_err("Expected prove() to return Error; got Ok");
        assert!(matches!(err, MerkleTreeError::InvalidProofIndex(10)))
    }

    #[test]
    fn prove_returns_the_merkle_root_and_proof_set_for_1_leaf() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..1]; // 1 leaf
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        let leaf_0 = leaf_sum(data[0]);

        {
            let (root, proof_set) = tree.prove(0).unwrap();
            assert_eq!(root, leaf_0);
            assert!(proof_set.is_empty());
        }
    }

    #[test]
    fn prove_returns_the_merkle_root_and_proof_set_for_4_leaves() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..4]; // 4 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        //       03
        //      /  \
        //     /    \
        //   01      05
        //  /  \    /  \
        // 00  02  04  06
        // 00  01  02  03

        let leaf_0 = leaf_sum(data[0]);
        let leaf_1 = leaf_sum(data[1]);
        let leaf_2 = leaf_sum(data[2]);
        let leaf_3 = leaf_sum(data[3]);

        let node_1 = node_sum(&leaf_0, &leaf_1);
        let node_5 = node_sum(&leaf_2, &leaf_3);
        let node_3 = node_sum(&node_1, &node_5);

        {
            let (root, proof_set) = tree.prove(0).unwrap();
            assert_eq!(root, node_3);
            assert_eq!(proof_set[0], leaf_1);
            assert_eq!(proof_set[1], node_5);
        }
        {
            let (root, proof_set) = tree.prove(1).unwrap();
            assert_eq!(root, node_3);
            assert_eq!(proof_set[0], leaf_0);
            assert_eq!(proof_set[1], node_5);
        }
        {
            let (root, proof_set) = tree.prove(2).unwrap();
            assert_eq!(root, node_3);
            assert_eq!(proof_set[0], leaf_3);
            assert_eq!(proof_set[1], node_1);
        }
        {
            let (root, proof_set) = tree.prove(3).unwrap();
            assert_eq!(root, node_3);
            assert_eq!(proof_set[0], leaf_2);
            assert_eq!(proof_set[1], node_1);
        }
    }

    #[test]
    fn prove_returns_the_merkle_root_and_proof_set_for_5_leaves() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..5]; // 5 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        //          07
        //          /\
        //         /  \
        //       03    \
        //      /  \    \
        //     /    \    \
        //   01      05   \
        //  /  \    /  \   \
        // 00  02  04  06  08
        // 00  01  02  03  04

        let leaf_0 = leaf_sum(data[0]);
        let leaf_1 = leaf_sum(data[1]);
        let leaf_2 = leaf_sum(data[2]);
        let leaf_3 = leaf_sum(data[3]);
        let leaf_4 = leaf_sum(data[4]);

        let node_1 = node_sum(&leaf_0, &leaf_1);
        let node_5 = node_sum(&leaf_2, &leaf_3);
        let node_3 = node_sum(&node_1, &node_5);
        let node_7 = node_sum(&node_3, &leaf_4);

        {
            let (root, proof_set) = tree.prove(0).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_1);
            assert_eq!(proof_set[1], node_5);
            assert_eq!(proof_set[2], leaf_4);
        }
        {
            let (root, proof_set) = tree.prove(1).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_0);
            assert_eq!(proof_set[1], node_5);
            assert_eq!(proof_set[2], leaf_4);
        }
        {
            let (root, proof_set) = tree.prove(2).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_3);
            assert_eq!(proof_set[1], node_1);
            assert_eq!(proof_set[2], leaf_4);
        }
        {
            let (root, proof_set) = tree.prove(3).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_2);
            assert_eq!(proof_set[1], node_1);
            assert_eq!(proof_set[2], leaf_4);
        }
        {
            let (root, proof_set) = tree.prove(4).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], node_3);
        }
    }

    #[test]
    fn prove_returns_the_merkle_root_and_proof_set_for_7_leaves() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..7]; // 7 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        //               07
        //              /  \
        //             /    \
        //            /      \
        //           /        \
        //          /          \
        //         /            \
        //       03              11
        //      /  \            /  \
        //     /    \          /    \
        //   01      05      09      \
        //  /  \    /  \    /  \      \
        // 00  02  04  06  08  10     12
        // 00  01  02  03  04  05     06

        let leaf_0 = leaf_sum(data[0]);
        let leaf_1 = leaf_sum(data[1]);
        let leaf_2 = leaf_sum(data[2]);
        let leaf_3 = leaf_sum(data[3]);
        let leaf_4 = leaf_sum(data[4]);
        let leaf_5 = leaf_sum(data[5]);
        let leaf_6 = leaf_sum(data[6]);

        let node_1 = node_sum(&leaf_0, &leaf_1);
        let node_5 = node_sum(&leaf_2, &leaf_3);
        let node_3 = node_sum(&node_1, &node_5);
        let node_9 = node_sum(&leaf_4, &leaf_5);
        let node_11 = node_sum(&node_9, &leaf_6);
        let node_7 = node_sum(&node_3, &node_11);

        {
            let (root, proof_set) = tree.prove(0).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_1);
            assert_eq!(proof_set[1], node_5);
            assert_eq!(proof_set[2], node_11);
        }
        {
            let (root, proof_set) = tree.prove(1).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_0);
            assert_eq!(proof_set[1], node_5);
            assert_eq!(proof_set[2], node_11);
        }
        {
            let (root, proof_set) = tree.prove(2).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_3);
            assert_eq!(proof_set[1], node_1);
            assert_eq!(proof_set[2], node_11);
        }
        {
            let (root, proof_set) = tree.prove(3).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_2);
            assert_eq!(proof_set[1], node_1);
            assert_eq!(proof_set[2], node_11);
        }
        {
            let (root, proof_set) = tree.prove(4).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_5);
            assert_eq!(proof_set[1], leaf_6);
            assert_eq!(proof_set[2], node_3);
        }
        {
            let (root, proof_set) = tree.prove(5).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], leaf_4);
            assert_eq!(proof_set[1], leaf_6);
            assert_eq!(proof_set[2], node_3);
        }
        {
            let (root, proof_set) = tree.prove(6).unwrap();
            assert_eq!(root, node_7);
            assert_eq!(proof_set[0], node_9);
            assert_eq!(proof_set[1], node_3);
        }
    }

    #[test]
    fn reset_reverts_tree_to_empty_state() {
        let mut storage_map = StorageMap::<TestTable>::new();
        let mut tree = MerkleTree::new(&mut storage_map);

        let data = &TEST_DATA[0..4]; // 4 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        tree.reset();

        let root = tree.root();
        let expected_root = *MerkleTree::<(), ()>::empty_root();
        assert_eq!(root, expected_root);

        let data = &TEST_DATA[0..4]; // 4 leaves
        for datum in data.iter() {
            let _ = tree.push(datum);
        }

        let leaf_0 = leaf_sum(data[0]);
        let leaf_1 = leaf_sum(data[1]);
        let leaf_2 = leaf_sum(data[2]);
        let leaf_3 = leaf_sum(data[3]);

        let node_1 = node_sum(&leaf_0, &leaf_1);
        let node_5 = node_sum(&leaf_2, &leaf_3);
        let node_3 = node_sum(&node_1, &node_5);

        let root = tree.root();
        let expected_root = node_3;
        assert_eq!(root, expected_root);
    }

    #[test]
    fn load_overflows() {
        // Given
        let storage_map = StorageMap::<TestTable>::new();
        const LEAVES_COUNT: u64 = u64::MAX;

        // When
        let result = MerkleTree::load(storage_map, LEAVES_COUNT).map(|_| ());

        // Then
        assert_eq!(result, Err(MerkleTreeError::TooLarge));
    }

    #[test]
    fn push_overflows() {
        // Given
        let mut storage_map = StorageMap::<TestTable>::new();
        const LEAVES_COUNT: u64 = u64::MAX / 2;
        loop {
            let result = MerkleTree::load(&mut storage_map, LEAVES_COUNT).map(|_| ());

            if let Err(MerkleTreeError::LoadError(index)) = result {
                storage_map.insert(&index, &Primitive::default()).unwrap();
            } else {
                break;
            }
        }

        // When
        let mut tree = MerkleTree::load(storage_map, LEAVES_COUNT)
            .expect("Expected `load()` to succeed");
        let _ = tree.push(&[]);
        let result = tree.push(&[]);

        // Then
        assert_eq!(result, Err(MerkleTreeError::TooLarge));
    }
}
