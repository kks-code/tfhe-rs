use super::shortint::load_params;
use crate::{load_and_unversionize, TestedModule};
use std::path::Path;
#[cfg(feature = "zk-pok")]
use tfhe::integer::parameters::DynamicDistribution;
use tfhe::prelude::{CiphertextList, FheDecrypt, FheEncrypt, ParameterSetConformant};
#[cfg(feature = "zk-pok")]
use tfhe::shortint::parameters::{
    CompactCiphertextListExpansionKind, CompactPublicKeyEncryptionParameters,
};
#[cfg(feature = "zk-pok")]
use tfhe::shortint::prelude::LweDimension;
use tfhe::shortint::{
    AtomicPatternParameters, CarryModulus, CiphertextModulus, MessageModulus, PBSParameters,
};
#[cfg(feature = "zk-pok")]
use tfhe::zk::{CompactPkeCrs, CompactPkeCrsConformanceParams};
use tfhe::{
    set_server_key, ClientKey, CompactCiphertextList, CompressedCiphertextList,
    CompressedCompactPublicKey, CompressedFheBool, CompressedFheInt8, CompressedFheUint8,
    CompressedPublicKey, CompressedServerKey, CompressedSquashedNoiseCiphertextList, FheBool,
    FheInt8, FheUint8, SquashedNoiseFheBool, SquashedNoiseFheInt, SquashedNoiseFheUint,
};
#[cfg(feature = "zk-pok")]
use tfhe::{CompactPublicKey, ProvenCompactCiphertextList};
use tfhe_backward_compat_data::load::{
    load_versioned_auxiliary, DataFormat, TestFailure, TestResult, TestSuccess,
};
use tfhe_backward_compat_data::{
    DataKind, HlBoolCiphertextTest, HlCiphertextTest, HlClientKeyTest,
    HlCompressedSquashedNoiseCiphertextListTest, HlHeterogeneousCiphertextListTest,
    HlPublicKeyTest, HlServerKeyTest, HlSignedCiphertextTest, HlSquashedNoiseBoolCiphertextTest,
    HlSquashedNoiseSignedCiphertextTest, HlSquashedNoiseUnsignedCiphertextTest, TestMetadata,
    TestParameterSet, TestType, Testcase, ZkPkePublicParamsTest,
};
use tfhe_versionable::Unversionize;

fn load_hl_params(test_params: &TestParameterSet) -> AtomicPatternParameters {
    let pbs_params = load_params(test_params);

    PBSParameters::PBS(pbs_params).into()
}

