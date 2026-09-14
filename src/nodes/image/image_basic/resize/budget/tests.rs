use super::*;

fn limits(bytes: u64) -> ImageDecodeLimits {
    ImageDecodeLimits {
        max_encoded_bytes: 1024,
        max_pixels: u64::MAX,
        max_allocation_bytes: bytes,
    }
}

#[test]
fn crossed_dimensions_count_the_float_intermediate() {
    let peak = peak_bytes((100, 1), (1, 100), image::ColorType::Rgb8, 300).unwrap();
    assert_eq!(peak, 300 + 160_000 + 300 + 1024);
    assert!(
        admit(
            (100, 1),
            (Some(1), Some(100)),
            image::ColorType::Rgb8,
            300,
            limits(1024)
        )
        .is_err()
    );
    assert!(
        admit(
            (100, 1),
            (Some(1), Some(100)),
            image::ColorType::Rgb8,
            300,
            limits(peak - 1)
        )
        .is_err()
    );
    assert_eq!(
        admit(
            (100, 1),
            (Some(1), Some(100)),
            image::ColorType::Rgb8,
            300,
            limits(peak)
        )
        .unwrap(),
        (1, 100)
    );
}

#[test]
fn vertical_filter_scratch_can_dominate_the_output_pass() {
    assert_eq!(
        peak_bytes((1, 100), (100, 1), image::ColorType::Rgb8, 300).unwrap(),
        300 + 16 + 1024
    );
}

#[test]
fn same_size_copies_do_not_budget_resampling() {
    assert_eq!(
        peak_bytes((100, 1), (100, 1), image::ColorType::Rgb8, 300).unwrap(),
        600
    );
    assert!(
        admit(
            (100, 1),
            (Some(100), None),
            image::ColorType::Rgb8,
            300,
            limits(600)
        )
        .is_ok()
    );
    assert!(
        admit(
            (100, 1),
            (Some(100), None),
            image::ColorType::Rgb8,
            300,
            limits(599)
        )
        .is_err()
    );
}

#[test]
fn pixel_types_keep_a_four_channel_float_intermediate() {
    use image::ColorType::*;
    for color in [
        L8, La8, Rgb8, Rgba8, L16, La16, Rgb16, Rgba16, Rgb32F, Rgba32F,
    ] {
        let source_bytes = 8 * u64::from(color.bytes_per_pixel());
        let output_bytes = 2 * u64::from(color.bytes_per_pixel());
        assert_eq!(
            peak_bytes((4, 2), (2, 1), color, source_bytes).unwrap(),
            source_bytes + 64 + output_bytes + 32
        );
    }
}

#[test]
fn extreme_dimensions_and_totals_fail_without_allocating() {
    assert!(peak_bytes((u32::MAX, 1), (1, u32::MAX), image::ColorType::L8, 1).is_err());
    assert!(peak_bytes((1, 1), (u32::MAX, u32::MAX), image::ColorType::Rgba32F, 16).is_err());
    assert!(peak_bytes((1, 1), (1, 1), image::ColorType::L8, u64::MAX).is_err());
    assert!(peak_bytes((1, 1), (1, 2), image::ColorType::L8, u64::MAX).is_err());
}

#[test]
fn aspect_ratio_and_output_pixel_ceiling_are_preserved() {
    for requested in [(Some(2), None), (None, Some(1))] {
        assert_eq!(
            admit((4, 2), requested, image::ColorType::Rgb8, 24, limits(1024)).unwrap(),
            (2, 1)
        );
    }
    let mut limits = limits(1024);
    limits.max_pixels = 1;
    assert!(
        admit((4, 2), (Some(2), None), image::ColorType::Rgb8, 24, limits)
            .unwrap_err()
            .to_string()
            .contains("IRONFLOW_MAX_IMAGE_PIXELS")
    );
}
