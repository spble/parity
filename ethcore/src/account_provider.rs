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

//! Account management.

use std::fmt;
use std::collections::HashMap;
use std::time::{Instant, Duration};
use util::{Address as H160, H256, H520, Mutex, RwLock};
use ethstore::{SecretStore, Error as SSError, SafeAccount, EthStore};
use ethstore::dir::{KeyDirectory};
use ethstore::ethkey::{Address as SSAddress, Message as SSMessage, Secret as SSSecret, Random, Generator};


/// Type of unlock.
#[derive(Clone)]
enum Unlock {
	/// If account is unlocked temporarily, it should be locked after first usage.
	Temp,
	/// Account unlocked permantently can always sign message.
	/// Use with caution.
	Perm,
	/// Account unlocked with a timeout
	Timed((Instant, u32)),
}

/// Data associated with account.
#[derive(Clone)]
struct AccountData {
	unlock: Unlock,
	password: String,
}

/// `AccountProvider` errors.
#[derive(Debug)]
pub enum Error {
	/// Returned when account is not unlocked.
	NotUnlocked,
	/// Returned when signing fails.
	SStore(SSError),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		match *self {
			Error::NotUnlocked => write!(f, "Account is locked"),
			Error::SStore(ref e) => write!(f, "{}", e),
		}
	}
}

impl From<SSError> for Error {
	fn from(e: SSError) -> Self {
		Error::SStore(e)
	}
}

macro_rules! impl_bridge_type {
	($name: ident, $size: expr, $core: ident, $store: ident) => {
		/// Primitive
		pub struct $name([u8; $size]);

		impl From<[u8; $size]> for $name {
			fn from(s: [u8; $size]) -> Self {
				$name(s)
			}
		}

		impl From<$core> for $name {
			fn from(s: $core) -> Self {
				$name(s.0)
			}
		}

		impl From<$store> for $name {
			fn from(s: $store) -> Self {
				$name(s.into())
			}
		}

		impl Into<$core> for $name {
			fn into(self) -> $core {
				$core(self.0)
			}
		}

		impl Into<$store> for $name {
			fn into(self) -> $store {
				$store::from(self.0)
			}
		}
	}
}

impl_bridge_type!(Secret, 32, H256, SSSecret);
impl_bridge_type!(Message, 32, H256, SSMessage);
impl_bridge_type!(Address, 20, H160, SSAddress);


#[derive(Default)]
struct NullDir {
	accounts: RwLock<HashMap<SSAddress, SafeAccount>>,
}

impl KeyDirectory for NullDir {
	fn load(&self) -> Result<Vec<SafeAccount>, SSError> {
		Ok(self.accounts.read().values().cloned().collect())
	}

	fn insert(&self, account: SafeAccount) -> Result<SafeAccount, SSError> {
		self.accounts.write().insert(account.address.clone(), account.clone());
		Ok(account)
	}

	fn remove(&self, address: &SSAddress) -> Result<(), SSError> {
		self.accounts.write().remove(address);
		Ok(())
	}
}

/// Account management.
/// Responsible for unlocking accounts.
pub struct AccountProvider {
	unlocked: Mutex<HashMap<SSAddress, AccountData>>,
	sstore: Box<SecretStore>,
}

/// Collected account metadata
#[derive(Clone, Debug, PartialEq)]
pub struct AccountMeta {
	/// The name of the account.
	pub name: String,
	/// The rest of the metadata of the account.
	pub meta: String,
	/// The 128-bit UUID of the account, if it has one (brain-wallets don't).
	pub uuid: Option<String>,
}

impl Default for AccountMeta {
	fn default() -> Self {
		AccountMeta {
			name: String::new(),
			meta: "{}".to_owned(),
			uuid: None,
		}
	}
}

impl AccountProvider {
	/// Creates new account provider.
	pub fn new(sstore: Box<SecretStore>) -> Self {
		AccountProvider {
			unlocked: Mutex::new(HashMap::new()),
			sstore: sstore,
		}
	}

	/// Creates not disk backed provider.
	pub fn transient_provider() -> Self {
		AccountProvider {
			unlocked: Mutex::new(HashMap::new()),
			sstore: Box::new(EthStore::open(Box::new(NullDir::default())).unwrap())
		}
	}

	/// Creates new random account.
	pub fn new_account(&self, password: &str) -> Result<H160, Error> {
		let secret = Random.generate().unwrap().secret().clone();
		let address = try!(self.sstore.insert_account(secret, password));
		Ok(Address::from(address).into())
	}

	/// Inserts new account into underlying store.
	/// Does not unlock account!
	pub fn insert_account<S>(&self, secret: S, password: &str) -> Result<H160, Error> where Secret: From<S> {
		let s = Secret::from(secret);
		let address = try!(self.sstore.insert_account(s.into(), password));
		Ok(Address::from(address).into())
	}

	/// Returns addresses of all accounts.
	pub fn accounts(&self) -> Result<Vec<H160>, Error> {
		let accounts = try!(self.sstore.accounts()).into_iter().map(|a| H160(a.into())).collect();
		Ok(accounts)
	}

	/// Returns each account along with name and meta.
	pub fn accounts_info(&self) -> Result<HashMap<H160, AccountMeta>, Error> {
		let r: HashMap<H160, AccountMeta> = try!(self.sstore.accounts())
			.into_iter()
			.map(|a| (H160(a.clone().into()), self.account_meta(a).unwrap_or_else(|_| Default::default())))
			.collect();
		Ok(r)
	}

