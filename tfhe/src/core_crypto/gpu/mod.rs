pub mod algorithms;
pub mod entities;
pub mod slice;
pub mod vec;

use crate::core_crypto::gpu::lwe_bootstrap_key::{
    prepare_cuda_ms_noise_reduction_key_ffi, CudaModulusSwitchNoiseReductionKey,
};
use crate::core_crypto::gpu::vec::{CudaVec, GpuIndex};
use crate::core_crypto::prelude::{
    CiphertextModulus, DecompositionBaseLog, DecompositionLevelCount, GlweCiphertextCount,
    GlweDimension, LweBskGroupingFactor, LweCiphertextCount, LweDimension, PolynomialSize,
    UnsignedInteger,
};
pub use algorithms::*;
pub use entities::*;
use std::any::{Any, TypeId};
use std::ffi::c_void;
use tfhe_cuda_backend::bindings::*;
use tfhe_cuda_backend::cuda_bind::*;

pub struct CudaStreams {
    pub ptr: Vec<*mut c_void>,
    pub gpu_indexes: Vec<GpuIndex>,
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for CudaStreams {}
unsafe impl Sync for CudaStreams {}

impl CudaStreams {
    /// Create a new `CudaStreams` structure with as many GPUs as there are on the machine
    pub fn new_multi_gpu() -> Self {
        let gpu_count = setup_multi_gpu(GpuIndex::new(0));
        let mut gpu_indexes = Vec::with_capacity(gpu_count as usize);
        let mut ptr_array = Vec::with_capacity(gpu_count as usize);

        for i in 0..gpu_count {
            ptr_array.push(unsafe { cuda_create_stream(i) });
            gpu_indexes.push(GpuIndex::new(i));
        }
        Self {
            ptr: ptr_array,
            gpu_indexes,
        }
    }
    /// Create a new `CudaStreams` structure with the GPUs with id provided in a list
    pub fn new_multi_gpu_with_indexes(indexes: &[GpuIndex]) -> Self {
        let _gpu_count = setup_multi_gpu(indexes[0]);

        let mut gpu_indexes = Vec::with_capacity(indexes.len());
        let mut ptr_array = Vec::with_capacity(indexes.len());

        for &i in indexes {
            ptr_array.push(unsafe { cuda_create_stream(i.get()) });
            gpu_indexes.push(i);
        }
        Self {
            ptr: ptr_array,
            gpu_indexes,
        }
    }
    /// Create a new `CudaStreams` structure with one GPU, whose index corresponds to the one given
    /// as input
    pub fn new_single_gpu(gpu_index: GpuIndex) -> Self {
        Self {
            ptr: vec![unsafe { cuda_create_stream(gpu_index.get()) }],
            gpu_indexes: vec![gpu_index],
        }
    }
    /// Synchronize all cuda streams in the `CudaStreams` structure
    pub fn synchronize(&self) {
        for i in 0..self.len() {
            unsafe {
                cuda_synchronize_stream(self.ptr[i], self.gpu_indexes[i].get());
            }
        }
    }
    /// Synchronize one cuda streams in the `CudaStreams` structure
    pub fn synchronize_one(&self, gpu_index: u32) {
        unsafe {
            cuda_synchronize_stream(
                self.ptr[gpu_index as usize],
                self.gpu_indexes[gpu_index as usize].get(),
            );
        }
    }
    /// Return the number of GPU indexes, which is the same as the number of Cuda streams
    pub fn len(&self) -> usize {
        self.gpu_indexes.len()
    }
    /// Returns `true` if the CudaVec contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn gpu_indexes(&self) -> &[GpuIndex] {
        &self.gpu_indexes
    }

    /// Returns a pointer the array of GpuIndex as u32
    pub(crate) fn gpu_indexes_ptr(&self) -> *const u32 {
        // The cast here is safe as GpuIndex is repr(transparent)
        self.gpu_indexes.as_ptr().cast()
    }
}

impl Clone for CudaStreams {
    fn clone(&self) -> Self {
        // The `new_multi_gpu_with_indexes()` function is used here to adapt to any specific type of
        // streams being cloned (single, multi, or custom)
        Self::new_multi_gpu_with_indexes(self.gpu_indexes.as_slice())
    }
}

impl Drop for CudaStreams {
    fn drop(&mut self) {
        for (i, &s) in self.ptr.iter().enumerate() {
            unsafe {
                cuda_destroy_stream(s, self.gpu_indexes[i].get());
            }
        }
    }
}

