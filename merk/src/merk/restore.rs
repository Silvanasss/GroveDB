// MIT LICENSE
//
// Copyright (c) 2021 Dash Core Group
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Provides `Restorer`, which can create a replica of a Merk instance by
//! receiving chunk proofs.

#[cfg(feature = "full")]
use std::{iter::Peekable, u8};

#[cfg(feature = "full")]
use storage::{Batch, StorageContext};

#[cfg(feature = "full")]
use super::Merk;
#[cfg(feature = "full")]
use crate::{
    error::Error,
    merk::MerkSource,
    proofs::{
        chunk::{verify_leaf, verify_trunk, MIN_TRUNK_HEIGHT},
        tree::{Child, Tree as ProofTree},
        Node, Op,
    },
    tree::{combine_hash, value_hash, Link, RefWalker, Tree},
    CryptoHash,
    Error::{CostsError, EdError, StorageError},
    TreeFeatureType::BasicMerk,
};

#[cfg(feature = "full")]
/// A `Restorer` handles decoding, verifying, and storing chunk proofs to
/// replicate an entire Merk tree. It expects the chunks to be processed in
/// order, retrying the last chunk if verification fails.
pub struct Restorer<S> {
    leaf_hashes: Option<Peekable<std::vec::IntoIter<CryptoHash>>>,
    parent_keys: Option<Peekable<std::vec::IntoIter<Vec<u8>>>>,
    trunk_height: Option<usize>,
    merk: Merk<S>,
    expected_root_hash: CryptoHash,
    combining_value: Option<Vec<u8>>,
}

#[cfg(feature = "full")]
impl<'db, S: StorageContext<'db>> Restorer<S> {
    /// Creates a new `Restorer`, which will initialize a new Merk at the given
    /// file path. The first chunk (the "trunk") will be compared against
    /// `expected_root_hash`, then each subsequent chunk will be compared
    /// against the hashes stored in the trunk, so that the restore process will
    /// never allow malicious peers to send more than a single invalid chunk.
    pub fn new(
        merk: Merk<S>,
        combining_value: Option<Vec<u8>>,
        expected_root_hash: CryptoHash,
    ) -> Self {
        Self {
            expected_root_hash,
            combining_value,
            trunk_height: None,
            merk,
            leaf_hashes: None,
            parent_keys: None,
        }
    }

