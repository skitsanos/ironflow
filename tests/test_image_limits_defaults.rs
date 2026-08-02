#[test]
fn image_resource_limits_have_bounded_defaults() {
    const VARIABLES: [&str; 6] = [
        "IRONFLOW_MAX_IMAGE_ENCODED_BYTES",
        "IRONFLOW_MAX_IMAGE_PIXELS",
        "IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES",
        "IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES",
        "IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES",
        "IRONFLOW_MAX_IMAGE_TO_PDF_PIXELS",
    ];
    let saved: Vec<_> = VARIABLES.iter().map(std::env::var_os).collect();
    for name in VARIABLES {
        // SAFETY: this dedicated test binary contains one test and restores all variables.
        unsafe { std::env::remove_var(name) };
    }

    assert_eq!(
        ironflow::util::limits::max_image_encoded_bytes(),
        50 * 1024 * 1024
    );
    assert_eq!(ironflow::util::limits::max_image_pixels(), 25_000_000);
    assert_eq!(
        ironflow::util::limits::max_image_decode_allocation_bytes(),
        128 * 1024 * 1024
    );
    assert_eq!(ironflow::util::limits::max_image_to_pdf_sources(), 100);
    assert_eq!(
        ironflow::util::limits::max_image_to_pdf_encoded_bytes(),
        100 * 1024 * 1024
    );
    assert_eq!(
        ironflow::util::limits::max_image_to_pdf_pixels(),
        50_000_000
    );

    for (name, value) in VARIABLES.into_iter().zip(saved) {
        // SAFETY: restore the exact process environment captured above.
        unsafe {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