/// Programmable bootstrap on a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn programmable_bootstrap_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_out_indexes: &CudaVec<T>,
    test_vector: &CudaVec<T>,
    test_vector_indexes: &CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    lwe_in_indexes: &CudaVec<T>,
    bootstrapping_key: &CudaVec<f64>,
    lwe_dimension: LweDimension,
    glwe_dimension: GlweDimension,
    polynomial_size: PolynomialSize,
    base_log: DecompositionBaseLog,
    level: DecompositionLevelCount,
    num_samples: u32,
    noise_reduction_key: Option<&CudaModulusSwitchNoiseReductionKey>,
    ct_modulus: f64,
) {
    let num_many_lut = 1u32;
    let lut_stride = 0u32;
    let mut pbs_buffer: *mut i8 = std::ptr::null_mut();

    let ms_noise_reduction_key_ffi =
        prepare_cuda_ms_noise_reduction_key_ffi(noise_reduction_key, ct_modulus);
    let ms_noise_reduction_ptr = noise_reduction_key.map_or(std::ptr::null_mut(), |ms_key| {
        ms_key.modulus_switch_zeros.ptr[0]
    });

    let allocate_ms_noise_array = noise_reduction_key.is_some();
    scratch_cuda_programmable_bootstrap_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(pbs_buffer),
        lwe_dimension.0 as u32,
        glwe_dimension.0 as u32,
        polynomial_size.0 as u32,
        level.0 as u32,
        num_samples,
        true,
        allocate_ms_noise_array,
    );

    cuda_programmable_bootstrap_lwe_ciphertext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_out_indexes.as_c_ptr(0),
        test_vector.as_c_ptr(0),
        test_vector_indexes.as_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        lwe_in_indexes.as_c_ptr(0),
        bootstrapping_key.as_c_ptr(0),
        &raw const ms_noise_reduction_key_ffi,
        ms_noise_reduction_ptr,
        pbs_buffer,
        lwe_dimension.0 as u32,
        glwe_dimension.0 as u32,
        polynomial_size.0 as u32,
        base_log.0 as u32,
        level.0 as u32,
        num_samples,
        num_many_lut,
        lut_stride,
    );

    cleanup_cuda_programmable_bootstrap(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(pbs_buffer),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn get_programmable_bootstrap_size_on_gpu(
    streams: &CudaStreams,
    lwe_dimension: LweDimension,
    glwe_dimension: GlweDimension,
    polynomial_size: PolynomialSize,
    level: DecompositionLevelCount,
    num_samples: u32,
    noise_reduction_key: Option<&CudaModulusSwitchNoiseReductionKey>,
) -> u64 {
    let mut pbs_buffer: *mut i8 = std::ptr::null_mut();
    let allocate_ms_noise_array = noise_reduction_key.is_some();
    let size_tracker = unsafe {
        scratch_cuda_programmable_bootstrap_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
            lwe_dimension.0 as u32,
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
            level.0 as u32,
            num_samples,
            false,
            allocate_ms_noise_array,
        )
    };

    unsafe {
        cleanup_cuda_programmable_bootstrap(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
        );
    }
    size_tracker
}

/// Programmable bootstrap on a vector of 128 bit LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn programmable_bootstrap_128_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    test_vector: &CudaVec<T>,
    lwe_array_in: &CudaVec<u64>,
    bootstrapping_key: &CudaVec<f64>,
    lwe_dimension: LweDimension,
    glwe_dimension: GlweDimension,
    polynomial_size: PolynomialSize,
    base_log: DecompositionBaseLog,
    level: DecompositionLevelCount,
    num_samples: u32,
    noise_reduction_key: Option<&CudaModulusSwitchNoiseReductionKey>,
    ct_modulus: f64,
) {
    let mut pbs_buffer: *mut i8 = std::ptr::null_mut();
    let ms_noise_reduction_key_ffi =
        prepare_cuda_ms_noise_reduction_key_ffi(noise_reduction_key, ct_modulus);
    let ms_noise_reduction_ptr = noise_reduction_key.map_or(std::ptr::null_mut(), |ms_key| {
        ms_key.modulus_switch_zeros.ptr[0]
    });
    let allocate_ms_noise_array = noise_reduction_key.is_some();
    scratch_cuda_programmable_bootstrap_128(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(pbs_buffer),
        lwe_dimension.0 as u32,
        glwe_dimension.0 as u32,
        polynomial_size.0 as u32,
        level.0 as u32,
        num_samples,
        true,
        allocate_ms_noise_array,
    );

    cuda_programmable_bootstrap_lwe_ciphertext_vector_128(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        test_vector.as_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        bootstrapping_key.as_c_ptr(0),
        &raw const ms_noise_reduction_key_ffi,
        ms_noise_reduction_ptr,
        pbs_buffer,
        lwe_dimension.0 as u32,
        glwe_dimension.0 as u32,
        polynomial_size.0 as u32,
        base_log.0 as u32,
        level.0 as u32,
        num_samples,
    );

    cleanup_cuda_programmable_bootstrap_128(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(pbs_buffer),
    );
}

