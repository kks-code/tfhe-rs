#ifndef CUDA_INTEGER_CMUX_CUH
#define CUDA_INTEGER_CMUX_CUH

#include "integer.cuh"
#include "radix_ciphertext.cuh"

template <typename Torus>
__host__ void
zero_out_if(cudaStream_t const *streams, uint32_t const *gpu_indexes,
            uint32_t gpu_count, CudaRadixCiphertextFFI *lwe_array_out,
            CudaRadixCiphertextFFI const *lwe_array_input,
            CudaRadixCiphertextFFI const *lwe_condition,
            int_zero_out_if_buffer<Torus> *mem_ptr,
            int_radix_lut<Torus> *predicate, void *const *bsks,
            Torus *const *ksks,
            CudaModulusSwitchNoiseReductionKeyFFI const *ms_noise_reduction_key,
            uint32_t num_radix_blocks) {
  if (lwe_array_out->num_radix_blocks < num_radix_blocks ||
      lwe_array_input->num_radix_blocks < num_radix_blocks)
    PANIC("Cuda error: input or output radix ciphertexts does not have enough "
          "blocks")
  if (lwe_array_out->lwe_dimension != lwe_array_input->lwe_dimension ||
      lwe_array_input->lwe_dimension != lwe_condition->lwe_dimension)
    PANIC("Cuda error: input and output radix ciphertexts must have the same "
          "lwe dimension")
  cuda_set_device(gpu_indexes[0]);
  auto params = mem_ptr->params;

  // We can't use integer_radix_apply_bivariate_lookup_table_kb since the
  // second operand is not an array
  auto tmp_lwe_array_input = mem_ptr->tmp;
  host_pack_bivariate_blocks_with_single_block<Torus>(
      streams, gpu_indexes, gpu_count, tmp_lwe_array_input,
      predicate->lwe_indexes_in, lwe_array_input, lwe_condition,
      predicate->lwe_indexes_in, params.message_modulus, num_radix_blocks);

  integer_radix_apply_univariate_lookup_table_kb<Torus>(
      streams, gpu_indexes, gpu_count, lwe_array_out, tmp_lwe_array_input, bsks,
      ksks, ms_noise_reduction_key, predicate, num_radix_blocks);
}

template <typename Torus>
__host__ void host_integer_radix_cmux_kb(
    cudaStream_t const *streams, uint32_t const *gpu_indexes,
    uint32_t gpu_count, CudaRadixCiphertextFFI *lwe_array_out,
    CudaRadixCiphertextFFI const *lwe_condition,
    CudaRadixCiphertextFFI const *lwe_array_true,
    CudaRadixCiphertextFFI const *lwe_array_false,
    int_cmux_buffer<Torus> *mem_ptr, void *const *bsks, Torus *const *ksks,
    CudaModulusSwitchNoiseReductionKeyFFI const *ms_noise_reduction_key) {

  if (lwe_array_out->num_radix_blocks != lwe_array_true->num_radix_blocks)
    PANIC("Cuda error: input and output num radix blocks must be the same")
  if (lwe_array_out->num_radix_blocks != lwe_array_false->num_radix_blocks)
    PANIC("Cuda error: input and output num radix blocks must be the same")

  auto num_radix_blocks = lwe_array_out->num_radix_blocks;
  auto params = mem_ptr->params;
  Torus lwe_size = params.big_lwe_dimension + 1;
  copy_radix_ciphertext_slice_async<Torus>(
      streams[0], gpu_indexes[0], mem_ptr->buffer_in, 0, num_radix_blocks,
      lwe_array_true, 0, num_radix_blocks);
  copy_radix_ciphertext_slice_async<Torus>(
      streams[0], gpu_indexes[0], mem_ptr->buffer_in, num_radix_blocks,
      2 * num_radix_blocks, lwe_array_false, 0, num_radix_blocks);
  for (uint i = 0; i < 2 * num_radix_blocks; i++) {
    copy_radix_ciphertext_slice_async<Torus>(streams[0], gpu_indexes[0],
                                             mem_ptr->condition_array, i, i + 1,
                                             lwe_condition, 0, 1);
  }
  integer_radix_apply_bivariate_lookup_table_kb<Torus>(
      streams, gpu_indexes, gpu_count, mem_ptr->buffer_out, mem_ptr->buffer_in,
      mem_ptr->condition_array, bsks, ksks, ms_noise_reduction_key,
      mem_ptr->predicate_lut, 2 * num_radix_blocks, params.message_modulus);

  // If the condition was true, true_ct will have kept its value and false_ct
  // will be 0 If the condition was false, true_ct will be 0 and false_ct will
  // have kept its value
  CudaRadixCiphertextFFI mem_true;
  CudaRadixCiphertextFFI mem_false;
  as_radix_ciphertext_slice<Torus>(&mem_true, mem_ptr->buffer_out, 0,
                                   num_radix_blocks);
  as_radix_ciphertext_slice<Torus>(&mem_false, mem_ptr->buffer_out,
                                   num_radix_blocks, 2 * num_radix_blocks);

  host_addition<Torus>(streams[0], gpu_indexes[0], &mem_true, &mem_true,
                       &mem_false, num_radix_blocks, params.message_modulus,
                       params.carry_modulus);

  integer_radix_apply_univariate_lookup_table_kb<Torus>(
      streams, gpu_indexes, gpu_count, lwe_array_out, &mem_true, bsks, ksks,
      ms_noise_reduction_key, mem_ptr->message_extract_lut, num_radix_blocks);
}

template <typename Torus>
__host__ uint64_t scratch_cuda_integer_radix_cmux_kb(
    cudaStream_t const *streams, uint32_t const *gpu_indexes,
    uint32_t gpu_count, int_cmux_buffer<Torus> **mem_ptr,
    std::function<Torus(Torus)> predicate_lut_f, uint32_t num_radix_blocks,
    int_radix_params params, bool allocate_gpu_memory) {
  uint64_t size_tracker = 0;
  *mem_ptr = new int_cmux_buffer<Torus>(
      streams, gpu_indexes, gpu_count, predicate_lut_f, params,
      num_radix_blocks, allocate_gpu_memory, size_tracker);
  return size_tracker;
}
#endif
