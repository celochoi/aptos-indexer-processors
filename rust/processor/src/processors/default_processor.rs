// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::processor_trait::{ProcessingResult, ProcessorTrait};
use crate::{
    models::default_models::{
        block_metadata_transactions::BlockMetadataTransactionModel,
        events::EventModel,
        move_modules::MoveModule,
        move_resources::MoveResource,
        move_tables::{TableItem, TableMetadata},
        signatures::Signature,
        transactions::{TransactionDetail, TransactionModel},
        user_transactions::UserTransactionModel,
        write_set_changes::{WriteSetChangeDetail, WriteSetChangeModel},
    },
    schema,
    utils::database::{
        clean_data_for_db, execute_with_better_error, get_chunks, PgDbPool, PgPoolConnection,
    },
};
use anyhow::bail;
use aptos_indexer_protos::transaction::v1::{write_set_change::Change, Transaction};
use async_trait::async_trait;
use diesel::{pg::upsert::excluded, result::Error, ExpressionMethods, PgConnection};
use field_count::FieldCount;
use std::{collections::HashMap, fmt::Debug};
use tracing::error;

pub const NAME: &str = "default_processor";
pub struct DefaultTransactionProcessor {
    connection_pool: PgDbPool,
}

impl DefaultTransactionProcessor {
    pub fn new(connection_pool: PgDbPool) -> Self {
        Self { connection_pool }
    }
}

impl Debug for DefaultTransactionProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "DefaultTransactionProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

fn insert_to_db_impl(
    conn: &mut PgConnection,
    txns: &[TransactionModel],
    (user_transactions, signatures, block_metadata_transactions): (
        &[UserTransactionModel],
        &[Signature],
        &[BlockMetadataTransactionModel],
    ),
    events: &[EventModel],
    wscs: &[WriteSetChangeModel],
    (move_modules, move_resources, table_items, table_metadata): (
        &[MoveModule],
        &[MoveResource],
        &[TableItem],
        &[TableMetadata],
    ),
) -> Result<(), diesel::result::Error> {
    insert_transactions(conn, txns)?;
    Ok(())
}

fn insert_to_db(
    conn: &mut PgPoolConnection,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    txns: Vec<TransactionModel>,
    (user_transactions, signatures, block_metadata_transactions): (
        Vec<UserTransactionModel>,
        Vec<Signature>,
        Vec<BlockMetadataTransactionModel>,
    ),
    events: Vec<EventModel>,
    wscs: Vec<WriteSetChangeModel>,
    (move_modules, move_resources, table_items, table_metadata): (
        Vec<MoveModule>,
        Vec<MoveResource>,
        Vec<TableItem>,
        Vec<TableMetadata>,
    ),
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );
    match conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|pg_conn| {
            insert_to_db_impl(
                pg_conn,
                &txns,
                (
                    &user_transactions,
                    &signatures,
                    &block_metadata_transactions,
                ),
                &events,
                &wscs,
                (
                    &move_modules,
                    &move_resources,
                    &table_items,
                    &table_metadata,
                ),
            )
        }) {
        Ok(_) => Ok(()),
        Err(_) => {
            let txns = clean_data_for_db(txns, true);
            let user_transactions = clean_data_for_db(user_transactions, true);
            let signatures = clean_data_for_db(signatures, true);
            let block_metadata_transactions = clean_data_for_db(block_metadata_transactions, true);
            let events = clean_data_for_db(events, true);
            let wscs = clean_data_for_db(wscs, true);
            let move_modules = clean_data_for_db(move_modules, true);
            let move_resources = clean_data_for_db(move_resources, true);
            let table_items = clean_data_for_db(table_items, true);
            let table_metadata = clean_data_for_db(table_metadata, true);

            conn.build_transaction()
                .read_write()
                .run::<_, Error, _>(|pg_conn| {
                    insert_to_db_impl(
                        pg_conn,
                        &txns,
                        (
                            &user_transactions,
                            &signatures,
                            &block_metadata_transactions,
                        ),
                        &events,
                        &wscs,
                        (
                            &move_modules,
                            &move_resources,
                            &table_items,
                            &table_metadata,
                        ),
                    )
                })
        },
    }
}

fn insert_transactions(
    conn: &mut PgConnection,
    items_to_insert: &[TransactionModel],
) -> Result<(), diesel::result::Error> {
    use schema::transactions::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), TransactionModel::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::transactions::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict(version)
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_user_transactions(
    conn: &mut PgConnection,
    items_to_insert: &[UserTransactionModel],
) -> Result<(), diesel::result::Error> {
    use schema::user_transactions::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), UserTransactionModel::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::user_transactions::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict(version)
                .do_update()
                .set((
                    entry_function_id_str.eq(excluded(entry_function_id_str)),
                    inserted_at.eq(excluded(inserted_at)),
                )),
            None,
        )?;
    }
    Ok(())
}