/// Programmable multi-bit bootstrap on a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn programmable_bootstrap_multi_bit_async<
    T: UnsignedInteger,
    B: Any + UnsignedInteger,
>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<B>,
    output_indexes: &CudaVec<T>,
    test_vector: &CudaVec<B>,
    test_vector_indexes: &CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    input_indexes: &CudaVec<T>,
    bootstrapping_key: &CudaVec<B>,
    lwe_dimension: LweDimension,
    glwe_dimension: GlweDimension,
    polynomial_size: PolynomialSize,
    base_log: DecompositionBaseLog,
    level: DecompositionLevelCount,
    grouping_factor: LweBskGroupingFactor,
    num_samples: u32,
) {
    let num_many_lut = 1u32;
    let lut_stride = 0u32;
    let mut pbs_buffer: *mut i8 = std::ptr::null_mut();
    if TypeId::of::<B>() == TypeId::of::<u128>() {
        scratch_cuda_multi_bit_programmable_bootstrap_128_vector_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
            level.0 as u32,
            num_samples,
            true,
        );
        cuda_multi_bit_programmable_bootstrap_lwe_ciphertext_vector_128(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            lwe_array_out.as_mut_c_ptr(0),
            output_indexes.as_c_ptr(0),
            test_vector.as_c_ptr(0),
            test_vector_indexes.as_c_ptr(0),
            lwe_array_in.as_c_ptr(0),
            input_indexes.as_c_ptr(0),
            bootstrapping_key.as_c_ptr(0),
            pbs_buffer,
            lwe_dimension.0 as u32,
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
            grouping_factor.0 as u32,
            base_log.0 as u32,
            level.0 as u32,
            num_samples,
            num_many_lut,
            lut_stride,
        );
        cleanup_cuda_multi_bit_programmable_bootstrap_128(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
        );
    } else if TypeId::of::<B>() == TypeId::of::<u64>() {
        scratch_cuda_multi_bit_programmable_bootstrap_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
            level.0 as u32,
            num_samples,
            true,
        );
        cuda_multi_bit_programmable_bootstrap_lwe_ciphertext_vector_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            lwe_array_out.as_mut_c_ptr(0),
            output_indexes.as_c_ptr(0),
            test_vector.as_c_ptr(0),
            test_vector_indexes.as_c_ptr(0),
            lwe_array_in.as_c_ptr(0),
            input_indexes.as_c_ptr(0),
            bootstrapping_key.as_c_ptr(0),
            pbs_buffer,
            lwe_dimension.0 as u32,
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
            grouping_factor.0 as u32,
            base_log.0 as u32,
            level.0 as u32,
            num_samples,
            num_many_lut,
            lut_stride,
        );
        cleanup_cuda_multi_bit_programmable_bootstrap(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
        );
    } else {
        panic!("Unsupported torus size")
    }
}

#[allow(clippy::too_many_arguments)]
pub fn get_programmable_bootstrap_multi_bit_size_on_gpu(
    streams: &CudaStreams,
    glwe_dimension: GlweDimension,
    polynomial_size: PolynomialSize,
    level: DecompositionLevelCount,
    num_samples: u32,
) -> u64 {
    let mut pbs_buffer: *mut i8 = std::ptr::null_mut();
    let size_tracker = unsafe {
        scratch_cuda_multi_bit_programmable_bootstrap_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
            level.0 as u32,
            num_samples,
            false,
        )
    };
    unsafe {
        cleanup_cuda_multi_bit_programmable_bootstrap(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(pbs_buffer),
        );
    }
    size_tracker
}

