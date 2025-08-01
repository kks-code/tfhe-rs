use super::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(not(tarpaulin))]
const NB_TESTS: usize = 10;
#[cfg(not(tarpaulin))]
// Divided by two compared to other tests, we are running the algorithm twice for determinism
const NB_TESTS_LIGHT: usize = 5;
#[cfg(tarpaulin)]
const NB_TESTS: usize = 1;
#[cfg(tarpaulin)]
const NB_TESTS_LIGHT: usize = 1;

pub fn generate_keys<
    Scalar: UnsignedTorus + Sync + Send + CastFrom<usize> + CastInto<usize> + Serialize + DeserializeOwned,
>(
    params: MultiBitTestParams<Scalar>,
    rsc: &mut TestResources,
) -> MultiBitBootstrapKeys<Scalar> {
    // Keygen is a bit slow on this one so we keep it out of the testing loop

    // Create the LweSecretKey
    let input_lwe_secret_key = allocate_and_generate_new_binary_lwe_secret_key(
        params.input_lwe_dimension,
        &mut rsc.secret_random_generator,
    );
    let output_glwe_secret_key = allocate_and_generate_new_binary_glwe_secret_key(
        params.glwe_dimension,
        params.polynomial_size,
        &mut rsc.secret_random_generator,
    );
    let output_lwe_secret_key = output_glwe_secret_key.clone().into_lwe_secret_key();

    let mut bsk = LweMultiBitBootstrapKey::new(
        Scalar::ZERO,
        params.glwe_dimension.to_glwe_size(),
        params.polynomial_size,
        params.decomp_base_log,
        params.decomp_level_count,
        params.input_lwe_dimension,
        params.grouping_factor,
        params.ciphertext_modulus,
    );

    par_generate_lwe_multi_bit_bootstrap_key(
        &input_lwe_secret_key,
        &output_glwe_secret_key,
        &mut bsk,
        params.glwe_noise_distribution,
        &mut rsc.encryption_random_generator,
    );

    let mut fbsk = FourierLweMultiBitBootstrapKey::new(
        params.input_lwe_dimension,
        params.glwe_dimension.to_glwe_size(),
        params.polynomial_size,
        params.decomp_base_log,
        params.decomp_level_count,
        params.grouping_factor,
    );

    par_convert_standard_lwe_multi_bit_bootstrap_key_to_fourier(&bsk, &mut fbsk);

    MultiBitBootstrapKeys {
        small_lwe_sk: input_lwe_secret_key,
        big_lwe_sk: output_lwe_secret_key,
        bsk,
        fbsk,
    }
}