fn insert_signatures(
    conn: &mut PgConnection,
    items_to_insert: &[Signature],
) -> Result<(), diesel::result::Error> {
    use schema::signatures::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), Signature::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::signatures::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict((
                    transaction_version,
                    multi_agent_index,
                    multi_sig_index,
                    is_sender_primary,
                ))
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_block_metadata_transactions(
    conn: &mut PgConnection,
    items_to_insert: &[BlockMetadataTransactionModel],
) -> Result<(), diesel::result::Error> {
    use schema::block_metadata_transactions::dsl::*;
    let chunks = get_chunks(
        items_to_insert.len(),
        BlockMetadataTransactionModel::field_count(),
    );
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::block_metadata_transactions::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict(version)
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_events(
    conn: &mut PgConnection,
    items_to_insert: &[EventModel],
) -> Result<(), diesel::result::Error> {
    use schema::events::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), EventModel::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::events::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict((account_address, creation_number, sequence_number))
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_write_set_changes(
    conn: &mut PgConnection,
    items_to_insert: &[WriteSetChangeModel],
) -> Result<(), diesel::result::Error> {
    use schema::write_set_changes::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), WriteSetChangeModel::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::write_set_changes::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict((transaction_version, index))
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_move_modules(
    conn: &mut PgConnection,
    items_to_insert: &[MoveModule],
) -> Result<(), diesel::result::Error> {
    use schema::move_modules::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), MoveModule::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::move_modules::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict((transaction_version, write_set_change_index))
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_move_resources(
    conn: &mut PgConnection,
    items_to_insert: &[MoveResource],
) -> Result<(), diesel::result::Error> {
    use schema::move_resources::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), MoveResource::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::move_resources::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict((transaction_version, write_set_change_index))
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

fn insert_table_items(
    conn: &mut PgConnection,
    items_to_insert: &[TableItem],
) -> Result<(), diesel::result::Error> {
    use schema::table_items::dsl::*;
    let chunks = get_chunks(items_to_insert.len(), TableItem::field_count());
    for (start_ind, end_ind) in chunks {
        execute_with_better_error(
            conn,
            diesel::insert_into(schema::table_items::table)
                .values(&items_to_insert[start_ind..end_ind])
                .on_conflict((transaction_version, write_set_change_index))
                .do_nothing(),
            None,
        )?;
    }
    Ok(())
}

#[async_trait]
impl ProcessorTrait for DefaultTransactionProcessor {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn process_transactions(
        &self,
        transactions: Vec<Transaction>,
        start_version: u64,
        end_version: u64,
    ) -> anyhow::Result<ProcessingResult> {
        let mut conn = self.get_conn();
        let (txns, txn_details, events, write_set_changes, wsc_details) =
            TransactionModel::from_transactions(&transactions);

        let mut signatures = vec![];
        let mut user_transactions = vec![];
        let mut block_metadata_transactions = vec![];
        for detail in txn_details {
            match detail {
                TransactionDetail::User(user_txn, sigs) => {
                    signatures.append(&mut sigs.clone());
                    user_transactions.push(user_txn.clone());
                },
                TransactionDetail::BlockMetadata(bmt) => {
                    block_metadata_transactions.push(bmt.clone())
                },
            }
        }
        let mut move_modules = vec![];
        let mut move_resources = vec![];
        let mut table_items = vec![];
        let mut table_metadata = HashMap::new();
        for detail in wsc_details {
            match detail {
                WriteSetChangeDetail::Module(module) => move_modules.push(module.clone()),
                WriteSetChangeDetail::Resource(resource) => move_resources.push(resource.clone()),
                WriteSetChangeDetail::Table(item, metadata) => {
                    table_items.push(item.clone());

                    if let Some(meta) = metadata {
                        table_metadata.insert(meta.handle.clone(), meta.clone());
                    }
                },
            }
        }

        
        // Getting list of values and sorting by pk in order to avoid postgres deadlock since we're doing multi threaded db writes
        let mut table_metadata = table_metadata.into_values().collect::<Vec<TableMetadata>>();
        
        table_metadata.sort_by(|a, b| a.handle.cmp(&b.handle));

        let tx_result = insert_to_db(
            &mut conn,
            self.name(),
            start_version,
            end_version,
            txns,
            (user_transactions, signatures, block_metadata_transactions),
            events,
            write_set_changes,
            (
                move_modules,
                move_resources,
                table_items,
                table_metadata,
            ),
        );
        match tx_result {
            Ok(_) => Ok((start_version, end_version)),
            Err(e) => {
                error!(
                    start_version = start_version,
                    end_version = end_version,
                    processor_name = self.name(),
                    error = ?e,
                    "[Parser] Error inserting transactions to db",
                );
                bail!(e)
            },
        }
    }

    fn connection_pool(&self) -> &PgDbPool {
        &self.connection_pool
    }
}