/// Keyswitch on a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn keyswitch_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_out_indexes: &CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    lwe_in_indexes: &CudaVec<T>,
    input_lwe_dimension: LweDimension,
    output_lwe_dimension: LweDimension,
    keyswitch_key: &CudaVec<T>,
    base_log: DecompositionBaseLog,
    l_gadget: DecompositionLevelCount,
    num_samples: u32,
) {
    cuda_keyswitch_lwe_ciphertext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_out_indexes.as_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        lwe_in_indexes.as_c_ptr(0),
        keyswitch_key.as_c_ptr(0),
        input_lwe_dimension.0 as u32,
        output_lwe_dimension.0 as u32,
        base_log.0 as u32,
        l_gadget.0 as u32,
        num_samples,
    );
}

/// Convert keyswitch key
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn convert_lwe_keyswitch_key_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    dest: &mut CudaVec<T>,
    src: &[T],
) {
    dest.copy_from_cpu_multi_gpu_async(src, streams);
}

/// Applies packing keyswitch on a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn packing_keyswitch_list_64_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    glwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    input_lwe_dimension: LweDimension,
    output_glwe_dimension: GlweDimension,
    output_polynomial_size: PolynomialSize,
    packing_keyswitch_key: &CudaVec<T>,
    base_log: DecompositionBaseLog,
    l_gadget: DecompositionLevelCount,
    num_lwes: LweCiphertextCount,
) {
    let mut fp_ks_buffer: *mut i8 = std::ptr::null_mut();
    scratch_packing_keyswitch_lwe_list_to_glwe_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(fp_ks_buffer),
        input_lwe_dimension.0 as u32,
        output_glwe_dimension.0 as u32,
        output_polynomial_size.0 as u32,
        num_lwes.0 as u32,
        true,
    );
    cuda_packing_keyswitch_lwe_list_to_glwe_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        glwe_array_out.as_mut_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        packing_keyswitch_key.as_c_ptr(0),
        fp_ks_buffer,
        input_lwe_dimension.0 as u32,
        output_glwe_dimension.0 as u32,
        output_polynomial_size.0 as u32,
        base_log.0 as u32,
        l_gadget.0 as u32,
        num_lwes.0 as u32,
    );
    cleanup_packing_keyswitch_lwe_list_to_glwe(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(fp_ks_buffer),
        true,
    );
}

/// Applies packing keyswitch on a vector of 128-bit LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn packing_keyswitch_list_128_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    glwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    input_lwe_dimension: LweDimension,
    output_glwe_dimension: GlweDimension,
    output_polynomial_size: PolynomialSize,
    packing_keyswitch_key: &CudaVec<T>,
    base_log: DecompositionBaseLog,
    l_gadget: DecompositionLevelCount,
    num_lwes: LweCiphertextCount,
) {
    let mut fp_ks_buffer: *mut i8 = std::ptr::null_mut();
    scratch_packing_keyswitch_lwe_list_to_glwe_128(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(fp_ks_buffer),
        input_lwe_dimension.0 as u32,
        output_glwe_dimension.0 as u32,
        output_polynomial_size.0 as u32,
        num_lwes.0 as u32,
        true,
    );
    cuda_packing_keyswitch_lwe_list_to_glwe_128(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        glwe_array_out.as_mut_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        packing_keyswitch_key.as_c_ptr(0),
        fp_ks_buffer,
        input_lwe_dimension.0 as u32,
        output_glwe_dimension.0 as u32,
        output_polynomial_size.0 as u32,
        base_log.0 as u32,
        l_gadget.0 as u32,
        num_lwes.0 as u32,
    );
    cleanup_packing_keyswitch_lwe_list_to_glwe(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        std::ptr::addr_of_mut!(fp_ks_buffer),
        true,
    );
}