fn lwe_encrypt_multi_bit_pbs_decrypt_custom_mod<Scalar>(params: MultiBitTestParams<Scalar>)
where
    Scalar: UnsignedTorus
        + Sync
        + Send
        + CastFrom<usize>
        + CastInto<usize>
        + Serialize
        + DeserializeOwned,
    MultiBitTestParams<Scalar>: KeyCacheAccess<Keys = MultiBitBootstrapKeys<Scalar>>,
{
    let lwe_noise_distribution = params.lwe_noise_distribution;
    let ciphertext_modulus = params.ciphertext_modulus;
    let message_modulus_log = params.message_modulus_log;
    let msg_modulus = Scalar::ONE.shl(message_modulus_log.0);
    let encoding_with_padding = get_encoding_with_padding(ciphertext_modulus);
    let glwe_dimension = params.glwe_dimension;
    let polynomial_size = params.polynomial_size;
    let thread_count = params.thread_count;

    let mut rsc = TestResources::new();

    let f = |x: Scalar| x;

    let delta: Scalar = encoding_with_padding / msg_modulus;
    let mut msg = msg_modulus;

    let accumulator = generate_programmable_bootstrap_glwe_lut(
        polynomial_size,
        glwe_dimension.to_glwe_size(),
        msg_modulus.cast_into(),
        ciphertext_modulus,
        delta,
        f,
    );

    assert!(check_encrypted_content_respects_mod(
        &accumulator,
        ciphertext_modulus
    ));

    let mut keys_gen = |params| generate_keys(params, &mut rsc);

    let keys = gen_keys_or_get_from_cache_if_enabled(params, &mut keys_gen);
    let (input_lwe_secret_key, output_lwe_secret_key, bsk, fbsk) =
        (keys.small_lwe_sk, keys.big_lwe_sk, keys.bsk, keys.fbsk);

    assert!(check_encrypted_content_respects_mod(
        &*bsk,
        ciphertext_modulus
    ));

    while msg != Scalar::ZERO {
        msg = msg.wrapping_sub(Scalar::ONE);
        for _ in 0..NB_TESTS {
            let plaintext = Plaintext(msg * delta);

            let lwe_ciphertext_in = allocate_and_encrypt_new_lwe_ciphertext(
                &input_lwe_secret_key,
                plaintext,
                lwe_noise_distribution,
                ciphertext_modulus,
                &mut rsc.encryption_random_generator,
            );

            assert!(check_encrypted_content_respects_mod(
                &lwe_ciphertext_in,
                ciphertext_modulus
            ));

            let mut out_pbs_ct = LweCiphertext::new(
                Scalar::ZERO,
                output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                ciphertext_modulus,
            );

            multi_bit_programmable_bootstrap_lwe_ciphertext(
                &lwe_ciphertext_in,
                &mut out_pbs_ct,
                &accumulator,
                &fbsk,
                thread_count,
                false,
            );

            assert!(check_encrypted_content_respects_mod(
                &out_pbs_ct,
                ciphertext_modulus
            ));

            let decrypted = decrypt_lwe_ciphertext(&output_lwe_secret_key, &out_pbs_ct);

            let decoded = round_decode(decrypted.0, delta) % msg_modulus;

            assert_eq!(decoded, f(msg));
        }

        // In coverage, we break after one while loop iteration, changing message values does not
        // yield higher coverage
        #[cfg(tarpaulin)]
        break;
    }
}

fn lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod<Scalar>(
    params: MultiBitTestParams<Scalar>,
) where
    Scalar: UnsignedTorus
        + Sync
        + Send
        + CastFrom<usize>
        + CastInto<usize>
        + Serialize
        + DeserializeOwned,
    MultiBitTestParams<Scalar>: KeyCacheAccess<Keys = MultiBitBootstrapKeys<Scalar>>,
{
    let lwe_noise_distribution = params.lwe_noise_distribution;
    let ciphertext_modulus = params.ciphertext_modulus;
    let message_modulus_log = params.message_modulus_log;
    let msg_modulus = Scalar::ONE.shl(message_modulus_log.0);
    let encoding_with_padding = get_encoding_with_padding(ciphertext_modulus);
    let glwe_dimension = params.glwe_dimension;
    let polynomial_size = params.polynomial_size;
    let thread_count = params.thread_count;

    let mut rsc = TestResources::new();

    let f = |x: Scalar| x;

    let delta: Scalar = encoding_with_padding / msg_modulus;
    let mut msg = msg_modulus;

    let accumulator = generate_programmable_bootstrap_glwe_lut(
        polynomial_size,
        glwe_dimension.to_glwe_size(),
        msg_modulus.cast_into(),
        ciphertext_modulus,
        delta,
        f,
    );

    assert!(check_encrypted_content_respects_mod(
        &accumulator,
        ciphertext_modulus
    ));

    let mut keys_gen = |params| generate_keys(params, &mut rsc);

    let keys = gen_keys_or_get_from_cache_if_enabled(params, &mut keys_gen);
    let (input_lwe_secret_key, output_lwe_secret_key, bsk, fbsk) =
        (keys.small_lwe_sk, keys.big_lwe_sk, keys.bsk, keys.fbsk);

    assert!(check_encrypted_content_respects_mod(
        &*bsk,
        ciphertext_modulus
    ));

    while msg != Scalar::ZERO {
        msg = msg.wrapping_sub(Scalar::ONE);
        for _ in 0..NB_TESTS_LIGHT {
            let plaintext = Plaintext(msg * delta);

            let lwe_ciphertext_in = allocate_and_encrypt_new_lwe_ciphertext(
                &input_lwe_secret_key,
                plaintext,
                lwe_noise_distribution,
                ciphertext_modulus,
                &mut rsc.encryption_random_generator,
            );

            assert!(check_encrypted_content_respects_mod(
                &lwe_ciphertext_in,
                ciphertext_modulus
            ));

            let out_pbs_ct = {
                let mut out_pbs_ct = LweCiphertext::new(
                    Scalar::ZERO,
                    output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                    ciphertext_modulus,
                );

                multi_bit_programmable_bootstrap_lwe_ciphertext(
                    &lwe_ciphertext_in,
                    &mut out_pbs_ct,
                    &accumulator,
                    &fbsk,
                    thread_count,
                    true,
                );

                assert!(check_encrypted_content_respects_mod(
                    &out_pbs_ct,
                    ciphertext_modulus
                ));

                let decrypted = decrypt_lwe_ciphertext(&output_lwe_secret_key, &out_pbs_ct);

                let decoded = round_decode(decrypted.0, delta) % msg_modulus;

                assert_eq!(decoded, f(msg));

                out_pbs_ct
            };

            let out_pbs_ct_other = {
                let mut out_pbs_ct = LweCiphertext::new(
                    Scalar::ZERO,
                    output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                    ciphertext_modulus,
                );

                multi_bit_programmable_bootstrap_lwe_ciphertext(
                    &lwe_ciphertext_in,
                    &mut out_pbs_ct,
                    &accumulator,
                    &fbsk,
                    thread_count,
                    true,
                );

                out_pbs_ct
            };

            assert_eq!(out_pbs_ct_other, out_pbs_ct);
        }

        // In coverage, we break after one while loop iteration, changing message values does not
        // yield higher coverage
        #[cfg(tarpaulin)]
        break;
    }
}

