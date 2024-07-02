// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

// This is required because a diesel macro makes clippy sad
#![allow(clippy::extra_unused_lifetimes)]
#![allow(clippy::unused_unit)]

use field_count::FieldCount;
use std::borrow::Borrow;
use parquet::data_type::{AsBytes, ByteArray, Decimal};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde::de::{self, Visitor};
use std::fmt;
use futures_util::TryFutureExt;
pub type CurrentFungibleAssetBalancePK = String;

#[derive(Clone, Debug, Deserialize, FieldCount, Serialize)]
pub struct FungibleAssetBalance {
    pub txn_version: i64,
    pub write_set_change_index: i64,
    pub storage_id: String,
    pub owner_address: String,
    pub asset_type: String,
    pub is_primary: bool,
    pub is_frozen: bool,
    pub amount: Vec<u8>,
    pub block_timestamp: chrono::NaiveDateTime,
    pub token_standard: String,
}