/// Convert programmable bootstrap key
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn convert_lwe_programmable_bootstrap_key_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    dest: &mut CudaVec<f64>,
    src: &[T],
    input_lwe_dim: LweDimension,
    glwe_dim: GlweDimension,
    l_gadget: DecompositionLevelCount,
    polynomial_size: PolynomialSize,
) {
    for (i, &stream_ptr) in streams.ptr.iter().enumerate() {
        if size_of::<T>() == 16 {
            cuda_convert_lwe_programmable_bootstrap_key_128(
                stream_ptr,
                streams.gpu_indexes[i].get(),
                dest.as_mut_c_ptr(i as u32),
                src.as_ptr().cast(),
                input_lwe_dim.0 as u32,
                glwe_dim.0 as u32,
                l_gadget.0 as u32,
                polynomial_size.0 as u32,
            );
        } else if size_of::<T>() == 8 {
            cuda_convert_lwe_programmable_bootstrap_key_64(
                stream_ptr,
                streams.gpu_indexes[i].get(),
                dest.as_mut_c_ptr(i as u32),
                src.as_ptr().cast(),
                input_lwe_dim.0 as u32,
                glwe_dim.0 as u32,
                l_gadget.0 as u32,
                polynomial_size.0 as u32,
            );
        } else {
            panic!("Unsupported torus size for bsk conversion")
        }
    }
}

/// Convert multi-bit programmable bootstrap key
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn convert_lwe_multi_bit_programmable_bootstrap_key_async<T: Any + UnsignedInteger>(
    streams: &CudaStreams,
    dest: &mut CudaVec<T>,
    src: &[T],
    input_lwe_dim: LweDimension,
    glwe_dim: GlweDimension,
    l_gadget: DecompositionLevelCount,
    polynomial_size: PolynomialSize,
    grouping_factor: LweBskGroupingFactor,
) {
    let size = std::mem::size_of_val(src);
    for (i, &stream_ptr) in streams.ptr.iter().enumerate() {
        assert_eq!(dest.len() * std::mem::size_of::<T>(), size);

        if TypeId::of::<T>() == TypeId::of::<u128>() {
            cuda_convert_lwe_multi_bit_programmable_bootstrap_key_128(
                stream_ptr,
                streams.gpu_indexes[i].get(),
                dest.as_mut_c_ptr(i as u32),
                src.as_ptr().cast(),
                input_lwe_dim.0 as u32,
                glwe_dim.0 as u32,
                l_gadget.0 as u32,
                polynomial_size.0 as u32,
                grouping_factor.0 as u32,
            );
        } else if TypeId::of::<T>() == TypeId::of::<u64>() {
            cuda_convert_lwe_multi_bit_programmable_bootstrap_key_64(
                stream_ptr,
                streams.gpu_indexes[i].get(),
                dest.as_mut_c_ptr(i as u32),
                src.as_ptr().cast(),
                input_lwe_dim.0 as u32,
                glwe_dim.0 as u32,
                l_gadget.0 as u32,
                polynomial_size.0 as u32,
                grouping_factor.0 as u32,
            );
        } else {
            panic!("Unsupported torus size for bsk conversion")
        }
    }
}

/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn extract_lwe_samples_from_glwe_ciphertext_list_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    glwe_array_in: &CudaVec<T>,
    nth_array: &CudaVec<u32>,
    num_nths: u32,
    lwe_per_glwe: u32,
    glwe_dimension: GlweDimension,
    polynomial_size: PolynomialSize,
) {
    if size_of::<T>() == 16 {
        cuda_glwe_sample_extract_128(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            lwe_array_out.as_mut_c_ptr(0),
            glwe_array_in.as_c_ptr(0),
            nth_array.as_c_ptr(0).cast::<u32>(),
            num_nths,
            lwe_per_glwe,
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
        );
    } else if size_of::<T>() == 8 {
        cuda_glwe_sample_extract_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            lwe_array_out.as_mut_c_ptr(0),
            glwe_array_in.as_c_ptr(0),
            nth_array.as_c_ptr(0).cast::<u32>(),
            num_nths,
            lwe_per_glwe,
            glwe_dimension.0 as u32,
            polynomial_size.0 as u32,
        );
    } else {
        panic!("Unsupported torus size for glwe sample extraction")
    }
}

/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn cuda_modulus_switch_ciphertext_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    log_modulus: u32,
) {
    cuda_modulus_switch_inplace_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_out.len() as u32,
        log_modulus,
    );
}

