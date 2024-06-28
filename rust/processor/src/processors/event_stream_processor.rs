// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::{
    db::common::models::{
        events_models::events::{CachedEvents, EventContext, EventModel, EventStreamMessage},
        fungible_asset_models::{
            v2_fungible_asset_activities::{EventToCoinType, FungibleAssetActivity},
            v2_fungible_asset_balances::FungibleAssetBalance,
        },
    },
    processors::{DefaultProcessingResult, ProcessingResult, ProcessorName, ProcessorTrait},
    utils::{
        database::ArcDbPool,
        in_memory_cache::InMemoryCache,
        util::{get_entry_function_from_user_request, parse_timestamp},
    },
};
use ahash::AHashMap;
use aptos_in_memory_cache::Cache;
use aptos_protos::transaction::v1::{transaction::TxnData, write_set_change::Change, Transaction};
use async_trait::async_trait;
use std::{fmt::Debug, sync::Arc};

pub struct EventStreamProcessor {
    connection_pool: ArcDbPool,
    cache: Arc<InMemoryCache>,
}

impl EventStreamProcessor {
    pub fn new(connection_pool: ArcDbPool, cache: Arc<InMemoryCache>) -> Self {
        Self {
            connection_pool,
            cache,
        }
    }
}

impl Debug for EventStreamProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "EventStreamProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[async_trait]
impl ProcessorTrait for EventStreamProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::EventStreamProcessor.into()
    }

    async fn process_transactions(
        &self,
        transactions: Vec<Transaction>,
        start_version: u64,
        end_version: u64,
        _: Option<u64>,
    ) -> anyhow::Result<ProcessingResult> {
        let processing_start = std::time::Instant::now();
        let mut batch = vec![];
        for txn in &transactions {
            let txn_version = txn.version as i64;
            let block_height = txn.block_height as i64;
            let txn_data = txn.txn_data.as_ref().expect("Txn Data doesn't exit!");
            let transaction_info = txn.info.as_ref().expect("Transaction info doesn't exist!");
            let txn_timestamp = parse_timestamp(txn.timestamp.as_ref().unwrap(), txn_version);
            let default = vec![];
            let (raw_events, _user_request, entry_function_id_str) = match txn_data {
                TxnData::BlockMetadata(tx_inner) => (&tx_inner.events, None, None),
                TxnData::Genesis(tx_inner) => (&tx_inner.events, None, None),
                TxnData::User(tx_inner) => {
                    let user_request = tx_inner
                        .request
                        .as_ref()
                        .expect("Sends is not present in user txn");
                    let entry_function_id_str = get_entry_function_from_user_request(user_request);
                    (&tx_inner.events, Some(user_request), entry_function_id_str)
                },
                _ => (&default, None, None),
            };

            // This is because v1 events (deposit/withdraw) don't have coin type so the only way is to match
            // the event to the resource using the event guid
            let mut event_to_v1_coin_type: EventToCoinType = AHashMap::new();

            for (index, wsc) in transaction_info.changes.iter().enumerate() {
                if let Change::WriteResource(write_resource) = wsc.change.as_ref().unwrap() {
                    if let Some((_balance, _current_balance, event_to_coin)) =
                        FungibleAssetBalance::get_v1_from_write_resource(
                            write_resource,
                            index as i64,
                            txn_version,
                            txn_timestamp,
                        )
                        .unwrap()
                    {
                        event_to_v1_coin_type.extend(event_to_coin);
                    }
                }
            }

            let mut event_context = AHashMap::new();
            for (index, event) in raw_events.iter().enumerate() {
                // Only support v1 for now
                if let Some(v1_activity) = FungibleAssetActivity::get_v1_from_event(
                    event,
                    txn_version,
                    block_height,
                    txn_timestamp,
                    &entry_function_id_str,
                    &event_to_v1_coin_type,
                    index as i64,
                )
                .unwrap_or_else(|e| {
                    tracing::error!(
                        transaction_version = txn_version,
                        index = index,
                        error = ?e,
                        "[Parser] error parsing fungible asset activity v1");
                    panic!("[Parser] error parsing fungible asset activity v1");
                }) {
                    event_context.insert((txn_version, index as i64), EventContext {
                        coin_type: v1_activity.asset_type.clone(),
                    });
                }
            }

            batch.push(CachedEvents {
                transaction_version: txn_version,
                events: EventModel::from_events(raw_events, txn_version, block_height)
                    .iter()
                    .map(|event| {
                        let context = event_context
                            .get(&(txn_version, event.event_index))
                            .cloned();
                        Arc::new(EventStreamMessage::from_event(
                            event,
                            context,
                            txn_timestamp.clone(),
                        ))
                    })
                    .collect(),
            });
        }

        for events in batch {
            self.cache
                .insert(events.transaction_version, events.clone());
        }

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        Ok(ProcessingResult::DefaultProcessingResult(
            DefaultProcessingResult {
                start_version,
                end_version,
                last_transaction_timestamp: transactions.last().unwrap().timestamp.clone(),
                processing_duration_in_secs,
                db_insertion_duration_in_secs: 0.0,
            },
        ))
    }

    fn connection_pool(&self) -> &ArcDbPool {
        &self.connection_pool
    }
}