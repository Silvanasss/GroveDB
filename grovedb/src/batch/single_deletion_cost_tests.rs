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

//! Tests

#[cfg(feature = "full")]
mod tests {

    use costs::storage_cost::removal::{
        Identifier, StorageRemovalPerEpochByIdentifier,
        StorageRemovedBytes::SectionedStorageRemoval,
    };
    use intmap::IntMap;

    use crate::{batch::GroveDbOp, tests::make_empty_grovedb, Element};

    #[test]
    fn test_batch_one_deletion_tree_costs_match_non_batch_on_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(vec![], b"key1", Element::empty_tree(), None, None)
            .cost_as_result()
            .expect("expected to insert successfully");

        let tx = db.start_transaction();

        let non_batch_cost = db
            .delete(vec![], b"key1", None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 37
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for empty tree value
        // 32 for node hash
        // 0 for value hash
        // 2 byte for the value_size (required space for 98 + up to 256 for child key)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 37 + 39 = 113

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        tx.rollback().expect("expected to rollback");
        let ops = vec![GroveDbOp::delete_tree_op(vec![], b"key1".to_vec(), false)];
        let batch_cost = db
            .apply_batch(ops, None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_item_costs_match_non_batch_on_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::new_item(b"cat".to_vec()),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let tx = db.start_transaction();

        let non_batch_cost = db
            .delete(vec![], b"key1", None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 71
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for required space for bytes
        //   3 bytes for value
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 70)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 71 + 39 = 147

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        tx.rollback().expect("expected to rollback");
        let ops = vec![GroveDbOp::delete_op(vec![], b"key1".to_vec())];
        let batch_cost = db
            .apply_batch(ops, None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_tree_costs_match_non_batch_without_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(vec![], b"key1", Element::empty_tree(), None, None)
            .cost_as_result()
            .expect("expected to insert successfully");

        let non_batch_cost = db
            .delete(vec![], b"key1", None, None)
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 37
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for empty tree value
        // 32 for node hash
        // 0 for value hash
        // 2 byte for the value_size (required space for 98 + up to 256 for child key)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 37 + 39 = 113

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        let db = make_empty_grovedb();

        let _insertion_cost = db
            .insert(vec![], b"key1", Element::empty_tree(), None, None)
            .cost_as_result()
            .expect("expected to insert successfully");

        let ops = vec![GroveDbOp::delete_tree_op(vec![], b"key1".to_vec(), false)];
        let batch_cost = db
            .apply_batch(ops, None, None)
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_item_costs_match_non_batch_without_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::new_item(b"cat".to_vec()),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let non_batch_cost = db
            .delete(vec![], b"key1", None, None)
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 71
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for required space for bytes
        //   3 bytes for value
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 70)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 71 + 39 = 147

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        let db = make_empty_grovedb();

        let _insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::new_item(b"cat".to_vec()),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let ops = vec![GroveDbOp::delete_op(vec![], b"key1".to_vec())];
        let batch_cost = db
            .apply_batch(ops, None, None)
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_tree_with_flags_costs_match_non_batch_on_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::empty_tree_with_flags(Some(b"dog".to_vec())),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let tx = db.start_transaction();

        let non_batch_cost = db
            .delete(vec![], b"key1", None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 116 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 42
        //   1 for the flag option (but no flags)
        //   1 for the flags size
        //   3 bytes for flags
        //   1 for the enum type
        //   1 for empty tree value
        //   1 for Basic Merk
        // 32 for node hash
        // 0 for value hash
        // 2 byte for the value_size (required space for 98 + up to 256 for child key)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1

        // Total 37 + 42 + 40 = 119

        assert_eq!(insertion_cost.storage_cost.added_bytes, 119);
        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        tx.rollback().expect("expected to rollback");
        let ops = vec![GroveDbOp::delete_tree_op(vec![], b"key1".to_vec(), false)];
        let batch_cost = db
            .apply_batch(ops, None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_tree_with_identity_cost_flags_costs_match_non_batch_on_transaction()
    {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::empty_tree_with_flags(Some(vec![0, 0])),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let tx = db.start_transaction();

        let non_batch_cost = db
            .delete_with_sectional_storage_function(
                vec![],
                b"key1",
                None,
                Some(&tx),
                &mut |_element_flags, removed_key_bytes, removed_value_bytes| {
                    let mut removed_bytes = StorageRemovalPerEpochByIdentifier::default();
                    // we are removing 1 byte from epoch 0 for an identity
                    let mut removed_bytes_for_identity = IntMap::new();
                    removed_bytes_for_identity.insert(0, removed_key_bytes);
                    removed_bytes.insert(Identifier::default(), removed_bytes_for_identity);
                    let key_sectioned = SectionedStorageRemoval(removed_bytes);

                    let mut removed_bytes = StorageRemovalPerEpochByIdentifier::default();
                    // we are removing 1 byte from epoch 0 for an identity
                    let mut removed_bytes_for_identity = IntMap::new();
                    removed_bytes_for_identity.insert(0, removed_value_bytes);
                    removed_bytes.insert(Identifier::default(), removed_bytes_for_identity);
                    let value_sectioned = SectionedStorageRemoval(removed_bytes);
                    Ok((key_sectioned, value_sectioned))
                },
            )
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 116 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 41
        //   1 for the flag option (but no flags)
        //   1 for the flags size
        //   2 bytes for flags
        //   1 for the enum type
        //   1 for empty tree value
        //   1 for basic merk
        // 32 for node hash
        // 0 for value hash
        // 2 byte for the value_size (required space for 98 + up to 256 for child key)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 41 + 40 = 118

        assert_eq!(insertion_cost.storage_cost.added_bytes, 118);
        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );
        assert!(matches!(
            non_batch_cost.storage_cost.removed_bytes,
            SectionedStorageRemoval(_)
        ));

        tx.rollback().expect("expected to rollback");
        let ops = vec![GroveDbOp::delete_tree_op(vec![], b"key1".to_vec(), false)];
        let batch_cost = db
            .apply_batch_with_element_flags_update(
                ops,
                None,
                |_, _, _| Ok(false),
                |_element_flags, removed_key_bytes, removed_value_bytes| {
                    let mut removed_bytes = StorageRemovalPerEpochByIdentifier::default();
                    // we are removing 1 byte from epoch 0 for an identity
                    let mut removed_bytes_for_identity = IntMap::new();
                    removed_bytes_for_identity.insert(0, removed_key_bytes);
                    removed_bytes.insert(Identifier::default(), removed_bytes_for_identity);
                    let key_sectioned = SectionedStorageRemoval(removed_bytes);

                    let mut removed_bytes = StorageRemovalPerEpochByIdentifier::default();
                    // we are removing 1 byte from epoch 0 for an identity
                    let mut removed_bytes_for_identity = IntMap::new();
                    removed_bytes_for_identity.insert(0, removed_value_bytes);
                    removed_bytes.insert(Identifier::default(), removed_bytes_for_identity);
                    let value_sectioned = SectionedStorageRemoval(removed_bytes);
                    Ok((key_sectioned, value_sectioned))
                },
                Some(&tx),
            )
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
        assert!(matches!(
            batch_cost.storage_cost.removed_bytes,
            SectionedStorageRemoval(_)
        ));
    }

    #[test]
    fn test_batch_one_deletion_item_with_flags_costs_match_non_batch_on_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::new_item_with_flags(b"cat".to_vec(), Some(b"apple".to_vec())),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let tx = db.start_transaction();

        let non_batch_cost = db
            .delete(vec![], b"key1", None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 71
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for required space for bytes
        //   3 bytes for value
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 70)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 71 + 39 = 147

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        tx.rollback().expect("expected to rollback");
        let ops = vec![GroveDbOp::delete_op(vec![], b"key1".to_vec())];
        let batch_cost = db
            .apply_batch(ops, None, Some(&tx))
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_tree_with_flags_costs_match_non_batch_without_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::empty_tree_with_flags(Some(b"dog".to_vec())),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let non_batch_cost = db
            .delete(vec![], b"key1", None, None)
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 42
        //   1 for the flag option
        //   1 for flags size
        //   3 for flag bytes
        //   1 for the enum type
        //   1 for empty tree value
        //   1 for Basic Merk
        // 32 for node hash
        // 0 for value hash
        // 2 byte for the value_size (required space for 98 + up to 256 for child key)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 42 + 40 = 119

        assert_eq!(insertion_cost.storage_cost.added_bytes, 119);

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        let db = make_empty_grovedb();

        let _insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::empty_tree_with_flags(Some(b"dog".to_vec())),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let ops = vec![GroveDbOp::delete_tree_op(vec![], b"key1".to_vec(), false)];
        let batch_cost = db
            .apply_batch(ops, None, None)
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }

    #[test]
    fn test_batch_one_deletion_item_with_flags_costs_match_non_batch_without_transaction() {
        let db = make_empty_grovedb();

        let insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::new_item_with_flags(b"cat".to_vec(), Some(b"apple".to_vec())),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let non_batch_cost = db
            .delete(vec![], b"key1", None, None)
            .cost_as_result()
            .expect("expected to delete successfully");

        // Explanation for 113 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 71
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for required space for bytes
        //   3 bytes for value
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 70)

        // Parent Hook -> 40
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2
        // Sum 1
        // Total 37 + 71 + 39 = 147

        assert_eq!(
            insertion_cost.storage_cost.added_bytes,
            non_batch_cost
                .storage_cost
                .removed_bytes
                .total_removed_bytes()
        );

        let db = make_empty_grovedb();

        let _insertion_cost = db
            .insert(
                vec![],
                b"key1",
                Element::new_item_with_flags(b"cat".to_vec(), Some(b"apple".to_vec())),
                None,
                None,
            )
            .cost_as_result()
            .expect("expected to insert successfully");

        let ops = vec![GroveDbOp::delete_op(vec![], b"key1".to_vec())];
        let batch_cost = db
            .apply_batch(ops, None, None)
            .cost_as_result()
            .expect("expected to delete successfully");
        assert_eq!(non_batch_cost.storage_cost, batch_cost.storage_cost);
    }
}
