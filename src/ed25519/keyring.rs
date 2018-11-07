use signatory::ed25519::Signature;
use std::{collections::BTreeMap, sync::RwLock};

use super::{ConsensusKey, PublicKey, Signer};
use config::provider::ProviderConfig;
use error::{KmsError, KmsErrorKind::*};

use super::signer::softsign;
#[cfg(feature = "yubihsm")]
use super::signer::yubihsm;

lazy_static! {
    static ref GLOBAL_KEYRING: RwLock<KeyRing> = RwLock::new(KeyRing(BTreeMap::default()));
}

pub struct KeyRing(BTreeMap<PublicKey, Signer>);

impl KeyRing {
    /// Create a keyring from the given provider configuration
    pub fn load_from_config(config: &ProviderConfig) -> Result<(), KmsError> {
        let mut keyring = GLOBAL_KEYRING.write().unwrap();

        // Clear the current global keyring
        if !keyring.0.is_empty() {
            info!("[keyring:*] Clearing keyring");
            keyring.0.clear();
        }

        #[cfg(feature = "softsign")]
        softsign::init(&mut keyring, &config.softsign)?;

        #[cfg(feature = "yubihsm")]
        yubihsm::init(&mut keyring, &config.yubihsm)?;

        if keyring.0.is_empty() {
            fail!(ConfigError, "no signing keys configured!")
        } else {
            Ok(())
        }
    }

    /// Add a key to the keyring, returning an error if we already have a
    /// signer registered for the given public key
    pub(super) fn add(&mut self, public_key: PublicKey, signer: Signer) -> Result<(), KmsError> {
        info!(
            "[keyring:{}:{}] added validator key {}",
            signer.provider_name,
            signer.key_id,
            ConsensusKey(public_key)
        );

        if let Some(other) = self.0.insert(public_key, signer) {
            fail!(
                InvalidKey,
                "duplicate signer for {}: {}:{}",
                ConsensusKey(public_key),
                other.provider_name,
                other.key_id
            )
        } else {
            Ok(())
        }
    }

    pub fn get_only_signing_pubkey() -> Result<PublicKey, KmsError> {
        let keyring = GLOBAL_KEYRING.read().unwrap();

        let mut keys = keyring.0.keys();
        if keys.len() > 1 {
            fail!(InvalidKey, "expected only one key in keyring");
        }
        Ok(*keys.next().unwrap())
    }

    /// Sign a message using the secret key associated with the given public key
    /// (if it is in our keyring)
    pub fn sign(public_key: Option<&PublicKey>, msg: &[u8]) -> Result<Signature, KmsError> {
        let keyring = GLOBAL_KEYRING.read().unwrap();
        let signer: &Signer = match public_key {
            Some(public_key) => keyring
                .0
                .get(public_key)
                .ok_or_else(|| err!(InvalidKey, "not in keyring: {}", ConsensusKey(*public_key)))?,
            None => {
                let mut vals = keyring.0.values();

                if vals.len() > 1 {
                    fail!(SigningError, "expected only one key in keyring");
                } else {
                    vals.next()
                        .ok_or_else(|| err!(InvalidKey, "could not get only signer"))?
                }
            }
        };

        signer.sign(msg)
    }
}