/// Addition of a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn add_lwe_ciphertext_vector_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_array_in_1: &CudaVec<T>,
    lwe_array_in_2: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    let mut output_degrees_vec: Vec<u64> = vec![0; num_samples as usize];
    let mut output_noise_levels_vec: Vec<u64> = vec![0; num_samples as usize];
    let mut input_1_degrees_vec = output_degrees_vec.clone();
    let mut input_1_noise_levels_vec = output_noise_levels_vec.clone();
    let mut input_2_degrees_vec = output_degrees_vec.clone();
    let mut input_2_noise_levels_vec = output_noise_levels_vec.clone();
    let mut lwe_array_out_data = CudaRadixCiphertextFFI {
        ptr: lwe_array_out.as_mut_c_ptr(0),
        degrees: output_degrees_vec.as_mut_ptr(),
        noise_levels: output_noise_levels_vec.as_mut_ptr(),
        num_radix_blocks: num_samples,
        max_num_radix_blocks: num_samples,
        lwe_dimension: lwe_dimension.0 as u32,
    };
    let lwe_array_in_1_data = CudaRadixCiphertextFFI {
        ptr: lwe_array_in_1.get_mut_c_ptr(0),
        degrees: input_1_degrees_vec.as_mut_ptr(),
        noise_levels: input_1_noise_levels_vec.as_mut_ptr(),
        num_radix_blocks: num_samples,
        max_num_radix_blocks: num_samples,
        lwe_dimension: lwe_dimension.0 as u32,
    };
    let lwe_array_in_2_data = CudaRadixCiphertextFFI {
        ptr: lwe_array_in_2.get_mut_c_ptr(0),
        degrees: input_2_degrees_vec.as_mut_ptr(),
        noise_levels: input_2_noise_levels_vec.as_mut_ptr(),
        num_radix_blocks: num_samples,
        max_num_radix_blocks: num_samples,
        lwe_dimension: lwe_dimension.0 as u32,
    };
    cuda_add_lwe_ciphertext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        &raw mut lwe_array_out_data,
        &raw const lwe_array_in_1_data,
        &raw const lwe_array_in_2_data,
    );
}

/// Assigned addition of a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn add_lwe_ciphertext_vector_assign_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    let mut output_degrees_vec: Vec<u64> = vec![0; num_samples as usize];
    let mut output_noise_levels_vec: Vec<u64> = vec![0; num_samples as usize];
    let mut input_degrees_vec = output_degrees_vec.clone();
    let mut input_noise_levels_vec = output_noise_levels_vec.clone();
    let mut lwe_array_out_data = CudaRadixCiphertextFFI {
        ptr: lwe_array_out.as_mut_c_ptr(0),
        degrees: output_degrees_vec.as_mut_ptr(),
        noise_levels: output_noise_levels_vec.as_mut_ptr(),
        num_radix_blocks: num_samples,
        max_num_radix_blocks: num_samples,
        lwe_dimension: lwe_dimension.0 as u32,
    };
    let lwe_array_in_data = CudaRadixCiphertextFFI {
        ptr: lwe_array_in.get_mut_c_ptr(0),
        degrees: input_degrees_vec.as_mut_ptr(),
        noise_levels: input_noise_levels_vec.as_mut_ptr(),
        num_radix_blocks: num_samples,
        max_num_radix_blocks: num_samples,
        lwe_dimension: lwe_dimension.0 as u32,
    };
    cuda_add_lwe_ciphertext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        &raw mut lwe_array_out_data,
        &raw const lwe_array_out_data,
        &raw const lwe_array_in_data,
    );
}

/// Addition of a vector of LWE ciphertexts with a vector of plaintexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn add_lwe_ciphertext_vector_plaintext_vector_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    plaintext_in: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_add_lwe_ciphertext_vector_plaintext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        plaintext_in.as_c_ptr(0),
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// Addition of a vector of LWE ciphertexts with a plaintext scalar
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn add_lwe_ciphertext_vector_plaintext_scalar_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    plaintext_in: u64,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_add_lwe_ciphertext_vector_plaintext_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        plaintext_in,
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// Assigned addition of a vector of LWE ciphertexts with a vector of plaintexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn add_lwe_ciphertext_vector_plaintext_vector_assign_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    plaintext_in: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_add_lwe_ciphertext_vector_plaintext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_out.as_c_ptr(0),
        plaintext_in.as_c_ptr(0),
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// Negation of a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn negate_lwe_ciphertext_vector_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_negate_lwe_ciphertext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// Assigned negation of a vector of LWE ciphertexts
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn negate_lwe_ciphertext_vector_assign_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_negate_lwe_ciphertext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_out.as_c_ptr(0),
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// Multiplication of a vector of LWEs with a vector of cleartexts (assigned)
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn mult_lwe_ciphertext_vector_cleartext_vector_assign_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array: &mut CudaVec<T>,
    cleartext_array_in: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_mult_lwe_ciphertext_vector_cleartext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array.as_mut_c_ptr(0),
        lwe_array.as_c_ptr(0),
        cleartext_array_in.as_c_ptr(0),
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// Multiplication of a vector of LWEs with a vector of cleartexts.
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
pub unsafe fn mult_lwe_ciphertext_vector_cleartext_vector<T: UnsignedInteger>(
    streams: &CudaStreams,
    lwe_array_out: &mut CudaVec<T>,
    lwe_array_in: &CudaVec<T>,
    cleartext_array_in: &CudaVec<T>,
    lwe_dimension: LweDimension,
    num_samples: u32,
) {
    cuda_mult_lwe_ciphertext_vector_cleartext_vector_64(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        lwe_array_out.as_mut_c_ptr(0),
        lwe_array_in.as_c_ptr(0),
        cleartext_array_in.as_c_ptr(0),
        lwe_dimension.0 as u32,
        num_samples,
    );
}