	/// Returns each account along with name and meta.
	pub fn account_meta<A>(&self, account: A) -> Result<AccountMeta, Error> where Address: From<A> {
		let account = Address::from(account).into();
		Ok(AccountMeta {
			name: try!(self.sstore.name(&account)),
			meta: try!(self.sstore.meta(&account)),
			uuid: self.sstore.uuid(&account).ok().map(Into::into),	// allowed to not have a UUID
		})
	}

	/// Returns each account along with name and meta.
	pub fn set_account_name<A>(&self, account: A, name: String) -> Result<(), Error> where Address: From<A> {
		let account = Address::from(account).into();
		try!(self.sstore.set_name(&account, name));
		Ok(())
	}

	/// Returns each account along with name and meta.
	pub fn set_account_meta<A>(&self, account: A, meta: String) -> Result<(), Error> where Address: From<A> {
		let account = Address::from(account).into();
		try!(self.sstore.set_meta(&account, meta));
		Ok(())
	}

	/// Helper method used for unlocking accounts.
	fn unlock_account<A>(&self, account: A, password: String, unlock: Unlock) -> Result<(), Error> where Address: From<A> {
		let a = Address::from(account);
		let account = a.into();
		// verify password by signing dump message
		// result may be discarded
		let _ = try!(self.sstore.sign(&account, &password, &Default::default()));

		// check if account is already unlocked pernamently, if it is, do nothing
		let mut unlocked = self.unlocked.lock();
		if let Some(data) = unlocked.get(&account) {
			if let Unlock::Perm = data.unlock {
				return Ok(())
			}
		}

		let data = AccountData {
			unlock: unlock,
			password: password,
		};

		unlocked.insert(account, data);
		Ok(())
	}

	/// Unlocks account permanently.
	pub fn unlock_account_permanently<A>(&self, account: A, password: String) -> Result<(), Error> where Address: From<A> {
		self.unlock_account(account, password, Unlock::Perm)
	}

	/// Unlocks account temporarily (for one signing).
	pub fn unlock_account_temporarily<A>(&self, account: A, password: String) -> Result<(), Error> where Address: From<A> {
		self.unlock_account(account, password, Unlock::Temp)
	}

	/// Unlocks account temporarily with a timeout.
	pub fn unlock_account_timed<A>(&self, account: A, password: String, duration_ms: u32) -> Result<(), Error> where Address: From<A> {
		self.unlock_account(account, password, Unlock::Timed((Instant::now(), duration_ms)))
	}

	/// Checks if given account is unlocked
	pub fn is_unlocked<A>(&self, account: A) -> bool where Address: From<A> {
		let account = Address::from(account).into();
		let unlocked = self.unlocked.lock();
		unlocked.get(&account).is_some()
	}

	/// Signs the message. Account must be unlocked.
	pub fn sign<A, M>(&self, account: A, message: M) -> Result<H520, Error> where Address: From<A>, Message: From<M> {
		let account = Address::from(account).into();
		let message = Message::from(message).into();

		let data = {
			let mut unlocked = self.unlocked.lock();
			let data = try!(unlocked.get(&account).ok_or(Error::NotUnlocked)).clone();
			if let Unlock::Temp = data.unlock {
				unlocked.remove(&account).expect("data exists: so key must exist: qed");
			}
			if let Unlock::Timed((ref start, ref duration)) = data.unlock {
				if start.elapsed() > Duration::from_millis(*duration as u64) {
					unlocked.remove(&account).expect("data exists: so key must exist: qed");
					return Err(Error::NotUnlocked);
				}
			}
			data
		};

		let signature = try!(self.sstore.sign(&account, &data.password, &message));
		Ok(H520(signature.into()))
	}

	/// Unlocks an account, signs the message, and locks it again.
	pub fn sign_with_password<A, M>(&self, account: A, password: String, message: M) -> Result<H520, Error> where Address: From<A>, Message: From<M> {
		let account = Address::from(account).into();
		let message = Message::from(message).into();
		let signature = try!(self.sstore.sign(&account, &password, &message));
		Ok(H520(signature.into()))
	}
}

#[cfg(test)]
mod tests {
	use super::AccountProvider;
	use ethstore::ethkey::{Generator, Random};
	use std::time::Duration;

	#[test]
	fn unlock_account_temp() {
		let kp = Random.generate().unwrap();
		let ap = AccountProvider::transient_provider();
		assert!(ap.insert_account(kp.secret().clone(), "test").is_ok());
		assert!(ap.unlock_account_temporarily(kp.address(), "test1".into()).is_err());
		assert!(ap.unlock_account_temporarily(kp.address(), "test".into()).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_err());
	}

	#[test]
	fn unlock_account_perm() {
		let kp = Random.generate().unwrap();
		let ap = AccountProvider::transient_provider();
		assert!(ap.insert_account(kp.secret().clone(), "test").is_ok());
		assert!(ap.unlock_account_permanently(kp.address(), "test1".into()).is_err());
		assert!(ap.unlock_account_permanently(kp.address(), "test".into()).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_ok());
		assert!(ap.unlock_account_temporarily(kp.address(), "test".into()).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_ok());
	}

	#[test]
	fn unlock_account_timer() {
		let kp = Random.generate().unwrap();
		let ap = AccountProvider::transient_provider();
		assert!(ap.insert_account(kp.secret().clone(), "test").is_ok());
		assert!(ap.unlock_account_timed(kp.address(), "test1".into(), 2000).is_err());
		assert!(ap.unlock_account_timed(kp.address(), "test".into(), 2000).is_ok());
		assert!(ap.sign(kp.address(), [0u8; 32]).is_ok());
		::std::thread::sleep(Duration::from_millis(2000));
		assert!(ap.sign(kp.address(), [0u8; 32]).is_err());
	}
}
