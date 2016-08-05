// Copyright 2015, 2016 Ethcore (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

#![warn(missing_docs)]
#![cfg_attr(all(nightly, feature="dev"), feature(plugin))]
#![cfg_attr(all(nightly, feature="dev"), plugin(clippy))]

//! Miner module
//! Keeps track of transactions and mined block.
//!
//! Usage example:
//!
//! ```rust
//! extern crate ethcore_util as util;
//! extern crate ethcore;
//! use std::env;
//! use ethcore::ethereum;
//! use ethcore::client::{Client, ClientConfig};
//! use ethcore::miner::{Miner, MinerService};
//!
//! fn main() {
//!		let miner: Miner = Miner::with_spec(ethereum::new_frontier());
//!		// get status
//!		assert_eq!(miner.status().transactions_in_pending_queue, 0);
//!
//!		// Check block for sealing
//!		//assert!(miner.sealing_block(client.deref()).lock().is_some());
//! }
//! ```

mod miner;
mod external;
mod transaction_queue;
mod work_notify;
mod price_info;
mod gas_pricer;

pub use self::transaction_queue::{TransactionQueue, AccountDetails, TransactionOrigin};
pub use self::miner::{Miner, MinerOptions, PendingSet};
pub use self::gas_pricer::{GasPricer, GasPriceCalibratorOptions};
pub use self::external::{ExternalMiner, ExternalMinerService};
pub use client::TransactionImportResult;

use std::collections::BTreeMap;
use util::{H256, U256, Address, Bytes};
use client::{MiningBlockChainClient, Executed, CallAnalytics};
use block::ClosedBlock;
use receipt::Receipt;
use error::{Error, CallError};
use transaction::SignedTransaction;

/// Miner client API
pub trait MinerService : Send + Sync {

	/// Returns miner's status.
	fn status(&self) -> MinerStatus;

	/// Get the author that we will seal blocks as.
	fn author(&self) -> Address;

	/// Set the author that we will seal blocks as.
	fn set_author(&self, author: Address);

	/// Get the extra_data that we will seal blocks with.
	fn extra_data(&self) -> Bytes;

	/// Set the extra_data that we will seal blocks with.
	fn set_extra_data(&self, extra_data: Bytes);

	/// Get current minimal gas price for transactions accepted to queue.
	fn minimal_gas_price(&self) -> U256;

	/// Set minimal gas price of transaction to be accepted for mining.
	fn set_minimal_gas_price(&self, min_gas_price: U256);

	/// Get the lower bound of the gas limit we wish to target when sealing a new block.
	fn gas_floor_target(&self) -> U256;

	/// Get the upper bound of the gas limit we wish to target when sealing a new block.
	fn gas_ceil_target(&self) -> U256;

	// TODO: coalesce into single set_range function.
	/// Set the lower bound of gas limit we wish to target when sealing a new block.
	fn set_gas_floor_target(&self, target: U256);

	/// Set the upper bound of gas limit we wish to target when sealing a new block.
	fn set_gas_ceil_target(&self, target: U256);

	/// Get current transactions limit in queue.
	fn transactions_limit(&self) -> usize;

	/// Set maximal number of transactions kept in the queue (both current and future).
	fn set_transactions_limit(&self, limit: usize);

	/// Set maximum amount of gas allowed for any single transaction to mine.
	fn set_tx_gas_limit(&self, limit: U256);

	/// Imports transactions to transaction queue.
	fn import_external_transactions(&self, chain: &MiningBlockChainClient, transactions: Vec<SignedTransaction>) ->
		Vec<Result<TransactionImportResult, Error>>;

	/// Imports own (node owner) transaction to queue.
	fn import_own_transaction(&self, chain: &MiningBlockChainClient, transaction: SignedTransaction) ->
		Result<TransactionImportResult, Error>;

	/// Returns hashes of transactions currently in pending
	fn pending_transactions_hashes(&self) -> Vec<H256>;

	/// Removes all transactions from the queue and restart mining operation.
	fn clear_and_reset(&self, chain: &MiningBlockChainClient);

	/// Called when blocks are imported to chain, updates transactions queue.
	fn chain_new_blocks(&self, chain: &MiningBlockChainClient, imported: &[H256], invalid: &[H256], enacted: &[H256], retracted: &[H256]);

	/// New chain head event. Restart mining operation.
	fn update_sealing(&self, chain: &MiningBlockChainClient);

	/// Submit `seal` as a valid solution for the header of `pow_hash`.
	/// Will check the seal, but not actually insert the block into the chain.
	fn submit_seal(&self, chain: &MiningBlockChainClient, pow_hash: H256, seal: Vec<Bytes>) -> Result<(), Error>;

	/// Get the sealing work package and if `Some`, apply some transform.
	fn map_sealing_work<F, T>(&self, chain: &MiningBlockChainClient, f: F) -> Option<T>
		where F: FnOnce(&ClosedBlock) -> T, Self: Sized;

	/// Query pending transactions for hash.
	fn transaction(&self, hash: &H256) -> Option<SignedTransaction>;

	/// Get a list of all transactions.
	fn all_transactions(&self) -> Vec<SignedTransaction>;

	/// Get a list of all pending transactions.
	fn pending_transactions(&self) -> Vec<SignedTransaction>;

	/// Get a list of all pending receipts.
	fn pending_receipts(&self) -> BTreeMap<H256, Receipt>;

	/// Returns highest transaction nonce for given address.
	fn last_nonce(&self, address: &Address) -> Option<U256>;

	/// Is it currently sealing?
	fn is_sealing(&self) -> bool;

	/// Suggested gas price.
	fn sensible_gas_price(&self) -> U256 { 20000000000u64.into() }

	/// Suggested gas limit.
	fn sensible_gas_limit(&self) -> U256 { 21000.into() }

	/// Latest account balance in pending state.
	fn balance(&self, chain: &MiningBlockChainClient, address: &Address) -> U256;

	/// Call into contract code using pending state.
	fn call(&self, chain: &MiningBlockChainClient, t: &SignedTransaction, analytics: CallAnalytics) -> Result<Executed, CallError>;

	/// Get storage value in pending state.
	fn storage_at(&self, chain: &MiningBlockChainClient, address: &Address, position: &H256) -> H256;

	/// Get account nonce in pending state.
	fn nonce(&self, chain: &MiningBlockChainClient, address: &Address) -> U256;

	/// Get contract code in pending state.
	fn code(&self, chain: &MiningBlockChainClient, address: &Address) -> Option<Bytes>;
}

/// Mining status
#[derive(Debug)]
pub struct MinerStatus {
	/// Number of transactions in queue with state `pending` (ready to be included in block)
	pub transactions_in_pending_queue: usize,
	/// Number of transactions in queue with state `future` (not yet ready to be included in block)
	pub transactions_in_future_queue: usize,
	/// Number of transactions included in currently mined block
	pub transactions_in_pending_block: usize,
}