/// Test HL ciphertext: loads the ciphertext and compare the decrypted value to the one in the
/// metadata.
pub fn test_hl_ciphertext(
    dir: &Path,
    test: &HlCiphertextTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let server_key = key.generate_server_key();
    set_server_key(server_key);

    let ct = if test.compressed {
        let compressed: CompressedFheUint8 = load_and_unversionize(dir, test, format)?;
        compressed.decompress()
    } else {
        load_and_unversionize(dir, test, format)?
    };

    let clear: u8 = ct.decrypt(&key);

    if clear != (test.clear_value as u8) {
        Err(test.failure(
            format!(
                "Invalid {} decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                format, clear, test.clear_value
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL signed ciphertext: loads the ciphertext and compare the decrypted value to the one in
/// the metadata.
pub fn test_hl_signed_ciphertext(
    dir: &Path,
    test: &HlSignedCiphertextTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let server_key = key.generate_server_key();
    set_server_key(server_key);

    let ct = if test.compressed {
        let compressed: CompressedFheInt8 = load_and_unversionize(dir, test, format)?;
        compressed.decompress()
    } else {
        load_and_unversionize(dir, test, format)?
    };

    let clear: i8 = ct.decrypt(&key);

    if clear != (test.clear_value as i8) {
        Err(test.failure(
            format!(
                "Invalid {} decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                format, clear, test.clear_value
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL bool ciphertext: loads the ciphertext and compare the decrypted value to the one in the
/// metadata.
pub fn test_hl_bool_ciphertext(
    dir: &Path,
    test: &HlBoolCiphertextTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let server_key = key.generate_server_key();
    set_server_key(server_key);

    let ct = if test.compressed {
        let compressed: CompressedFheBool = load_and_unversionize(dir, test, format)?;
        compressed.decompress()
    } else {
        load_and_unversionize(dir, test, format)?
    };

    let clear = ct.decrypt(&key);

    if clear != test.clear_value {
        Err(test.failure(
            format!(
                "Invalid {} decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                format, clear, test.clear_value
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test Zk Public params
pub fn test_zk_params(
    dir: &Path,
    test: &ZkPkePublicParamsTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    #[cfg(feature = "zk-pok")]
    {
        let loaded_crs: CompactPkeCrs = load_and_unversionize(dir, test, format)?;
        let modulus = (test.plaintext_modulus / 2).isqrt();
        let pke_params = CompactPublicKeyEncryptionParameters {
            encryption_lwe_dimension: LweDimension(test.lwe_dimension),
            encryption_noise_distribution: DynamicDistribution::new_t_uniform(
                test.noise_bound as u32,
            ),
            message_modulus: MessageModulus(modulus as u64),
            carry_modulus: CarryModulus(modulus as u64),
            ciphertext_modulus: CiphertextModulus::try_new(test.ciphertext_modulus).unwrap(),
            expansion_kind: CompactCiphertextListExpansionKind::RequiresCasting,
            zk_scheme: loaded_crs.scheme_version().into(),
        };
        let conformance_params =
            CompactPkeCrsConformanceParams::new(pke_params, loaded_crs.max_num_messages()).unwrap();

        loaded_crs.is_conformant(&conformance_params);
    }

    #[cfg(not(feature = "zk-pok"))]
    let _ = dir;

    Ok(test.success(format))
}

/// Test HL ciphertext list: loads the ciphertext list and compare the decrypted values to the ones
///  in the metadata.
pub fn test_hl_heterogeneous_ciphertext_list(
    dir: &Path,
    test: &HlHeterogeneousCiphertextListTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let server_key = key.generate_server_key();
    set_server_key(server_key);

    if test.compressed {
        let list: CompressedCiphertextList = load_and_unversionize(dir, test, format)?;
        test_hl_heterogeneous_ciphertext_list_elements(list, &key, test)
    } else if let Some(zk_info) = &test.proof_info {
        #[cfg(feature = "zk-pok")]
        {
            let crs_file = dir.join(&*zk_info.params_filename);
            let crs = CompactPkeCrs::unversionize(
                load_versioned_auxiliary(crs_file).map_err(|e| test.failure(e, format))?,
            )
            .map_err(|e| test.failure(e, format))?;

            let pubkey_file = dir.join(&*zk_info.public_key_filename);
            let pubkey = CompactPublicKey::unversionize(
                load_versioned_auxiliary(pubkey_file).map_err(|e| test.failure(e, format))?,
            )
            .map_err(|e| test.failure(e, format))?;

            let list: ProvenCompactCiphertextList = load_and_unversionize(dir, test, format)?;
            test_hl_heterogeneous_ciphertext_list_elements(
                list.verify_and_expand(&crs, &pubkey, zk_info.metadata.as_bytes())
                    .map_err(|msg| test.failure(msg, format))?,
                &key,
                test,
            )
        }
        #[cfg(not(feature = "zk-pok"))]
        {
            let _ = zk_info;
            Ok(())
        }
    } else {
        let list: CompactCiphertextList = load_and_unversionize(dir, test, format)?;
        test_hl_heterogeneous_ciphertext_list_elements(
            list.expand().map_err(|msg| test.failure(msg, format))?,
            &key,
            test,
        )
    }
    .map(|_| test.success(format))
    .map_err(|msg| test.failure(msg, format))
}

pub fn test_hl_heterogeneous_ciphertext_list_elements<CtList: CiphertextList>(
    list: CtList,
    key: &ClientKey,
    test: &HlHeterogeneousCiphertextListTest,
) -> Result<(), String> {
    for idx in 0..(list.len()) {
        match test.data_kinds[idx] {
            DataKind::Bool => {
                let ct: FheBool = list.get(idx).unwrap().unwrap();
                let clear = ct.decrypt(key);
                if clear != (test.clear_values[idx] != 0) {
                    return Err(format!(
                        "Invalid decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                        clear, test.clear_values[idx]
                    ));
                }
            }
            DataKind::Signed => {
                let ct: FheInt8 = list.get(idx).unwrap().unwrap();
                let clear: i8 = ct.decrypt(key);
                if clear != test.clear_values[idx] as i8 {
                    return Err(format!(
                        "Invalid decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                        clear,
                        (test.clear_values[idx] as i8)
                    ));
                }
            }
            DataKind::Unsigned => {
                let ct: FheUint8 = list.get(idx).unwrap().unwrap();
                let clear: u8 = ct.decrypt(key);
                if clear != test.clear_values[idx] as u8 {
                    return Err(format!(
                        "Invalid decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                        clear, test.clear_values[idx]
                    ));
                }
            }
        };
    }
    Ok(())
}

/// Test HL client key: loads the key and checks the parameters using the values stored in
/// the test metadata.
pub fn test_hl_clientkey(
    dir: &Path,
    test: &HlClientKeyTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let test_params = load_hl_params(&test.parameters);

    let key: ClientKey = load_and_unversionize(dir, test, format)?;
    let key_params = key.computation_parameters();

    if test_params != key_params {
        Err(test.failure(
            format!(
                "Invalid {format} parameters:\n Expected :\n{test_params:?}\nGot:\n{key_params:?}",
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL public key: encrypt a number with the pubkey and tries to decrypt it using
/// the associated client key.
pub fn test_hl_pubkey(
    dir: &Path,
    test: &HlPublicKeyTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let client_key_file = dir.join(&*test.client_key_filename);
    let client_key = ClientKey::unversionize(
        load_versioned_auxiliary(client_key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let value = 113u8;

    let encrypted = if test.compact {
        let public_key = if test.compressed {
            let compressed: CompressedCompactPublicKey = load_and_unversionize(dir, test, format)?;
            compressed.decompress()
        } else {
            load_and_unversionize(dir, test, format)?
        };
        let ct_list = CompactCiphertextList::builder(&public_key)
            .push(value)
            .build();
        ct_list.expand().unwrap().get(0).unwrap().unwrap()
    } else {
        let public_key = if test.compressed {
            let compressed: CompressedPublicKey = load_and_unversionize(dir, test, format)?;
            compressed.decompress()
        } else {
            load_and_unversionize(dir, test, format)?
        };
        FheUint8::encrypt(value, &public_key)
    };
    let decrypted: u8 = encrypted.decrypt(&client_key);

    if decrypted != value {
        Err(test.failure(
            format!(
                "Failed to decrypt value encrypted with public key, got {decrypted} expected {value}",
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL server key: encrypt two values with a client key, add them using the server key and
/// check that the decrypted sum is valid.
pub fn test_hl_serverkey(
    dir: &Path,
    test: &HlServerKeyTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let client_key_file = dir.join(&*test.client_key_filename);
    let client_key = ClientKey::unversionize(
        load_versioned_auxiliary(client_key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let v1 = 73u8;
    let ct1 = FheUint8::encrypt(v1, &client_key);
    let v2 = 102u8;
    let ct2 = FheUint8::encrypt(v2, &client_key);

    let key = if test.compressed {
        let compressed: CompressedServerKey = load_and_unversionize(dir, test, format)?;
        compressed.decompress()
    } else {
        load_and_unversionize(dir, test, format)?
    };
    set_server_key(key);

    let ct_sum = ct1 + ct2;
    let sum: u8 = ct_sum.decrypt(&client_key);

    if sum != v1 + v2 {
        Err(test.failure(
            format!(
                "Invalid result for addition using loaded server key, expected {} got {}",
                v1 + v2,
                sum,
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL ciphertext: loads the ciphertext and compare the decrypted value to the one in the
/// metadata.
pub fn test_hl_squashed_noise_unsigned_ciphertext(
    dir: &Path,
    test: &HlSquashedNoiseUnsignedCiphertextTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let ct: SquashedNoiseFheUint = load_and_unversionize(dir, test, format)?;

    let clear: u64 = ct.decrypt(&key);

    if clear != (test.clear_value) {
        Err(test.failure(
            format!(
                "Invalid {} decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                format, clear, test.clear_value
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL signed ciphertext: loads the ciphertext and compare the decrypted value to the one in
/// the metadata.
pub fn test_hl_squashed_noise_signed_ciphertext(
    dir: &Path,
    test: &HlSquashedNoiseSignedCiphertextTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let ct: SquashedNoiseFheInt = load_and_unversionize(dir, test, format)?;

    let clear: i64 = ct.decrypt(&key);

    if clear != (test.clear_value) {
        Err(test.failure(
            format!(
                "Invalid {} decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                format, clear, test.clear_value
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL bool ciphertext: loads the ciphertext and compare the decrypted value to the one in the
/// metadata.
pub fn test_hl_squashed_noise_bool_ciphertext(
    dir: &Path,
    test: &HlSquashedNoiseBoolCiphertextTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(e, format))?;

    let ct: SquashedNoiseFheBool = load_and_unversionize(dir, test, format)?;

    let clear: bool = ct.decrypt(&key);

    if clear != (test.clear_value) {
        Err(test.failure(
            format!(
                "Invalid {} decrypted cleartext:\n Expected :\n{:?}\nGot:\n{:?}",
                format, clear, test.clear_value
            ),
            format,
        ))
    } else {
        Ok(test.success(format))
    }
}

/// Test HL compressed squashed noise ciphertext list:
/// loads the ciphertext list and compare the decrypted value to the one in the
/// metadata.
pub fn test_hl_compressed_squashed_noise_ciphertext_list(
    dir: &Path,
    test: &HlCompressedSquashedNoiseCiphertextListTest,
    format: DataFormat,
) -> Result<TestSuccess, TestFailure> {
    let key_file = dir.join(&*test.key_filename);
    let key = ClientKey::unversionize(
        load_versioned_auxiliary(key_file).map_err(|e| test.failure(e, format))?,
    )
    .map_err(|e| test.failure(format!("Failed to load key file: {e}"), format))?;

    let list: CompressedSquashedNoiseCiphertextList = load_and_unversionize(dir, test, format)
        .map_err(|e| test.failure(format!("Failed to load list file: {e}"), format))?;

    if list.len() != test.clear_values.len() || list.len() != test.data_kinds.len() {
        return Err(test.failure(
            format!(
                "Invalid len for the compressed list, expected {} elements, got {}",
                test.clear_values.len(),
                list.len()
            ),
            format,
        ));
    }

    for i in 0..list.len() {
        let decrypted = match test.data_kinds[i] {
            DataKind::Unsigned => {
                let ct: SquashedNoiseFheUint = list.get(i).unwrap().unwrap();
                let clear: u64 = ct.decrypt(&key);
                clear
            }
            DataKind::Signed => {
                let ct: SquashedNoiseFheInt = list.get(i).unwrap().unwrap();
                let clear: i64 = ct.decrypt(&key);
                clear as u64
            }
            DataKind::Bool => {
                let ct: SquashedNoiseFheBool = list.get(i).unwrap().unwrap();
                let clear: bool = ct.decrypt(&key);
                clear as u64
            }
        };

        let expected = test.clear_values[i];
        if decrypted != expected {
            return Err(test.failure(
                format!(
                    "Invalid decryption at index {i}:\n Expected :{expected:?} Got: {decrypted:?}",
                ),
                format,
            ));
        }
    }

    Ok(test.success(format))
}

pub struct Hl;

impl TestedModule for Hl {
    const METADATA_FILE: &'static str = "high_level_api.ron";

    fn run_test<P: AsRef<Path>>(
        test_dir: P,
        testcase: &Testcase,
        format: DataFormat,
    ) -> TestResult {
        #[allow(unreachable_patterns)]
        match &testcase.metadata {
            TestMetadata::HlCiphertext(test) => {
                test_hl_ciphertext(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlSignedCiphertext(test) => {
                test_hl_signed_ciphertext(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlBoolCiphertext(test) => {
                test_hl_bool_ciphertext(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlHeterogeneousCiphertextList(test) => {
                test_hl_heterogeneous_ciphertext_list(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlClientKey(test) => {
                test_hl_clientkey(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlPublicKey(test) => {
                test_hl_pubkey(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlServerKey(test) => {
                test_hl_serverkey(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::ZkPkePublicParams(test) => {
                test_zk_params(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlSquashedNoiseUnsignedCiphertext(test) => {
                test_hl_squashed_noise_unsigned_ciphertext(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlSquashedNoiseSignedCiphertext(test) => {
                test_hl_squashed_noise_signed_ciphertext(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlSquashedNoiseBoolCiphertext(test) => {
                test_hl_squashed_noise_bool_ciphertext(test_dir.as_ref(), test, format).into()
            }
            TestMetadata::HlCompressedSquashedNoiseCiphertextList(test) => {
                test_hl_compressed_squashed_noise_ciphertext_list(test_dir.as_ref(), test, format)
                    .into()
            }
            _ => {
                println!("WARNING: missing test: {:?}", testcase.metadata);
                TestResult::Skipped(testcase.skip())
            }
        }
    }
}
