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

//! Batch structure

#[cfg(feature = "full")]
use std::{collections::BTreeMap, fmt};

#[cfg(feature = "full")]
use costs::{
    cost_return_on_error,
    storage_cost::{removal::StorageRemovedBytes, StorageCost},
    CostResult, CostsExt, OperationCost,
};
#[cfg(feature = "full")]
use nohash_hasher::IntMap;
#[cfg(feature = "full")]
use visualize::{DebugByteVectors, DebugBytes};

#[cfg(feature = "full")]
use crate::{
    batch::{key_info::KeyInfo, GroveDbOp, KeyInfoPath, Op, TreeCache},
    Element, ElementFlags, Error,
};

#[cfg(feature = "full")]
pub type OpsByPath = BTreeMap<KeyInfoPath, BTreeMap<KeyInfo, Op>>;
/// Level, path, key, op
#[cfg(feature = "full")]
pub type OpsByLevelPath = IntMap<u32, OpsByPath>;

/// Batch structure
#[cfg(feature = "full")]
pub(super) struct BatchStructure<C, F, SR> {
    /// Operations by level path
    pub(super) ops_by_level_paths: OpsByLevelPath,
    /// This is for references
    pub(super) ops_by_qualified_paths: BTreeMap<Vec<Vec<u8>>, Op>,
    /// Merk trees
    /// Very important: the type of run mode we are in is contained in this
    /// cache
    pub(super) merk_tree_cache: C,
    /// Flags modification function
    pub(super) flags_update: F,
    /// Split removal bytes
    pub(super) split_removal_bytes: SR,
    /// Last level
    pub(super) last_level: u32,
}

#[cfg(feature = "full")]
impl<F, SR, S: fmt::Debug> fmt::Debug for BatchStructure<S, F, SR> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt_int_map = IntMap::default();
        for (level, path_map) in self.ops_by_level_paths.iter() {
            let mut fmt_path_map = BTreeMap::default();

            for (path, key_map) in path_map.iter() {
                let mut fmt_key_map = BTreeMap::default();

                for (key, op) in key_map.iter() {
                    fmt_key_map.insert(DebugBytes(key.get_key_clone()), op);
                }
                fmt_path_map.insert(DebugByteVectors(path.to_path()), fmt_key_map);
            }
            fmt_int_map.insert(*level, fmt_path_map);
        }

        f.debug_struct("BatchStructure")
            .field("ops_by_level_paths", &fmt_int_map)
            .field("merk_tree_cache", &self.merk_tree_cache)
            .field("last_level", &self.last_level)
            .finish()
    }
}

#[cfg(feature = "full")]
impl<C, F, SR> BatchStructure<C, F, SR>
where
    C: TreeCache<F, SR>,
    F: FnMut(&StorageCost, Option<ElementFlags>, &mut ElementFlags) -> Result<bool, Error>,
    SR: FnMut(
        &mut ElementFlags,
        u32,
        u32,
    ) -> Result<(StorageRemovedBytes, StorageRemovedBytes), Error>,
{
    /// Create batch structure from a list of ops. Returns CostResult.
    pub(super) fn from_ops(
        ops: Vec<GroveDbOp>,
        update_element_flags_function: F,
        split_remove_bytes_function: SR,
        mut merk_tree_cache: C,
    ) -> CostResult<BatchStructure<C, F, SR>, Error> {
        Self::continue_from_ops(
            None,
            ops,
            update_element_flags_function,
            split_remove_bytes_function,
            merk_tree_cache,
        )
    }

    /// Create batch structure from a list of ops. Returns CostResult.
    pub(super) fn continue_from_ops(
        previous_ops: Option<OpsByLevelPath>,
        ops: Vec<GroveDbOp>,
        update_element_flags_function: F,
        split_remove_bytes_function: SR,
        mut merk_tree_cache: C,
    ) -> CostResult<BatchStructure<C, F, SR>, Error> {
        let mut cost = OperationCost::default();

        let mut ops_by_level_paths: OpsByLevelPath = previous_ops.unwrap_or_default();
        let mut current_last_level: u32 = 0;

        // qualified paths meaning path + key
        let mut ops_by_qualified_paths: BTreeMap<Vec<Vec<u8>>, Op> = BTreeMap::new();

        for op in ops.into_iter() {
            let mut path = op.path.clone();
            path.push(op.key.clone());
            ops_by_qualified_paths.insert(path.to_path_consume(), op.op.clone());
            let op_cost = OperationCost::default();
            let op_result = match &op.op {
                Op::Insert { element } | Op::Replace { element } | Op::Patch { element, .. } => {
                    if let Element::Tree(..) = element {
                        cost_return_on_error!(&mut cost, merk_tree_cache.insert(&op, false));
                    } else if let Element::SumTree(..) = element {
                        cost_return_on_error!(&mut cost, merk_tree_cache.insert(&op, true));
                    }
                    Ok(())
                }
                Op::Delete | Op::DeleteTree | Op::DeleteSumTree => Ok(()),
                Op::ReplaceTreeRootKey { .. } | Op::InsertTreeWithRootHash { .. } => {
                    Err(Error::InvalidBatchOperation(
                        "replace and insert tree hash are internal operations only",
                    ))
                }
            };
            if op_result.is_err() {
                return Err(op_result.err().unwrap()).wrap_with_cost(op_cost);
            }

            let level = op.path.len();
            if let Some(ops_on_level) = ops_by_level_paths.get_mut(&level) {
                if let Some(ops_on_path) = ops_on_level.get_mut(&op.path) {
                    ops_on_path.insert(op.key, op.op);
                } else {
                    let mut ops_on_path: BTreeMap<KeyInfo, Op> = BTreeMap::new();
                    ops_on_path.insert(op.key, op.op);
                    ops_on_level.insert(op.path.clone(), ops_on_path);
                }
            } else {
                let mut ops_on_path: BTreeMap<KeyInfo, Op> = BTreeMap::new();
                ops_on_path.insert(op.key, op.op);
                let mut ops_on_level: BTreeMap<KeyInfoPath, BTreeMap<KeyInfo, Op>> =
                    BTreeMap::new();
                ops_on_level.insert(op.path, ops_on_path);
                ops_by_level_paths.insert(level, ops_on_level);
                if current_last_level < level {
                    current_last_level = level;
                }
            }
        }

        Ok(BatchStructure {
            ops_by_level_paths,
            ops_by_qualified_paths,
            merk_tree_cache,
            flags_update: update_element_flags_function,
            split_removal_bytes: split_remove_bytes_function,
            last_level: current_last_level,
        })
        .wrap_with_cost(cost)
    }
}
