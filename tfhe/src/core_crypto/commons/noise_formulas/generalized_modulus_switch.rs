// This file was autogenerated, do not modify by hand.
use crate::core_crypto::commons::dispersion::Variance;
use crate::core_crypto::commons::parameters::*;

/// This formula is only valid when going from a larger to a smaller modulus
/// This formula is based on a heuristic, so may not always be valid
pub fn generalized_modulus_switch_additive_variance(
    input_lwe_dimension: LweDimension,
    modulus: f64,
    new_modulus: f64,
) -> Variance {
    Variance(generalized_modulus_switch_additive_variance_impl(
        input_lwe_dimension.0 as f64,
        modulus,
        new_modulus,
    ))
}

/// This formula is only valid when going from a larger to a smaller modulus
/// This formula is based on a heuristic, so may not always be valid
pub fn generalized_modulus_switch_additive_variance_impl(
    input_lwe_dimension: f64,
    modulus: f64,
    new_modulus: f64,
) -> f64 {
    0.5 * input_lwe_dimension
        * (0.0208333333333333 * modulus.powf(-2.0) + 0.0416666666666667 * new_modulus.powf(-2.0))
        - 0.0416666666666667 * modulus.powf(-2.0)
        + 0.0416666666666667 * new_modulus.powf(-2.0)
}