fn lwe_encrypt_std_multi_bit_pbs_decrypt_custom_mod<Scalar>(params: MultiBitTestParams<Scalar>)
where
    Scalar: UnsignedTorus
        + Sync
        + Send
        + CastFrom<usize>
        + CastInto<usize>
        + Serialize
        + DeserializeOwned,
    MultiBitTestParams<Scalar>: KeyCacheAccess<Keys = MultiBitBootstrapKeys<Scalar>>,
{
    let lwe_noise_distribution = params.lwe_noise_distribution;
    let ciphertext_modulus = params.ciphertext_modulus;
    let message_modulus_log = params.message_modulus_log;
    let msg_modulus = Scalar::ONE.shl(message_modulus_log.0);
    let encoding_with_padding = get_encoding_with_padding(ciphertext_modulus);
    let glwe_dimension = params.glwe_dimension;
    let polynomial_size = params.polynomial_size;
    let thread_count = params.thread_count;

    let mut rsc = TestResources::new();

    let f = |x: Scalar| x;

    let delta: Scalar = encoding_with_padding / msg_modulus;
    let mut msg = msg_modulus;

    let accumulator = generate_programmable_bootstrap_glwe_lut(
        polynomial_size,
        glwe_dimension.to_glwe_size(),
        msg_modulus.cast_into(),
        ciphertext_modulus,
        delta,
        f,
    );

    assert!(check_encrypted_content_respects_mod(
        &accumulator,
        ciphertext_modulus
    ));

    let mut keys_gen = |params| generate_keys(params, &mut rsc);

    let keys = gen_keys_or_get_from_cache_if_enabled(params, &mut keys_gen);
    let (input_lwe_secret_key, output_lwe_secret_key, bsk) =
        (keys.small_lwe_sk, keys.big_lwe_sk, keys.bsk);

    assert!(check_encrypted_content_respects_mod(
        &*bsk,
        ciphertext_modulus
    ));

    while msg != Scalar::ZERO {
        msg = msg.wrapping_sub(Scalar::ONE);
        for _ in 0..NB_TESTS {
            let plaintext = Plaintext(msg * delta);

            let lwe_ciphertext_in = allocate_and_encrypt_new_lwe_ciphertext(
                &input_lwe_secret_key,
                plaintext,
                lwe_noise_distribution,
                ciphertext_modulus,
                &mut rsc.encryption_random_generator,
            );

            assert!(check_encrypted_content_respects_mod(
                &lwe_ciphertext_in,
                ciphertext_modulus
            ));

            let mut out_pbs_ct = LweCiphertext::new(
                Scalar::ZERO,
                output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                ciphertext_modulus,
            );

            std_multi_bit_programmable_bootstrap_lwe_ciphertext(
                &lwe_ciphertext_in,
                &mut out_pbs_ct,
                &accumulator,
                &bsk,
                thread_count,
                false,
            );

            assert!(check_encrypted_content_respects_mod(
                &out_pbs_ct,
                ciphertext_modulus
            ));

            let decrypted = decrypt_lwe_ciphertext(&output_lwe_secret_key, &out_pbs_ct);

            let decoded = round_decode(decrypted.0, delta) % msg_modulus;

            assert_eq!(decoded, f(msg));
        }

        // In coverage, we break after one while loop iteration, changing message values does not
        // yield higher coverage
        #[cfg(tarpaulin)]
        break;
    }
}

