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

//! Helpers
//! Implements helper functions in Element

#[cfg(feature = "full")]
use integer_encoding::VarInt;
#[cfg(feature = "full")]
use merk::{
    tree::{kv::KV, Tree},
    TreeFeatureType,
    TreeFeatureType::{BasicMerk, SummedMerk},
};

#[cfg(any(feature = "full", feature = "verify"))]
use crate::{element::SUM_ITEM_COST_SIZE, Element, Error};
#[cfg(feature = "full")]
use crate::{
    element::{SUM_TREE_COST_SIZE, TREE_COST_SIZE},
    reference_path::{path_from_reference_path_type, ReferencePathType},
    ElementFlags,
};

impl Element {
    #[cfg(any(feature = "full", feature = "verify"))]
    /// Decoded the integer value in the SumItem element type, returns 0 for
    /// everything else
    pub fn sum_value_or_default(&self) -> i64 {
        match self {
            Element::SumItem(sum_value, _) | Element::SumTree(_, sum_value, _) => *sum_value,
            _ => 0,
        }
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Decoded the integer value in the SumItem element type, returns 0 for
    /// everything else
    pub fn as_sum_item_value(&self) -> Result<i64, Error> {
        match self {
            Element::SumItem(value, _) => Ok(*value),
            _ => Err(Error::WrongElementType("expected a sum item")),
        }
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Gives the item value in the Item element type
    pub fn as_item_bytes(&self) -> Result<&[u8], Error> {
        match self {
            Element::Item(value, _) => Ok(value),
            _ => Err(Error::WrongElementType("expected an item")),
        }
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Gives the item value in the Item element type
    pub fn into_item_bytes(self) -> Result<Vec<u8>, Error> {
        match self {
            Element::Item(value, _) => Ok(value),
            _ => Err(Error::WrongElementType("expected an item")),
        }
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Check if the element is a sum tree
    pub fn is_sum_tree(&self) -> bool {
        matches!(self, Element::SumTree(..))
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Check if the element is a tree
    pub fn is_tree(&self) -> bool {
        matches!(self, Element::SumTree(..) | Element::Tree(..))
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Check if the element is an item
    pub fn is_item(&self) -> bool {
        matches!(self, Element::Item(..) | Element::SumItem(..))
    }

    #[cfg(any(feature = "full", feature = "verify"))]
    /// Check if the element is a sum item
    pub fn is_sum_item(&self) -> bool {
        matches!(self, Element::SumItem(..))
    }

    #[cfg(feature = "full")]
    /// Get the tree feature type
    pub fn get_feature_type(&self, parent_is_sum_tree: bool) -> Result<TreeFeatureType, Error> {
        match parent_is_sum_tree {
            true => Ok(SummedMerk(self.sum_value_or_default())),
            false => Ok(BasicMerk),
        }
    }

    #[cfg(feature = "full")]
    /// Grab the optional flag stored in an element
    pub fn get_flags(&self) -> &Option<ElementFlags> {
        match self {
            Element::Tree(_, flags)
            | Element::Item(_, flags)
            | Element::Reference(_, _, flags)
            | Element::SumTree(.., flags)
            | Element::SumItem(_, flags) => flags,
        }
    }

    #[cfg(feature = "full")]
    /// Grab the optional flag stored in an element
    pub fn get_flags_owned(self) -> Option<ElementFlags> {
        match self {
            Element::Tree(_, flags)
            | Element::Item(_, flags)
            | Element::Reference(_, _, flags)
            | Element::SumTree(.., flags)
            | Element::SumItem(_, flags) => flags,
        }
    }

    #[cfg(feature = "full")]
    /// Grab the optional flag stored in an element as mutable
    pub fn get_flags_mut(&mut self) -> &mut Option<ElementFlags> {
        match self {
            Element::Tree(_, flags)
            | Element::Item(_, flags)
            | Element::Reference(_, _, flags)
            | Element::SumTree(.., flags)
            | Element::SumItem(_, flags) => flags,
        }
    }

    #[cfg(feature = "full")]
    /// Get the size of an element in bytes
    #[deprecated]
    pub fn byte_size(&self) -> u32 {
        match self {
            Element::Item(item, element_flag) => {
                if let Some(flag) = element_flag {
                    flag.len() as u32 + item.len() as u32
                } else {
                    item.len() as u32
                }
            }
            Element::SumItem(item, element_flag) => {
                if let Some(flag) = element_flag {
                    flag.len() as u32 + item.required_space() as u32
                } else {
                    item.required_space() as u32
                }
            }
            Element::Reference(path_reference, _, element_flag) => {
                let path_length = path_reference.serialized_size() as u32;

                if let Some(flag) = element_flag {
                    flag.len() as u32 + path_length
                } else {
                    path_length
                }
            }
            Element::Tree(_, element_flag) => {
                if let Some(flag) = element_flag {
                    flag.len() as u32 + 32
                } else {
                    32
                }
            }
            Element::SumTree(_, _, element_flag) => {
                if let Some(flag) = element_flag {
                    flag.len() as u32 + 32 + 8
                } else {
                    32 + 8
                }
            }
        }
    }

    #[cfg(feature = "full")]
    /// Get the required item space
    pub fn required_item_space(len: u32, flag_len: u32) -> u32 {
        len + len.required_space() as u32 + flag_len + flag_len.required_space() as u32 + 1
    }

    #[cfg(feature = "full")]
    /// Convert the reference to an absolute reference
    pub(crate) fn convert_if_reference_to_absolute_reference(
        self,
        path: &[&[u8]],
        key: Option<&[u8]>,
    ) -> Result<Element, Error> {
        // Convert any non absolute reference type to an absolute one
        // we do this here because references are aggregated first then followed later
        // to follow non absolute references, we need the path they are stored at
        // this information is lost during the aggregation phase.
        Ok(match &self {
            Element::Reference(reference_path_type, ..) => match reference_path_type {
                ReferencePathType::AbsolutePathReference(..) => self,
                _ => {
                    // Element is a reference and is not absolute.
                    // build the stored path for this reference
                    let current_path = <&[&[u8]]>::clone(&path).to_vec();
                    let absolute_path = path_from_reference_path_type(
                        reference_path_type.clone(),
                        current_path,
                        key,
                    )?;
                    // return an absolute reference that contains this info
                    Element::Reference(
                        ReferencePathType::AbsolutePathReference(absolute_path),
                        None,
                        None,
                    )
                }
            },
            _ => self,
        })
    }

    #[cfg(feature = "full")]
    /// Get tree costs for a key value
    pub fn specialized_costs_for_key_value(
        key: &Vec<u8>,
        value: &[u8],
        is_sum_node: bool,
    ) -> Result<u32, Error> {
        // todo: we actually don't need to deserialize the whole element
        let element = Element::deserialize(value)?;
        let cost = match element {
            Element::Tree(_, flags) => {
                let flags_len = flags.map_or(0, |flags| {
                    let flags_len = flags.len() as u32;
                    flags_len + flags_len.required_space() as u32
                });
                let value_len = TREE_COST_SIZE + flags_len;
                let key_len = key.len() as u32;
                KV::layered_value_byte_cost_size_for_key_and_value_lengths(
                    key_len,
                    value_len,
                    is_sum_node,
                )
            }
            Element::SumTree(_, _sum_value, flags) => {
                let flags_len = flags.map_or(0, |flags| {
                    let flags_len = flags.len() as u32;
                    flags_len + flags_len.required_space() as u32
                });
                let value_len = SUM_TREE_COST_SIZE + flags_len;
                let key_len = key.len() as u32;
                KV::layered_value_byte_cost_size_for_key_and_value_lengths(
                    key_len,
                    value_len,
                    is_sum_node,
                )
            }
            Element::SumItem(.., flags) => {
                let flags_len = flags.map_or(0, |flags| {
                    let flags_len = flags.len() as u32;
                    flags_len + flags_len.required_space() as u32
                });
                let value_len = SUM_ITEM_COST_SIZE + flags_len;
                let key_len = key.len() as u32;
                KV::specialized_value_byte_cost_size_for_key_and_value_lengths(
                    key_len,
                    value_len,
                    is_sum_node,
                )
            }
            _ => KV::specialized_value_byte_cost_size_for_key_and_value_lengths(
                key.len() as u32,
                value.len() as u32,
                is_sum_node,
            ),
        };
        Ok(cost)
    }

    #[cfg(feature = "full")]
    /// Get tree cost for the element
    pub fn get_specialized_cost(&self) -> Result<u32, Error> {
        match self {
            Element::Tree(..) => Ok(TREE_COST_SIZE),
            Element::SumTree(..) => Ok(SUM_TREE_COST_SIZE),
            Element::SumItem(..) => Ok(SUM_ITEM_COST_SIZE),
            _ => Err(Error::CorruptedCodeExecution(
                "trying to get tree cost from non tree element",
            )),
        }
    }
}

#[cfg(feature = "full")]
/// Decode from bytes
pub fn raw_decode(bytes: &[u8]) -> Result<Element, Error> {
    let tree = Tree::decode_raw(bytes, vec![]).map_err(|e| Error::CorruptedData(e.to_string()))?;
    let element: Element = Element::deserialize(tree.value_as_slice())?;
    Ok(element)
}
