use super::keys::CudaNoiseSquashingKey;
use crate::core_crypto::gpu::lwe_bootstrap_key::CudaLweBootstrapKey;
use crate::core_crypto::gpu::CudaStreams;
use crate::integer::noise_squashing::CompressedNoiseSquashingKey;
use crate::shortint::server_key::CompressedModulusSwitchConfiguration;

impl CompressedNoiseSquashingKey {
    pub fn decompress_to_cuda(&self, streams: &CudaStreams) -> CudaNoiseSquashingKey {
        let std_bsk = self
            .key
            .bootstrapping_key()
            .as_view()
            .par_decompress_into_lwe_bootstrap_key();

        let ms_noise_reduction_key = match self.key.modulus_switch_noise_reduction_key() {
            CompressedModulusSwitchConfiguration::Standard => None,
            CompressedModulusSwitchConfiguration::DriftTechniqueNoiseReduction(
                modulus_switch_noise_reduction_key,
            ) => Some(modulus_switch_noise_reduction_key.decompress()),
            CompressedModulusSwitchConfiguration::CenteredMeanNoiseReduction => {
                panic!("Centered MS not supportred on GPU")
            }
        };

        let bootstrapping_key = CudaLweBootstrapKey::from_lwe_bootstrap_key(
            &std_bsk,
            ms_noise_reduction_key.as_ref(),
            streams,
        );

        CudaNoiseSquashingKey {
            bootstrapping_key,
            message_modulus: self.key.message_modulus(),
            carry_modulus: self.key.carry_modulus(),
            output_ciphertext_modulus: self.key.output_ciphertext_modulus(),
        }
    }
}
