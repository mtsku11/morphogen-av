use crate::AudioError;

pub fn convolve_mono(input: &[f32], impulse: &[f32]) -> Result<Vec<f32>, AudioError> {
    if impulse.is_empty() {
        return Err(AudioError::InvalidSettings(
            "impulse response must contain at least one sample".to_string(),
        ));
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let output_len = input
        .len()
        .checked_add(impulse.len())
        .and_then(|len| len.checked_sub(1))
        .ok_or_else(|| {
            AudioError::InvalidSettings("convolution output is too large".to_string())
        })?;
    let mut output = vec![0.0; output_len];
    for (input_index, input_sample) in input.iter().enumerate() {
        for (impulse_index, impulse_sample) in impulse.iter().enumerate() {
            output[input_index + impulse_index] += input_sample * impulse_sample;
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_convolution_matches_tiny_fixture() {
        let output = convolve_mono(&[1.0, 2.0], &[0.5, 0.5]).expect("convolve");

        assert_eq!(output, vec![0.5, 1.5, 1.0]);
    }
}
