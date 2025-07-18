use tfhe_versionable::Versionize;

use crate::backward_compatibility::config::ConfigVersions;
use crate::high_level_api::keys::IntegerConfig;
use crate::shortint::parameters::list_compression::CompressionParameters;
use crate::shortint::parameters::{NoiseSquashingCompressionParameters, NoiseSquashingParameters};

/// The config type
#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize, Versionize)]
#[versionize(ConfigVersions)]
pub struct Config {
    pub(crate) inner: IntegerConfig,
}

impl Config {
    #[cfg(feature = "hpu")]
    pub fn from_hpu_device(hpu_device: &tfhe_hpu_backend::prelude::HpuDevice) -> Self {
        let pbs_params =
            crate::shortint::parameters::KeySwitch32PBSParameters::from(hpu_device.params());
        ConfigBuilder::with_custom_parameters(pbs_params).build()
    }

    pub fn public_key_encryption_parameters(
        &self,
    ) -> Result<crate::shortint::parameters::CompactPublicKeyEncryptionParameters, crate::Error>
    {
        self.inner.public_key_encryption_parameters()
    }
}

/// The builder to create your config
///
/// The configuration is needed to select parameters you wish to use for these types
/// (whether it is the default parameters or some custom parameters).
/// The default parameters are specialized for GPU execution
/// in case the gpu feature is activated.
#[derive(Clone)]
pub struct ConfigBuilder {
    config: Config,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            config: Config {
                inner: IntegerConfig::default(),
            },
        }
    }
}

impl ConfigBuilder {
    pub fn enable_compression(mut self, compression_parameters: CompressionParameters) -> Self {
        self.config.inner.enable_compression(compression_parameters);

        self
    }

    pub fn enable_noise_squashing(
        mut self,
        noise_squashing_parameters: NoiseSquashingParameters,
    ) -> Self {
        self.config
            .inner
            .enable_noise_squashing(noise_squashing_parameters);

        self
    }

    /// Enable the generation of keys needed for [crate::CompressedSquashedNoiseCiphertextList]
    ///
    /// # Note
    ///
    /// This requires nose squashing to be enabled first via [Self::enable_noise_squashing]
    pub fn enable_noise_squashing_compression(
        mut self,
        compression_parameters: NoiseSquashingCompressionParameters,
    ) -> Self {
        assert_ne!(
            self.config.inner.noise_squashing_parameters, None,
            "Noise squashing must be enabled first"
        );
        self.config
            .inner
            .enable_noise_squashing_compression(compression_parameters);
        self
    }

    pub fn with_custom_parameters<P>(block_parameters: P) -> Self
    where
        P: Into<crate::shortint::atomic_pattern::AtomicPatternParameters>,
    {
        Self {
            config: Config {
                inner: IntegerConfig::new(block_parameters.into()),
            },
        }
    }

    pub fn use_dedicated_compact_public_key_parameters(
        mut self,
        dedicated_compact_public_key_parameters: (
            crate::shortint::parameters::CompactPublicKeyEncryptionParameters,
            crate::shortint::parameters::ShortintKeySwitchingParameters,
        ),
    ) -> Self {
        self.config.inner.dedicated_compact_public_key_parameters =
            Some(dedicated_compact_public_key_parameters);
        self
    }

    pub fn use_custom_parameters<P>(mut self, block_parameters: P) -> Self
    where
        P: Into<crate::shortint::atomic_pattern::AtomicPatternParameters>,
    {
        self.config.inner = IntegerConfig::new(block_parameters.into());
        self
    }

    pub fn build(self) -> Config {
        self.config
    }
}

impl From<ConfigBuilder> for Config {
    fn from(builder: ConfigBuilder) -> Self {
        builder.build()
    }
}