/// forward fourier transform for complex f128 as integer
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn fourier_transform_forward_as_integer_f128_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    re0: &mut [f64],
    re1: &mut [f64],
    im0: &mut [f64],
    im1: &mut [f64],
    standard: &[T],
    fft_size: u32,
    number_of_samples: u32,
) {
    cuda_fourier_transform_forward_as_integer_f128_async(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        re0.as_mut_ptr().cast::<c_void>(),
        re1.as_mut_ptr().cast::<c_void>(),
        im0.as_mut_ptr().cast::<c_void>(),
        im1.as_mut_ptr().cast::<c_void>(),
        standard.as_ptr().cast::<c_void>(),
        fft_size,
        number_of_samples,
    );
}

/// forward fourier transform for complex f128 as torus
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn fourier_transform_forward_as_torus_f128_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    re0: &mut [f64],
    re1: &mut [f64],
    im0: &mut [f64],
    im1: &mut [f64],
    standard: &[T],
    fft_size: u32,
    number_of_samples: u32,
) {
    cuda_fourier_transform_forward_as_torus_f128_async(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        re0.as_mut_ptr().cast::<c_void>(),
        re1.as_mut_ptr().cast::<c_void>(),
        im0.as_mut_ptr().cast::<c_void>(),
        im1.as_mut_ptr().cast::<c_void>(),
        standard.as_ptr().cast::<c_void>(),
        fft_size,
        number_of_samples,
    );
}

/// backward fourier transform for complex f128 as torus
///
/// # Safety
///
/// [CudaStreams::synchronize] __must__ be called as soon as synchronization is
/// required
#[allow(clippy::too_many_arguments)]
pub unsafe fn fourier_transform_backward_as_torus_f128_async<T: UnsignedInteger>(
    streams: &CudaStreams,
    standard: &mut [T],
    re0: &[f64],
    re1: &[f64],
    im0: &[f64],
    im1: &[f64],
    fft_size: u32,
    number_of_samples: u32,
) {
    cuda_fourier_transform_backward_as_torus_f128_async(
        streams.ptr[0],
        streams.gpu_indexes[0].get(),
        standard.as_mut_ptr().cast::<c_void>(),
        re0.as_ptr().cast::<c_void>(),
        re1.as_ptr().cast::<c_void>(),
        im0.as_ptr().cast::<c_void>(),
        im1.as_ptr().cast::<c_void>(),
        fft_size,
        number_of_samples,
    );
}

#[derive(Clone, Debug)]
pub struct CudaLweList<T: UnsignedInteger> {
    // Pointer to GPU data
    pub d_vec: CudaVec<T>,
    // Number of ciphertexts in the array
    pub lwe_ciphertext_count: LweCiphertextCount,
    // Lwe dimension
    pub lwe_dimension: LweDimension,
    // Ciphertext Modulus
    pub ciphertext_modulus: CiphertextModulus<T>,
}