fn std_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod<Scalar>(
    params: MultiBitTestParams<Scalar>,
) where
    Scalar: UnsignedTorus
        + Sync
        + Send
        + CastFrom<usize>
        + CastInto<usize>
        + Serialize
        + DeserializeOwned,
    MultiBitTestParams<Scalar>: KeyCacheAccess<Keys = MultiBitBootstrapKeys<Scalar>>,
{
    let lwe_noise_distribution = params.lwe_noise_distribution;
    let ciphertext_modulus = params.ciphertext_modulus;
    let message_modulus_log = params.message_modulus_log;
    let msg_modulus = Scalar::ONE.shl(message_modulus_log.0);
    let encoding_with_padding = get_encoding_with_padding(ciphertext_modulus);
    let glwe_dimension = params.glwe_dimension;
    let polynomial_size = params.polynomial_size;
    let thread_count = params.thread_count;

    let mut rsc = TestResources::new();

    let f = |x: Scalar| x;

    let delta: Scalar = encoding_with_padding / msg_modulus;
    let mut msg = msg_modulus;

    let accumulator = generate_programmable_bootstrap_glwe_lut(
        polynomial_size,
        glwe_dimension.to_glwe_size(),
        msg_modulus.cast_into(),
        ciphertext_modulus,
        delta,
        f,
    );

    assert!(check_encrypted_content_respects_mod(
        &accumulator,
        ciphertext_modulus
    ));

    let mut keys_gen = |params| generate_keys(params, &mut rsc);

    let keys = gen_keys_or_get_from_cache_if_enabled(params, &mut keys_gen);
    let (input_lwe_secret_key, output_lwe_secret_key, bsk) =
        (keys.small_lwe_sk, keys.big_lwe_sk, keys.bsk);

    assert!(check_encrypted_content_respects_mod(
        &*bsk,
        ciphertext_modulus
    ));

    while msg != Scalar::ZERO {
        msg = msg.wrapping_sub(Scalar::ONE);
        for _ in 0..NB_TESTS {
            let plaintext = Plaintext(msg * delta);

            let lwe_ciphertext_in = allocate_and_encrypt_new_lwe_ciphertext(
                &input_lwe_secret_key,
                plaintext,
                lwe_noise_distribution,
                ciphertext_modulus,
                &mut rsc.encryption_random_generator,
            );

            assert!(check_encrypted_content_respects_mod(
                &lwe_ciphertext_in,
                ciphertext_modulus
            ));

            let out_pbs_ct = {
                let mut out_pbs_ct = LweCiphertext::new(
                    Scalar::ZERO,
                    output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                    ciphertext_modulus,
                );

                std_multi_bit_programmable_bootstrap_lwe_ciphertext(
                    &lwe_ciphertext_in,
                    &mut out_pbs_ct,
                    &accumulator,
                    &bsk,
                    thread_count,
                    true,
                );

                assert!(check_encrypted_content_respects_mod(
                    &out_pbs_ct,
                    ciphertext_modulus
                ));

                let decrypted = decrypt_lwe_ciphertext(&output_lwe_secret_key, &out_pbs_ct);

                let decoded = round_decode(decrypted.0, delta) % msg_modulus;

                assert_eq!(decoded, f(msg));

                out_pbs_ct
            };

            let out_pbs_ct_other = {
                let mut out_pbs_ct = LweCiphertext::new(
                    Scalar::ZERO,
                    output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                    ciphertext_modulus,
                );

                std_multi_bit_programmable_bootstrap_lwe_ciphertext(
                    &lwe_ciphertext_in,
                    &mut out_pbs_ct,
                    &accumulator,
                    &bsk,
                    thread_count,
                    true,
                );

                out_pbs_ct
            };

            assert_eq!(out_pbs_ct_other, out_pbs_ct);
        }

        // In coverage, we break after one while loop iteration, changing message values does not
        // yield higher coverage
        #[cfg(tarpaulin)]
        break;
    }
}

