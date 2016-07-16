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

//! Creates and registers client and network services.

use util::*;
use util::panics::*;
use spec::Spec;
use error::*;
use client::{Client, ClientConfig, ChainNotify};
use miner::Miner;
use ethdb;
use devtools;

/// Message type for external and internal events
#[derive(Clone)]
pub enum ClientIoMessage {
	/// Best Block Hash in chain has been changed
	NewChainHead,
	/// A block is ready
	BlockVerified,
	/// New transaction RLPs are ready to be imported
	NewTransactions(Vec<Bytes>),
}

/// Client service setup. Creates and registers client and network services with the IO subsystem.
pub struct ClientService {
	io_service: Arc<IoService<ClientIoMessage>>,
	client: Arc<Client>,
	panic_handler: Arc<PanicHandler>,
	_flush_guard: devtools::StopGuard,
}

impl ClientService {
	/// Start the service in a separate thread.
	pub fn start(
		config: ClientConfig,
		spec: Spec,
		db_path: &Path,
		miner: Arc<Miner>,
		) -> Result<ClientService, Error>
	{
		let panic_handler = PanicHandler::new_in_arc();
		let io_service = try!(IoService::<ClientIoMessage>::start());
		panic_handler.forward_from(&io_service);

<<<<<<< HEAD
		let (man, stop) = ethdb::run_manager();

		info!("Starting {}", net_service.host_info());
		info!("Configured for {} using {:?} engine", spec.name, spec.engine.name());
		let client = try!(Client::new(config, spec, man.clone(), db_path, miner, net_service.io().channel()));
=======
		info!("Configured for {} using {} engine", Colour::White.bold().paint(spec.name.clone()), Colour::Yellow.bold().paint(spec.engine.name()));
		let client = try!(Client::new(config, spec, db_path, miner, io_service.channel()));
>>>>>>> origin/misc-perf
		panic_handler.forward_from(client.deref());
		let client_io = Arc::new(ClientIoHandler {
			client: client.clone()
		});
		try!(io_service.register_handler(client_io));

		Ok(ClientService {
			io_service: Arc::new(io_service),
			client: client,
			panic_handler: panic_handler,
			_flush_guard: stop,
		})
	}

	/// Add a node to network
	pub fn add_node(&mut self, _enode: &str) {
		unimplemented!();
	}

	/// Get general IO interface
	pub fn register_io_handler(&self, handler: Arc<IoHandler<ClientIoMessage> + Send>) -> Result<(), IoError> {
		self.io_service.register_handler(handler)
	}

	/// Get client interface
	pub fn client(&self) -> Arc<Client> {
		self.client.clone()
	}

	/// Get network service component
	pub fn io(&self) -> Arc<IoService<ClientIoMessage>> {
		self.io_service.clone()
	}

	/// Set the actor to be notified on certain chain events
	pub fn set_notify(&self, notify: &Arc<ChainNotify>) {
		self.client.set_notify(notify);
	}
}

impl MayPanic for ClientService {
	fn on_panic<F>(&self, closure: F) where F: OnPanicListener {
		self.panic_handler.on_panic(closure);
	}
}

/// IO interface for the Client handler
struct ClientIoHandler {
	client: Arc<Client>
}

const CLIENT_TICK_TIMER: TimerToken = 0;
const CLIENT_TICK_MS: u64 = 5000;

impl IoHandler<ClientIoMessage> for ClientIoHandler {
	fn initialize(&self, io: &IoContext<ClientIoMessage>) {
		io.register_timer(CLIENT_TICK_TIMER, CLIENT_TICK_MS).expect("Error registering client timer");
	}

	fn timeout(&self, _io: &IoContext<ClientIoMessage>, timer: TimerToken) {
		if timer == CLIENT_TICK_TIMER {
			self.client.tick();
		}
	}

	#[cfg_attr(feature="dev", allow(single_match))]
	fn message(&self, io: &IoContext<ClientIoMessage>, net_message: &ClientIoMessage) {
		match *net_message {
			ClientIoMessage::BlockVerified => { self.client.import_verified_blocks(&io.channel()); }
			ClientIoMessage::NewTransactions(ref transactions) => { self.client.import_queued_transactions(&transactions); }
			_ => {} // ignore other messages
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tests::helpers::*;
	use devtools::*;
	use client::ClientConfig;
	use std::sync::Arc;
	use miner::Miner;

	#[test]
	fn it_can_be_started() {
		let temp_path = RandomTempPath::new();
		let service = ClientService::start(
			ClientConfig::default(),
			get_test_spec(),
			&temp_path.as_path(),
			Arc::new(Miner::with_spec(get_test_spec())),
		);
		assert!(service.is_ok());
	}
}