impl<T: UnsignedInteger> CudaLweList<T> {
    pub fn duplicate(&self, streams: &CudaStreams) -> Self {
        Self {
            d_vec: self.d_vec.duplicate(streams),
            lwe_ciphertext_count: self.lwe_ciphertext_count,
            lwe_dimension: self.lwe_dimension,
            ciphertext_modulus: self.ciphertext_modulus,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CudaGlweList<T: UnsignedInteger> {
    // Pointer to GPU data
    pub d_vec: CudaVec<T>,
    // Number of ciphertexts in the array
    pub glwe_ciphertext_count: GlweCiphertextCount,
    // Glwe dimension
    pub glwe_dimension: GlweDimension,
    // Polynomial size
    pub polynomial_size: PolynomialSize,
    // Ciphertext Modulus
    pub ciphertext_modulus: CiphertextModulus<T>,
}

impl<T: UnsignedInteger> CudaGlweList<T> {
    pub fn duplicate(&self, streams: &CudaStreams) -> Self {
        Self {
            d_vec: self.d_vec.duplicate(streams),
            glwe_ciphertext_count: self.glwe_ciphertext_count,
            glwe_dimension: self.glwe_dimension,
            polynomial_size: self.polynomial_size,
            ciphertext_modulus: self.ciphertext_modulus,
        }
    }
}
/// Get the number of GPUs on the machine
pub fn get_number_of_gpus() -> u32 {
    unsafe { cuda_get_number_of_gpus() as u32 }
}

/// Setup multi-GPU and return the number of GPUs used
pub fn setup_multi_gpu(device_0_id: GpuIndex) -> u32 {
    unsafe { cuda_setup_multi_gpu(device_0_id.get()) as u32 }
}

/// Synchronize device
pub fn synchronize_device(gpu_index: u32) {
    unsafe { cuda_synchronize_device(gpu_index) }
}

/// Synchronize all devices
pub fn synchronize_devices(gpu_count: u32) {
    for i in 0..gpu_count {
        unsafe { cuda_synchronize_device(i) }
    }
}

/// Check there is enough memory on the device to allocate the necessary data
pub fn check_valid_cuda_malloc(size: u64, gpu_index: GpuIndex) -> bool {
    unsafe { cuda_check_valid_malloc(size, gpu_index.get()) }
}

/// Check if a memory allocation fits in GPU memory. If it doesn't fit, panic with
/// a helpful message.
pub fn check_valid_cuda_malloc_assert_oom(size: u64, gpu_index: GpuIndex) {
    if !check_valid_cuda_malloc(size, gpu_index) {
        let total_memory;
        unsafe {
            total_memory = cuda_device_total_memory(gpu_index.get());
        }
        panic!(
            "Not enough memory on GPU {}. Allocating {} bytes exceeds total memory: {} bytes",
            gpu_index.get(),
            size,
            total_memory
        );
    }
}

// Determine if a cuda device is available, at runtime
pub fn is_cuda_available() -> bool {
    let result = unsafe { cuda_is_available() };
    result == 1u32
}

pub fn get_packing_keyswitch_list_64_size_on_gpu(
    streams: &CudaStreams,
    input_lwe_dimension: LweDimension,
    output_glwe_dimension: GlweDimension,
    output_polynomial_size: PolynomialSize,
    num_lwes: LweCiphertextCount,
) -> u64 {
    let mut fp_ks_buffer: *mut i8 = std::ptr::null_mut();
    let size_tracker = unsafe {
        scratch_packing_keyswitch_lwe_list_to_glwe_64(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(fp_ks_buffer),
            input_lwe_dimension.0 as u32,
            output_glwe_dimension.0 as u32,
            output_polynomial_size.0 as u32,
            num_lwes.0 as u32,
            false,
        )
    };
    unsafe {
        cleanup_packing_keyswitch_lwe_list_to_glwe(
            streams.ptr[0],
            streams.gpu_indexes[0].get(),
            std::ptr::addr_of_mut!(fp_ks_buffer),
            false,
        );
    }
    size_tracker
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_gpu_info() {
        println!("Number of GPUs: {}", get_number_of_gpus());
    }
    #[test]
    fn allocate_and_copy() {
        let vec = vec![1_u64, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let stream = CudaStreams::new_single_gpu(GpuIndex::new(0));
        unsafe {
            let mut d_vec: CudaVec<u64> = CudaVec::<u64>::new_async(vec.len(), &stream, 0);
            d_vec.copy_from_cpu_async(&vec, &stream, 0);
            let mut empty = vec![0_u64; vec.len()];
            d_vec.copy_to_cpu_async(&mut empty, &stream, 0);
            stream.synchronize();
            assert_eq!(vec, empty);
        }
    }
}