#[test]
pub fn test_lwe_encrypt_multi_bit_pbs_decrypt_factor_2_thread_5_native_mod() {
    lwe_encrypt_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_2_PARAMS);
    lwe_encrypt_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_3_3_2_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_multi_bit_pbs_decrypt_factor_3_thread_12_native_mod() {
    lwe_encrypt_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_3_PARAMS);
    lwe_encrypt_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_3_3_3_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_multi_bit_pbs_decrypt_factor_2_thread_5_custom_mod() {
    lwe_encrypt_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_2_CUSTOM_MOD_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_multi_bit_pbs_decrypt_factor_3_thread_12_custom_mod() {
    lwe_encrypt_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_3_CUSTOM_MOD_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_2_thread_5_native_mod() {
    lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_2_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_3_thread_12_native_mod() {
    lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_3_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_2_thread_5_custom_mod() {
    lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(
        MULTI_BIT_2_2_2_CUSTOM_MOD_PARAMS,
    );
}

#[test]
pub fn test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_3_thread_12_custom_mod() {
    lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(
        MULTI_BIT_2_2_3_CUSTOM_MOD_PARAMS,
    );
}

#[test]
pub fn test_lwe_encrypt_std_multi_bit_pbs_decrypt_factor_2_thread_5_native_mod() {
    lwe_encrypt_std_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_2_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_std_multi_bit_pbs_decrypt_factor_3_thread_12_native_mod() {
    lwe_encrypt_std_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_3_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_std_multi_bit_pbs_decrypt_factor_2_thread_5_custom_mod() {
    lwe_encrypt_std_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_2_CUSTOM_MOD_PARAMS);
}

#[test]
pub fn test_lwe_encrypt_std_multi_bit_pbs_decrypt_factor_3_thread_12_custom_mod() {
    lwe_encrypt_std_multi_bit_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_3_CUSTOM_MOD_PARAMS);
}

#[test]
pub fn std_test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_2_thread_5_native_mod() {
    std_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_2_PARAMS);
}

#[test]
pub fn std_test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_3_thread_12_native_mod() {
    std_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(MULTI_BIT_2_2_3_PARAMS);
}

#[test]
pub fn std_test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_2_thread_5_custom_mod() {
    std_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(
        MULTI_BIT_2_2_2_CUSTOM_MOD_PARAMS,
    );
}

#[test]
pub fn std_test_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_factor_3_thread_12_custom_mod() {
    std_lwe_encrypt_multi_bit_deterministic_pbs_decrypt_custom_mod::<u64>(
        MULTI_BIT_2_2_3_CUSTOM_MOD_PARAMS,
    );
}

fn std_lwe_encrypt_multi_bit_deterministic_pbs_f128_decrypt_custom_mod(
    params: MultiBitTestParams<u128>,
) {
    let lwe_noise_distribution = params.lwe_noise_distribution;
    let ciphertext_modulus = params.ciphertext_modulus;
    let message_modulus_log = params.message_modulus_log;
    let msg_modulus = 1u128 << message_modulus_log.0;
    let encoding_with_padding = get_encoding_with_padding(ciphertext_modulus);
    let glwe_dimension = params.glwe_dimension;
    let polynomial_size = params.polynomial_size;
    let thread_count = params.thread_count;

    let mut rsc = TestResources::new();

    let f = |x| x;

    let delta = encoding_with_padding / msg_modulus;
    let mut msg = msg_modulus;

    let accumulator = generate_programmable_bootstrap_glwe_lut(
        polynomial_size,
        glwe_dimension.to_glwe_size(),
        msg_modulus.cast_into(),
        ciphertext_modulus,
        delta,
        f,
    );

    assert!(check_encrypted_content_respects_mod(
        &accumulator,
        ciphertext_modulus
    ));

    // Create the LweSecretKey
    let input_lwe_secret_key = allocate_and_generate_new_binary_lwe_secret_key(
        params.input_lwe_dimension,
        &mut rsc.secret_random_generator,
    );
    let output_glwe_secret_key = allocate_and_generate_new_binary_glwe_secret_key(
        params.glwe_dimension,
        params.polynomial_size,
        &mut rsc.secret_random_generator,
    );

    let mut bsk = LweMultiBitBootstrapKey::new(
        0u128,
        params.glwe_dimension.to_glwe_size(),
        params.polynomial_size,
        params.decomp_base_log,
        params.decomp_level_count,
        params.input_lwe_dimension,
        params.grouping_factor,
        params.ciphertext_modulus,
    );

    par_generate_lwe_multi_bit_bootstrap_key(
        &input_lwe_secret_key,
        &output_glwe_secret_key,
        &mut bsk,
        params.glwe_noise_distribution,
        &mut rsc.encryption_random_generator,
    );

    let output_lwe_secret_key = output_glwe_secret_key.into_lwe_secret_key();

    assert!(check_encrypted_content_respects_mod(
        &*bsk,
        ciphertext_modulus
    ));

    while msg != 0 {
        msg = msg.wrapping_sub(1);
        for _ in 0..NB_TESTS {
            let plaintext = Plaintext(msg * delta);

            let lwe_ciphertext_in = allocate_and_encrypt_new_lwe_ciphertext(
                &input_lwe_secret_key,
                plaintext,
                lwe_noise_distribution,
                ciphertext_modulus,
                &mut rsc.encryption_random_generator,
            );

            assert!(check_encrypted_content_respects_mod(
                &lwe_ciphertext_in,
                ciphertext_modulus
            ));

            let out_pbs_ct = {
                let mut out_pbs_ct = LweCiphertext::new(
                    0,
                    output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                    ciphertext_modulus,
                );

                std_multi_bit_programmable_bootstrap_f128_lwe_ciphertext(
                    &lwe_ciphertext_in,
                    &mut out_pbs_ct,
                    &accumulator,
                    &bsk,
                    thread_count,
                    true,
                );

                assert!(check_encrypted_content_respects_mod(
                    &out_pbs_ct,
                    ciphertext_modulus
                ));

                let decrypted = decrypt_lwe_ciphertext(&output_lwe_secret_key, &out_pbs_ct);

                let decoded = round_decode(decrypted.0, delta) % msg_modulus;

                assert_eq!(decoded, f(msg));

                out_pbs_ct
            };

            let out_pbs_ct_other = {
                let mut out_pbs_ct = LweCiphertext::new(
                    0,
                    output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                    ciphertext_modulus,
                );

                std_multi_bit_programmable_bootstrap_f128_lwe_ciphertext(
                    &lwe_ciphertext_in,
                    &mut out_pbs_ct,
                    &accumulator,
                    &bsk,
                    thread_count,
                    true,
                );

                out_pbs_ct
            };

            assert_eq!(out_pbs_ct_other, out_pbs_ct);
        }

        // In coverage, we break after one while loop iteration, changing message values does not
        // yield higher coverage
        #[cfg(tarpaulin)]
        break;
    }
}

#[test]
pub fn test_std_lwe_encrypt_multi_bit_deterministic_pbs_f128_decrypt_factor_3_thread_12_native_mod()
{
    std_lwe_encrypt_multi_bit_deterministic_pbs_f128_decrypt_custom_mod(
        MULTI_BIT_2_2_3_PARAMS_U128,
    );
}

fn lwe_encrypt_multi_bit_deterministic_pbs_f128_decrypt_custom_mod(
    params: MultiBitTestParams<u128>,
) {
    let lwe_noise_distribution = params.lwe_noise_distribution;
    let ciphertext_modulus = params.ciphertext_modulus;
    let message_modulus_log = params.message_modulus_log;
    let msg_modulus = 1u128 << message_modulus_log.0;
    let encoding_with_padding = get_encoding_with_padding(ciphertext_modulus);
    let glwe_dimension = params.glwe_dimension;
    let polynomial_size = params.polynomial_size;
    let thread_count = params.thread_count;

    let mut rsc = TestResources::new();

    let f = |x| x;

    let delta = encoding_with_padding / msg_modulus;
    let mut msg = msg_modulus;

    let accumulator = generate_programmable_bootstrap_glwe_lut(
        polynomial_size,
        glwe_dimension.to_glwe_size(),
        msg_modulus.cast_into(),
        ciphertext_modulus,
        delta,
        f,
    );

    assert!(check_encrypted_content_respects_mod(
        &accumulator,
        ciphertext_modulus
    ));

    // Create the LweSecretKey
    let input_lwe_secret_key = allocate_and_generate_new_binary_lwe_secret_key(
        params.input_lwe_dimension,
        &mut rsc.secret_random_generator,
    );
    let output_glwe_secret_key = allocate_and_generate_new_binary_glwe_secret_key(
        params.glwe_dimension,
        params.polynomial_size,
        &mut rsc.secret_random_generator,
    );

    let mut bsk = LweMultiBitBootstrapKey::new(
        0u128,
        params.glwe_dimension.to_glwe_size(),
        params.polynomial_size,
        params.decomp_base_log,
        params.decomp_level_count,
        params.input_lwe_dimension,
        params.grouping_factor,
        params.ciphertext_modulus,
    );

    par_generate_lwe_multi_bit_bootstrap_key(
        &input_lwe_secret_key,
        &output_glwe_secret_key,
        &mut bsk,
        params.glwe_noise_distribution,
        &mut rsc.encryption_random_generator,
    );

    let mut fbsk = Fourier128LweMultiBitBootstrapKey::new(
        bsk.input_lwe_dimension(),
        bsk.glwe_size(),
        bsk.polynomial_size(),
        bsk.decomposition_base_log(),
        bsk.decomposition_level_count(),
        bsk.grouping_factor(),
    );

    par_convert_standard_lwe_multi_bit_bootstrap_key_to_fourier_128(&bsk, &mut fbsk);

    let output_lwe_secret_key = output_glwe_secret_key.into_lwe_secret_key();

    assert!(check_encrypted_content_respects_mod(
        &*bsk,
        ciphertext_modulus
    ));

    while msg != 0u128 {
        msg = msg.wrapping_sub(1);
        for _ in 0..NB_TESTS_LIGHT {
            let plaintext = Plaintext(msg * delta);

            let lwe_ciphertext_in = allocate_and_encrypt_new_lwe_ciphertext(
                &input_lwe_secret_key,
                plaintext,
                lwe_noise_distribution,
                ciphertext_modulus,
                &mut rsc.encryption_random_generator,
            );

            assert!(check_encrypted_content_respects_mod(
                &lwe_ciphertext_in,
                ciphertext_modulus
            ));

            let mut out_pbs_ct = LweCiphertext::new(
                0u128,
                output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                ciphertext_modulus,
            );

            multi_bit_programmable_bootstrap_f128_lwe_ciphertext(
                &lwe_ciphertext_in,
                &mut out_pbs_ct,
                &accumulator,
                &fbsk,
                thread_count,
                true,
            );

            assert!(check_encrypted_content_respects_mod(
                &out_pbs_ct,
                ciphertext_modulus
            ));

            let decrypted = decrypt_lwe_ciphertext(&output_lwe_secret_key, &out_pbs_ct);

            let decoded = round_decode(decrypted.0, delta) % msg_modulus;

            assert_eq!(decoded, f(msg));

            // Check determinism
            let mut out_pbs_ct_other = LweCiphertext::new(
                0u128,
                output_lwe_secret_key.lwe_dimension().to_lwe_size(),
                ciphertext_modulus,
            );

            multi_bit_programmable_bootstrap_f128_lwe_ciphertext(
                &lwe_ciphertext_in,
                &mut out_pbs_ct_other,
                &accumulator,
                &fbsk,
                thread_count,
                true,
            );

            assert_eq!(out_pbs_ct_other, out_pbs_ct);
        }

        // In coverage, we break after one while loop iteration, changing message values does not
        // yield higher coverage
        #[cfg(tarpaulin)]
        break;
    }
}

#[test]
pub fn test_lwe_encrypt_multi_bit_deterministic_pbs_f128_decrypt_factor_3_thread_12_native_mod() {
    lwe_encrypt_multi_bit_deterministic_pbs_f128_decrypt_custom_mod(MULTI_BIT_2_2_3_PARAMS_U128);
}