    /// Verifies a chunk and writes it to the working RocksDB instance. Expects
    /// to be called for each chunk in order. Returns the number of remaining
    /// chunks.
    ///
    /// Once there are no remaining chunks to be processed, `finalize` should
    /// be called.
    pub fn process_chunk(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<usize, Error> {
        match self.leaf_hashes {
            None => self.process_trunk(ops),
            Some(_) => self.process_leaf(ops),
        }
    }

    /// Consumes the `Restorer` and returns the newly-created, fully-populated
    /// Merk instance. This method will return an error if called before
    /// processing all chunks (e.g. `restorer.remaining_chunks()` is not equal
    /// to 0).
    pub fn finalize(mut self) -> Result<Merk<S>, Error> {
        if self.remaining_chunks().unwrap_or(0) != 0 {
            return Err(Error::ChunkRestoringError(
                "Called finalize before all chunks were processed".to_string(),
            ));
        }

        if self.trunk_height.unwrap() >= MIN_TRUNK_HEIGHT {
            self.rewrite_trunk_child_heights()?;
        }

        self.merk.load_base_root().unwrap()?;

        Ok(self.merk)
    }

    /// Returns the number of remaining chunks to be processed. If called before
    /// the first chunk is processed, this method will return `None` since we do
    /// not yet have enough information to know about the number of chunks.
    pub fn remaining_chunks(&self) -> Option<usize> {
        self.leaf_hashes.as_ref().map(|lh| lh.len())
    }

    /// Writes the data contained in `tree` (extracted from a verified chunk
    /// proof) to the RocksDB.
    fn write_chunk(&mut self, tree: ProofTree) -> Result<(), Error> {
        let mut batch = self.merk.storage.new_batch();

        tree.visit_refs(&mut |proof_node| {
            if let Some((mut node, key)) = match &proof_node.node {
                Node::KV(key, value) => Some((
                    Tree::new(key.clone(), value.clone(), None, BasicMerk).unwrap(),
                    key,
                )),
                Node::KVValueHash(key, value, value_hash) => Some((
                    Tree::new_with_value_hash(key.clone(), value.clone(), *value_hash, BasicMerk)
                        .unwrap(),
                    key,
                )),
                Node::KVValueHashFeatureType(key, value, value_hash, feature_type) => Some((
                    Tree::new_with_value_hash(
                        key.clone(),
                        value.clone(),
                        *value_hash,
                        *feature_type,
                    )
                    .unwrap(),
                    key,
                )),
                _ => None,
            } {
                // TODO: encode tree node without cloning key/value
                *node.slot_mut(true) = proof_node.left.as_ref().map(Child::as_link);
                *node.slot_mut(false) = proof_node.right.as_ref().map(Child::as_link);

                let bytes = node.encode();
                batch.put(key, &bytes, None, None).map_err(CostsError)
            } else {
                Ok(())
            }
        })?;

        self.merk
            .storage
            .commit_batch(batch)
            .unwrap()
            .map_err(StorageError)
    }

    /// Verifies the trunk then writes its data to the RocksDB.
    fn process_trunk(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<usize, Error> {
        let (trunk, height) = verify_trunk(ops.into_iter().map(Ok)).unwrap()?;

        let root_hash = if self.combining_value.is_none() {
            trunk.hash().unwrap()
        } else {
            combine_hash(
                value_hash(self.combining_value.as_ref().expect("confirmed exists")).value(),
                &trunk.hash().unwrap(),
            )
            .value
        };

        if root_hash != self.expected_root_hash {
            return Err(Error::ChunkRestoringError(format!(
                "Proof did not match expected hash\n\tExpected: {:?}\n\tActual: {:?}",
                self.expected_root_hash,
                trunk.hash()
            )));
        }

        let root_key = trunk.key().to_vec();

        let trunk_height = height / 2;
        self.trunk_height = Some(trunk_height);

        let chunks_remaining = if trunk_height >= MIN_TRUNK_HEIGHT {
            let leaf_hashes = trunk
                .layer(trunk_height)
                .map(|node| node.hash().unwrap())
                .collect::<Vec<CryptoHash>>()
                .into_iter()
                .peekable();
            self.leaf_hashes = Some(leaf_hashes);

            let parent_keys = trunk
                .layer(trunk_height - 1)
                .map(|node| node.key().to_vec())
                .collect::<Vec<Vec<u8>>>()
                .into_iter()
                .peekable();
            self.parent_keys = Some(parent_keys);
            assert_eq!(
                self.parent_keys.as_ref().unwrap().len(),
                self.leaf_hashes.as_ref().unwrap().len() / 2
            );

            let chunks_remaining = (2_usize).pow(trunk_height as u32);
            assert_eq!(self.remaining_chunks_unchecked(), chunks_remaining);
            chunks_remaining
        } else {
            self.leaf_hashes = Some(vec![].into_iter().peekable());
            self.parent_keys = Some(vec![].into_iter().peekable());
            0
        };

        // note that these writes don't happen atomically, which is fine here
        // because if anything fails during the restore process we will just
        // scrap the whole restore and start over
        self.write_chunk(trunk)?;
        self.merk.set_base_root_key(Some(root_key)).unwrap()?;

        Ok(chunks_remaining)
    }

    /// Verifies a leaf chunk then writes it to the RocksDB. This needs to be
    /// called in order, retrying the last chunk for any failed verifications.
    fn process_leaf(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<usize, Error> {
        let leaf_hashes = self.leaf_hashes.as_mut().unwrap();
        let leaf_hash = leaf_hashes
            .peek()
            .expect("Received more chunks than expected");

        let leaf = verify_leaf(ops.into_iter().map(Ok), *leaf_hash).unwrap()?;
        self.rewrite_parent_link(&leaf)?;
        self.write_chunk(leaf)?;

        let leaf_hashes = self.leaf_hashes.as_mut().unwrap();
        leaf_hashes.next();

        Ok(self.remaining_chunks_unchecked())
    }

    /// The parent of the root node of the leaf does not know the key of its
    /// children when it is first written. Now that we have verified this leaf,
    /// we can write the key into the parent node's entry. Note that this does
    /// not need to recalcuate hashes since it already had the child hash.
    fn rewrite_parent_link(&mut self, leaf: &ProofTree) -> Result<(), Error> {
        let parent_keys = self.parent_keys.as_mut().unwrap();
        let parent_key = parent_keys.peek().unwrap().clone();
        let mut parent = crate::merk::fetch_node(&self.merk.storage, parent_key.as_slice())?
            .expect("Could not find parent of leaf chunk");

        let is_left_child = self.remaining_chunks_unchecked() % 2 == 0;
        if let Some(Link::Reference { ref mut key, .. }) = parent.link_mut(is_left_child) {
            *key = leaf.key().to_vec();
        } else {
            panic!("Expected parent links to be type Link::Reference");
        };

        let parent_bytes = parent.encode();
        self.merk
            .storage
            .put(parent_key, &parent_bytes, None, None)
            .unwrap()
            .map_err(StorageError)?;

        if !is_left_child {
            let parent_keys = self.parent_keys.as_mut().unwrap();
            parent_keys.next();
        }

        Ok(())
    }

    fn rewrite_trunk_child_heights(&mut self) -> Result<(), Error> {
        fn recurse<'s, 'db, S: StorageContext<'db>>(
            mut node: RefWalker<MerkSource<'s, S>>,
            remaining_depth: usize,
            batch: &mut <S as StorageContext<'db>>::Batch,
        ) -> Result<(u8, u8), Error> {
            if remaining_depth == 0 {
                return Ok(node.tree().child_heights());
            }

            let mut cloned_node =
                Tree::decode(node.tree().key().to_vec(), node.tree().encode().as_slice())
                    .map_err(EdError)?;

            let left_child = node.walk(true).unwrap()?.unwrap();
            let left_child_heights = recurse(left_child, remaining_depth - 1, batch)?;
            let left_height = left_child_heights.0.max(left_child_heights.1) + 1;
            *cloned_node.link_mut(true).unwrap().child_heights_mut() = left_child_heights;

            let right_child = node.walk(false).unwrap()?.unwrap();
            let right_child_heights = recurse(right_child, remaining_depth - 1, batch)?;
            let right_height = right_child_heights.0.max(right_child_heights.1) + 1;
            *cloned_node.link_mut(false).unwrap().child_heights_mut() = right_child_heights;

            let bytes = cloned_node.encode();
            batch
                .put(node.tree().key(), &bytes, None, None)
                .map_err(CostsError)?;

            Ok((left_height, right_height))
        }

        self.merk.load_base_root().unwrap()?;

        let mut batch = self.merk.storage.new_batch();

        let depth = self.trunk_height.unwrap();
        self.merk.use_tree_mut(|maybe_tree| {
            let tree = maybe_tree.unwrap();
            let walker = RefWalker::new(tree, self.merk.source());
            recurse(walker, depth, &mut batch)
        })?;

        self.merk
            .storage
            .commit_batch(batch)
            .unwrap()
            .map_err(StorageError)
    }

    /// Returns the number of remaining chunks to be processed. This method will
    /// panic if called before processing the first chunk (since that chunk
    /// gives us the information to know how many chunks to expect).
    pub fn remaining_chunks_unchecked(&self) -> usize {
        self.leaf_hashes.as_ref().unwrap().len()
    }
}

#[cfg(feature = "full")]
impl<'db, S: StorageContext<'db>> Merk<S> {
    /// Creates a new `Restorer`, which can be used to verify chunk proofs to
    /// replicate an entire Merk tree. A new Merk instance will be initialized
    /// by creating a RocksDB at `path`.
    pub fn restore(merk: Merk<S>, expected_root_hash: CryptoHash) -> Restorer<S> {
        Restorer::new(merk, None, expected_root_hash)
    }
}

#[cfg(feature = "full")]
impl ProofTree {
    fn child_heights(&self) -> (u8, u8) {
        (
            self.left.as_ref().map_or(0, |c| c.tree.height as u8),
            self.right.as_ref().map_or(0, |c| c.tree.height as u8),
        )
    }
}

#[cfg(feature = "full")]
impl Child {
    fn as_link(&self) -> Link {
        let key = match &self.tree.node {
            Node::KV(key, _)
            | Node::KVValueHash(key, ..)
            | Node::KVValueHashFeatureType(key, ..) => key.as_slice(),
            // for the connection between the trunk and leaf chunks, we don't
            // have the child key so we must first write in an empty one. once
            // the leaf gets verified, we can write in this key to its parent
            _ => &[],
        };

        Link::Reference {
            hash: self.hash,
            sum: None,
            child_heights: self.tree.child_heights(),
            key: key.to_vec(),
        }
    }
}

#[cfg(feature = "full")]
#[cfg(test)]
mod tests {
    use std::iter::empty;

    use storage::{
        rocksdb_storage::{test_utils::TempStorage, PrefixedRocksDbStorageContext},
        RawIterator, Storage,
    };

    use super::*;
    use crate::{test_utils::*, tree::Op, MerkBatch};

    fn restore_test(batches: &[&MerkBatch<Vec<u8>>], expected_nodes: usize) {
        let mut original = TempMerk::new();
        for batch in batches {
            original
                .apply::<Vec<_>, Vec<_>>(batch, &[], None)
                .unwrap()
                .unwrap();
        }

        let chunks = original.chunks().unwrap();

        let storage = TempStorage::default();
        let ctx = storage.get_storage_context(empty()).unwrap();
        let merk = Merk::open_base(ctx, false).unwrap().unwrap();
        let mut restorer = Merk::restore(merk, original.root_hash().unwrap());

        assert_eq!(restorer.remaining_chunks(), None);

        let mut expected_remaining = chunks.len();
        for chunk in chunks {
            let remaining = restorer.process_chunk(chunk.unwrap()).unwrap();

            expected_remaining -= 1;
            assert_eq!(remaining, expected_remaining);
            assert_eq!(restorer.remaining_chunks().unwrap(), expected_remaining);
        }
        assert_eq!(expected_remaining, 0);

        let restored = restorer.finalize().unwrap();
        assert_eq!(restored.root_hash(), original.root_hash());
        assert_raw_db_entries_eq(&restored, &original, expected_nodes);
    }

    #[test]
    fn restore_10000() {
        restore_test(&[&make_batch_seq(0..10_000)], 10_000);
    }

    #[test]
    fn restore_3() {
        restore_test(&[&make_batch_seq(0..3)], 3);
    }

    #[test]
    fn restore_2_left_heavy() {
        restore_test(
            &[
                &[(vec![0], Op::Put(vec![], BasicMerk))],
                &[(vec![1], Op::Put(vec![], BasicMerk))],
            ],
            2,
        );
    }

    #[test]
    fn restore_2_right_heavy() {
        restore_test(
            &[
                &[(vec![1], Op::Put(vec![], BasicMerk))],
                &[(vec![0], Op::Put(vec![], BasicMerk))],
            ],
            2,
        );
    }

    #[test]
    fn restore_1() {
        restore_test(&[&make_batch_seq(0..1)], 1);
    }

    fn assert_raw_db_entries_eq(
        restored: &Merk<PrefixedRocksDbStorageContext>,
        original: &Merk<PrefixedRocksDbStorageContext>,
        length: usize,
    ) {
        assert_eq!(restored.root_hash().unwrap(), original.root_hash().unwrap());

        let mut original_entries = original.storage.raw_iter();
        let mut restored_entries = restored.storage.raw_iter();
        original_entries.seek_to_first().unwrap();
        restored_entries.seek_to_first().unwrap();

        let mut i = 0;
        loop {
            assert_eq!(restored_entries.valid(), original_entries.valid());
            if !restored_entries.valid().unwrap() {
                break;
            }

            assert_eq!(restored_entries.key(), original_entries.key());
            assert_eq!(restored_entries.value(), original_entries.value());

            restored_entries.next().unwrap();
            original_entries.next().unwrap();

            i += 1;
        }

        assert_eq!(i, length);
    }
}
