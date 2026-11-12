# Third-party licenses

IronFlow is distributed under the MIT License (`LICENSE`). Its binaries and
container image statically link the Rust crates listed below, which are
redistributed under their own licenses. The bundled document fonts are covered
separately by the SIL Open Font License (`assets/fonts/OFL.txt` in the source
tree, `OFL.txt` in the release archives, `fonts-OFL.txt` in the image).

This file is generated from `Cargo.lock` by
`bun --no-env-file scripts/third_party_notices.ts write`; do not edit it by
hand. It covers every crate reachable from IronFlow through normal or build
dependencies with all Cargo features enabled (539 crates), so it also lists
crates that only the `-full` archives and the container image include. Rows
carry no version: the exact versions linked into a binary are those in the
`Cargo.lock` of the source revision it was built from. Where a crate offers
a choice of licenses, IronFlow uses the first option in this order:

MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, MIT-0,
Unlicense, CC0-1.0, BSL-1.0, Unicode-3.0, Unicode-DFS-2016, bzip2-1.0.6,
CDLA-Permissive-2.0, MPL-2.0, Apache-2.0 WITH LLVM-exception

Each crate's own license file, including its copyright notices, is available
at the listed source.

## Crates

| Crate | License | Used under | Source |
| --- | --- | --- | --- |
| adler2 | 0BSD OR MIT OR Apache-2.0 | MIT | https://github.com/oyvindln/adler2 |
| aes | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/block-ciphers |
| ahash | MIT OR Apache-2.0 | MIT | https://github.com/tkaitchuck/ahash |
| aho-corasick | Unlicense OR MIT | MIT | https://github.com/BurntSushi/aho-corasick |
| alloc-no-stdlib | BSD-3-Clause | BSD-3-Clause | https://github.com/dropbox/rust-alloc-no-stdlib |
| alloc-stdlib | BSD-3-Clause | BSD-3-Clause | https://github.com/dropbox/rust-alloc-no-stdlib |
| allocator-api2 | MIT OR Apache-2.0 | MIT | https://github.com/zakarumych/allocator-api2 |
| ammonia | MIT OR Apache-2.0 | MIT | https://github.com/rust-ammonia/ammonia |
| android_system_properties | MIT OR Apache-2.0 | MIT | https://github.com/nical/android_system_properties |
| anstream | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle-parse | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle-query | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle-wincon | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anyhow | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/anyhow |
| arc-swap | MIT OR Apache-2.0 | MIT | https://github.com/vorner/arc-swap |
| arcstr | Apache-2.0 OR MIT OR Zlib | MIT | https://github.com/thomcc/arcstr |
| arrayref | BSD-2-Clause | BSD-2-Clause | https://github.com/droundy/arrayref |
| arrayvec | MIT OR Apache-2.0 | MIT | https://github.com/bluss/arrayvec |
| async-lock | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/async-lock |
| async-trait | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/async-trait |
| atoi | MIT | MIT | https://github.com/pacman82/atoi-rs |
| atoi_simd | MIT OR Apache-2.0 | MIT | https://github.com/RoDmitry/atoi_simd |
| atomic-waker | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/atomic-waker |
| autocfg | Apache-2.0 OR MIT | MIT | https://github.com/cuviper/autocfg |
| aws-config | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-credential-types | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-lc-rs | ISC AND (Apache-2.0 OR ISC) | Apache-2.0, ISC | https://github.com/aws/aws-lc-rs |
| aws-lc-sys | ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0) | MIT, Apache-2.0, BSD-3-Clause, ISC | https://github.com/aws/aws-lc-rs |
| aws-runtime | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-sdk-s3 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-s3vectors | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-sso | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-ssooidc | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-sts | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sigv4 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-async | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-checksums | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-eventstream | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-http | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-http-client | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-json | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-observability | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/smithy-rs |
| aws-smithy-query | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-runtime | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-runtime-api | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-runtime-api-macros | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-schema | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-types | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-xml | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-types | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| axum | MIT | MIT | https://github.com/tokio-rs/axum |
| axum-core | MIT | MIT | https://github.com/tokio-rs/axum |
| backon | Apache-2.0 | Apache-2.0 | https://github.com/Xuanwo/backon |
| base16ct | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| base64 | MIT OR Apache-2.0 | MIT | https://github.com/marshallpierce/rust-base64 |
| base64-simd | MIT | MIT | https://github.com/Nugine/simd |
| base64ct | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| bit-set | Apache-2.0 OR MIT | MIT | https://github.com/contain-rs/bit-set |
| bit-vec | Apache-2.0 OR MIT | MIT | https://github.com/contain-rs/bit-vec |
| bitflags | MIT OR Apache-2.0 | MIT | https://github.com/bitflags/bitflags |
| block-buffer | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| block-padding | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| borrow-or-share | MIT-0 | MIT-0 | https://github.com/yescallop/borrow-or-share |
| brotli-decompressor | BSD-3-Clause/MIT | MIT | https://github.com/dropbox/rust-brotli-decompressor |
| bstr | MIT OR Apache-2.0 | MIT | https://github.com/BurntSushi/bstr |
| bumpalo | MIT OR Apache-2.0 | MIT | https://github.com/fitzgen/bumpalo |
| bytecount | Apache-2.0/MIT | MIT | https://github.com/llogiq/bytecount |
| bytemuck | Zlib OR Apache-2.0 OR MIT | MIT | https://github.com/Lokathor/bytemuck |
| bytemuck_derive | Zlib OR Apache-2.0 OR MIT | MIT | https://github.com/Lokathor/bytemuck |
| byteorder | Unlicense OR MIT | MIT | https://github.com/BurntSushi/byteorder |
| byteorder-lite | Unlicense OR MIT | MIT | https://github.com/image-rs/byteorder-lite |
| bytes | MIT | MIT | https://github.com/tokio-rs/bytes |
| bytes-utils | Apache-2.0/MIT | MIT | https://github.com/vorner/bytes-utils |
| bzip2 | MIT OR Apache-2.0 | MIT | https://github.com/trifectatechfoundation/bzip2-rs |
| calamine | MIT | MIT | https://github.com/tafia/calamine |
| caseless | MIT | MIT | https://github.com/unicode-rs/rust-caseless |
| cbc | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/block-modes |
| cc | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cc-rs |
| cfg-if | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cfg-if |
| cfg_aliases | MIT | MIT | https://github.com/katharostech/cfg_aliases |
| chacha20 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/stream-ciphers |
| chrono | MIT OR Apache-2.0 | MIT | https://github.com/chronotope/chrono |
| chrono-tz | MIT OR Apache-2.0 | MIT | https://github.com/chronotope/chrono-tz |
| cipher | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| clap | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| clap_builder | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| clap_derive | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| clap_lex | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| cmake | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cmake-rs |
| cmov | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| codepage | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/codepage |
| color_quant | MIT | MIT | https://github.com/image-rs/color_quant.git |
| colorchoice | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| combine | MIT | MIT | https://github.com/Marwes/combine |
| comrak | BSD-2-Clause | BSD-2-Clause | https://github.com/kivikakk/comrak |
| console_error_panic_hook | Apache-2.0/MIT | MIT | https://github.com/rustwasm/console_error_panic_hook |
| console_log | MIT/Apache-2.0 | MIT | https://github.com/iamcodemaker/console_log |
| const-oid | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| constant_time_eq | CC0-1.0 OR MIT-0 OR Apache-2.0 | Apache-2.0 | https://codeberg.org/cesarb/constant_time_eq |
| core-foundation | MIT OR Apache-2.0 | MIT | https://github.com/servo/core-foundation-rs |
| core-foundation-sys | MIT OR Apache-2.0 | MIT | https://github.com/servo/core-foundation-rs |
| core_detect | MIT/Apache-2.0 | MIT | https://github.com/thomcc/core_detect |
| cpubits | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| cpufeatures | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| crc | MIT OR Apache-2.0 | MIT | https://github.com/mrhooray/crc-rs.git |
| crc-catalog | MIT OR Apache-2.0 | MIT | https://github.com/akhilles/crc-catalog.git |
| crc-fast | MIT OR Apache-2.0 | MIT | https://github.com/awesomized/crc-fast-rust |
| crc32fast | MIT OR Apache-2.0 | MIT | https://github.com/srijs/rust-crc32fast |
| cron | MIT OR Apache-2.0 | MIT | https://github.com/zslayton/cron |
| crossbeam-queue | MIT OR Apache-2.0 | MIT | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | MIT OR Apache-2.0 | MIT | https://github.com/crossbeam-rs/crossbeam |
| crunchy | MIT | MIT | https://github.com/eira-fransham/crunchy |
| crypto-bigint | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/crypto-bigint |
| crypto-common | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| cssparser | MPL-2.0 | MPL-2.0 | https://github.com/servo/rust-cssparser |
| csv | Unlicense/MIT | MIT | https://github.com/BurntSushi/rust-csv |
| csv-core | Unlicense/MIT | MIT | https://github.com/BurntSushi/rust-csv |
| ctutils | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| data-encoding | MIT | MIT | https://github.com/ia0/data-encoding |
| debug_unsafe | MIT OR Apache-2.0 | MIT | https://github.com/RoDmitry/debug_unsafe |
| deflate64 | MIT | MIT | https://github.com/anatawa12/deflate64-rs |
| der | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| deranged | MIT OR Apache-2.0 | MIT | https://github.com/jhpratt/deranged |
| digest | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| displaydoc | MIT OR Apache-2.0 | MIT | https://github.com/yaahc/displaydoc |
| dotenvy | MIT | MIT | https://github.com/allan2/dotenvy |
| dtoa | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/dtoa |
| dtoa-short | MPL-2.0 | MPL-2.0 | https://github.com/upsuper/dtoa-short |
| dunce | CC0-1.0 OR MIT-0 OR Apache-2.0 | Apache-2.0 | https://gitlab.com/kornelski/dunce |
| ecb | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/block-modes |
| ecdsa | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/signatures |
| either | MIT OR Apache-2.0 | MIT | https://github.com/rayon-rs/either |
| elliptic-curve | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/traits |
| email-encoding | MIT OR Apache-2.0 | MIT | https://github.com/lettre/email-encoding |
| email_address | MIT | MIT | https://github.com/johnstonskj/rust-email_address.git |
| encoding_rs | (Apache-2.0 OR MIT) AND BSD-3-Clause | MIT, BSD-3-Clause | https://github.com/hsivonen/encoding_rs |
| entities | MIT | MIT | https://github.com/p-jackson/entities |
| equivalent | Apache-2.0 OR MIT | MIT | https://github.com/indexmap-rs/equivalent |
| erased-serde | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/erased-serde |
| errno | MIT OR Apache-2.0 | MIT | https://github.com/lambda-fairy/rust-errno |
| etcetera | MIT OR Apache-2.0 | MIT | https://github.com/lunacookies/etcetera |
| euclid | MIT OR Apache-2.0 | MIT | https://github.com/servo/euclid |
| event-listener | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/event-listener |
| event-listener-strategy | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/event-listener-strategy |
| fancy-regex | MIT | MIT | https://github.com/fancy-regex/fancy-regex |
| fast-float2 | MIT OR Apache-2.0 | MIT | https://github.com/Alexhuszagh/fast-float-rust |
| fastrand | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/fastrand |
| fax | MIT | MIT | https://github.com/pdf-rs/fax |
| fdeflate | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/fdeflate |
| fearless_simd | Apache-2.0 OR MIT | MIT | https://github.com/linebender/fearless_simd |
| fearless_simd_macros | Apache-2.0 OR MIT | MIT | https://github.com/linebender/fearless_simd |
| ff | MIT/Apache-2.0 | MIT | https://github.com/zkcrypto/ff |
| find-msvc-tools | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cc-rs |
| finl_unicode | (MIT OR Apache-2.0) AND Unicode-DFS-2016 | MIT, Unicode-DFS-2016 | https://github.com/dahosek/finl_unicode |
| flate2 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/flate2-rs |
| float-cmp | MIT | MIT | https://github.com/mikedilger/float-cmp |
| fluent-uri | MIT | MIT | https://github.com/yescallop/fluent-uri-rs |
| flume | Apache-2.0/MIT | MIT | https://github.com/zesterer/flume |
| fnv | Apache-2.0 / MIT | MIT | https://github.com/servo/rust-fnv |
| foldhash | Zlib | Zlib | https://github.com/orlp/foldhash |
| font-types | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| fontique | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| form_urlencoded | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url |
| fraction | MIT OR Apache-2.0 | MIT | https://github.com/dnsl48/fraction.git |
| fs_extra | MIT | MIT | https://github.com/webdesus/fs_extra |
| futures | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-channel | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-core | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-executor | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-intrusive | MIT OR Apache-2.0 | MIT | https://github.com/Matthias247/futures-intrusive |
| futures-io | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-macro | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-sink | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-task | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-util | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| generic-array | MIT | MIT | https://github.com/fizyk20/generic-array.git |
| getrandom | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/getrandom |
| gif | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image-gif |
| group | MIT/Apache-2.0 | MIT | https://github.com/zkcrypto/group |
| h2 | MIT | MIT | https://github.com/hyperium/h2 |
| half | MIT OR Apache-2.0 | MIT | https://github.com/VoidStarKat/half-rs |
| harfrust | MIT | MIT | https://github.com/harfbuzz/harfrust |
| hashbrown | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/hashbrown |
| hashlink | MIT OR Apache-2.0 | MIT | https://github.com/djc/hashlink |
| heck | MIT OR Apache-2.0 | MIT | https://github.com/withoutboats/heck |
| hex | MIT OR Apache-2.0 | MIT | https://github.com/KokaKiwi/rust-hex |
| hkdf | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/KDFs/ |
| hmac | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/MACs |
| hostname | MIT | MIT | https://github.com/djc/hostname |
| htmd | Apache-2.0 | Apache-2.0 | https://github.com/letmutex/htmd |
| html5ever | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| http | MIT OR Apache-2.0 | MIT | https://github.com/hyperium/http |
| http-body | MIT | MIT | https://github.com/hyperium/http-body |
| http-body-util | MIT | MIT | https://github.com/hyperium/http-body |
| http-range-header | MIT | MIT | https://github.com/MarcusGrass/parse-range-headers |
| httparse | MIT OR Apache-2.0 | MIT | https://github.com/seanmonstar/httparse |
| httpdate | MIT OR Apache-2.0 | MIT | https://github.com/pyfisch/httpdate |
| hybrid-array | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hybrid-array |
| hyper | MIT | MIT | https://github.com/hyperium/hyper |
| hyper-rustls | Apache-2.0 OR ISC OR MIT | MIT | https://github.com/rustls/hyper-rustls |
| hyper-util | MIT | MIT | https://github.com/hyperium/hyper-util |
| iana-time-zone | MIT OR Apache-2.0 | MIT | https://github.com/strawlab/iana-time-zone |
| iana-time-zone-haiku | MIT OR Apache-2.0 | MIT | https://github.com/strawlab/iana-time-zone |
| icu_collections | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_core | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_fallback | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_fallback_data | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer_data | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties_data | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_provider | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_segmenter | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_segmenter_data | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| idna | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url/ |
| idna_adapter | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/idna_adapter |
| image | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image |
| image-webp | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image-webp |
| imagesize | MIT | MIT | https://github.com/Roughsketch/imagesize |
| indexmap | Apache-2.0 OR MIT | MIT | https://github.com/indexmap-rs/indexmap |
| inout | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| ipnet | MIT OR Apache-2.0 | MIT | https://github.com/krisprice/ipnet |
| is_terminal_polyfill | MIT OR Apache-2.0 | MIT | https://github.com/polyfill-rs/is_terminal_polyfill |
| itertools | MIT OR Apache-2.0 | MIT | https://github.com/rust-itertools/itertools |
| itoa | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/itoa |
| jni | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-rs |
| jni-macros | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-rs |
| jni-sys | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-sys |
| jni-sys-macros | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-sys |
| jobserver | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/jobserver-rs |
| js-sys | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys |
| jsonschema | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| jsonschema-regex | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| jsonschema-value | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| krilla | MIT OR Apache-2.0 | MIT | https://github.com/LaurenzV/krilla |
| kurbo | Apache-2.0 OR MIT | MIT | https://github.com/linebender/kurbo |
| lazy_static | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang-nursery/lazy-static.rs |
| lettre | MIT | MIT | https://github.com/lettre/lettre |
| libbz2-rs-sys | bzip2-1.0.6 | bzip2-1.0.6 | https://github.com/trifectatechfoundation/libbzip2-rs |
| libc | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/libc |
| libloading | ISC | ISC | https://github.com/nagisa/rust_libloading/ |
| libsqlite3-sys | MIT | MIT | https://github.com/rusqlite/rusqlite |
| linebender_resource_handle | Apache-2.0 OR MIT | MIT | https://github.com/linebender/raw_resource_handle |
| litemap | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| lock_api | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/parking_lot |
| log | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/log |
| lopdf | MIT | MIT | https://github.com/J-F-Liu/lopdf.git |
| lru | MIT | MIT | https://github.com/jeromefroe/lru-rs.git |
| lru-slab | MIT OR Apache-2.0 OR Zlib | MIT | https://github.com/Ralith/lru-slab |
| lua-src | MIT | MIT | https://github.com/mlua-rs/lua-src-rs |
| luajit-src | MIT | MIT | https://github.com/mlua-rs/luajit-src-rs |
| lzma-rust2 | Apache-2.0 | Apache-2.0 | https://github.com/hasenbanck/lzma-rust2/ |
| maplit | MIT/Apache-2.0 | MIT | https://github.com/bluss/maplit |
| markup5ever | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| markup5ever_rcdom | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| matchers | MIT | MIT | https://github.com/hawkw/matchers |
| matchit | MIT AND BSD-3-Clause | MIT, BSD-3-Clause | https://github.com/ibraheemdev/matchit |
| maybe-owned | MIT OR Apache-2.0 | MIT | https://github.com/rustonaut/maybe-owned |
| md-5 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| memchr | Unlicense OR MIT | MIT | https://github.com/BurntSushi/memchr |
| memchr-n | MIT OR Apache-2.0 | MIT | https://github.com/Dr-Emann/memchr_n |
| memmap2 | MIT OR Apache-2.0 | MIT | https://github.com/RazrFalcon/memmap2-rs |
| micromap | MIT | MIT | https://github.com/yegor256/micromap |
| mime | MIT OR Apache-2.0 | MIT | https://github.com/hyperium/mime |
| mime_guess | MIT | MIT | https://github.com/abonander/mime_guess |
| miniz_oxide | MIT OR Zlib OR Apache-2.0 | MIT | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| mio | MIT | MIT | https://github.com/tokio-rs/mio |
| mlua | MIT | MIT | https://github.com/mlua-rs/mlua |
| mlua-sys | MIT | MIT | https://github.com/mlua-rs/mlua |
| moxcms | BSD-3-Clause OR Apache-2.0 | Apache-2.0 | https://github.com/awxkee/moxcms.git |
| multiversion_no_op | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/multiversion_no_op |
| new_debug_unreachable | MIT | MIT | https://github.com/mbrubeck/rust-debug-unreachable |
| nix | MIT | MIT | https://github.com/nix-rust/nix |
| nom | MIT | MIT | https://github.com/rust-bakery/nom |
| noyalib | MIT OR Apache-2.0 | MIT | https://github.com/sebastienrousseau/noyalib |
| nu-ansi-term | MIT | MIT | https://github.com/nushell/nu-ansi-term |
| num | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num |
| num-bigint | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-bigint |
| num-cmp | MIT/Apache-2.0 | MIT | https://github.com/lifthrasiir/num-cmp |
| num-complex | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-complex |
| num-conv | MIT OR Apache-2.0 | MIT | https://github.com/jhpratt/num-conv |
| num-integer | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-integer |
| num-iter | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-iter |
| num-rational | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-rational |
| num-traits | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-traits |
| once_cell | MIT OR Apache-2.0 | MIT | https://github.com/matklad/once_cell |
| once_cell_polyfill | MIT OR Apache-2.0 | MIT | https://github.com/polyfill-rs/once_cell_polyfill |
| openssl-probe | MIT OR Apache-2.0 | MIT | https://github.com/rustls/openssl-probe |
| ordered-float | MIT | MIT | https://github.com/reem/rust-ordered-float |
| outref | MIT | MIT | https://github.com/Nugine/outref |
| p256 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| parking | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/parking |
| parking_lot | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/parking_lot |
| parking_lot_core | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/parking_lot |
| parlance | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| parley | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| parley_data | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| parley_emoji | MIT | MIT | https://github.com/linebender/parley |
| parley_engine | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| pbkdf2 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/password-hashes |
| pdf-writer | MIT OR Apache-2.0 | MIT | https://github.com/typst/pdf-writer |
| pdfium-render | MIT OR Apache-2.0 | MIT | https://github.com/ajrcarey/pdfium-render |
| pem-rfc7468 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| percent-encoding | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url/ |
| phf | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_codegen | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_generator | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_macros | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | MIT | MIT | https://github.com/rust-phf/rust-phf |
| pin-project-lite | Apache-2.0 OR MIT | MIT | https://github.com/taiki-e/pin-project-lite |
| piston-float | MIT | MIT | https://github.com/pistondevelopers/float.git |
| pkcs8 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| pkg-config | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/pkg-config-rs |
| png | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image-png |
| polycool | MIT OR Apache-2.0 | MIT | https://github.com/linebender/kurbo |
| potential_utf | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| powerfmt | MIT OR Apache-2.0 | MIT | https://github.com/jhpratt/powerfmt |
| ppmd-rust | CC0-1.0 OR MIT-0 | MIT-0 | https://github.com/hasenbanck/ppmd-rust |
| precomputed-hash | MIT | MIT | https://github.com/emilio/precomputed-hash |
| primefield | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| primeorder | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| proc-macro2 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/proc-macro2 |
| prometheus-client | Apache-2.0 OR MIT | MIT | https://github.com/prometheus/client_rust |
| prometheus-client-derive-encode | Apache-2.0 OR MIT | MIT | https://github.com/prometheus/client_rust |
| pxfm | BSD-3-Clause OR Apache-2.0 | Apache-2.0 | https://github.com/awxkee/pxfm |
| qoi | MIT/Apache-2.0 | MIT | https://github.com/aldanor/qoi-rust |
| quick-error | MIT/Apache-2.0 | MIT | http://github.com/tailhook/quick-error |
| quick-xml | MIT | MIT | https://github.com/tafia/quick-xml |
| quinn | MIT OR Apache-2.0 | MIT | https://github.com/quinn-rs/quinn |
| quinn-proto | MIT OR Apache-2.0 | MIT | https://github.com/quinn-rs/quinn |
| quinn-udp | MIT OR Apache-2.0 | MIT | https://github.com/quinn-rs/quinn |
| quote | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/quote |
| quoted_printable | 0BSD | 0BSD | https://github.com/staktrace/quoted-printable |
| r-efi | MIT OR Apache-2.0 OR LGPL-2.1-or-later | MIT | https://github.com/r-efi/r-efi |
| rand | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand |
| rand_core | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand_core |
| rand_core | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand |
| rand_pcg | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rngs |
| rangemap | MIT/Apache-2.0 | MIT | https://github.com/jeffparsons/rangemap |
| read-fonts | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| redis | BSD-3-Clause | BSD-3-Clause | https://github.com/redis-rs/redis-rs |
| redox_syscall | MIT | MIT | https://gitlab.redox-os.org/redox-os/syscall |
| ref-cast | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/ref-cast |
| ref-cast-impl | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/ref-cast |
| referencing | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| regex | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| regex-automata | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| regex-lite | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| regex-syntax | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| reqwest | MIT OR Apache-2.0 | MIT | https://github.com/seanmonstar/reqwest |
| rfc6979 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/signatures |
| ring | Apache-2.0 AND ISC | Apache-2.0, ISC | https://github.com/briansmith/ring |
| rmcp | Apache-2.0 | Apache-2.0 | https://github.com/modelcontextprotocol/rust-sdk/ |
| rustc-hash | Apache-2.0 OR MIT | MIT | https://github.com/rust-lang/rustc-hash |
| rustc_version | MIT OR Apache-2.0 | MIT | https://github.com/djc/rustc-version-rs |
| rustls | Apache-2.0 OR ISC OR MIT | MIT | https://github.com/rustls/rustls |
| rustls-native-certs | Apache-2.0 OR ISC OR MIT | MIT | https://github.com/rustls/rustls-native-certs |
| rustls-pki-types | MIT OR Apache-2.0 | MIT | https://github.com/rustls/pki-types |
| rustls-platform-verifier | MIT OR Apache-2.0 | MIT | https://github.com/rustls/rustls-platform-verifier |
| rustls-platform-verifier-android | MIT OR Apache-2.0 | MIT | https://github.com/rustls/rustls-platform-verifier |
| rustls-webpki | ISC | ISC | https://github.com/rustls/webpki |
| rustversion | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/rustversion |
| ryu | Apache-2.0 OR BSL-1.0 | Apache-2.0 | https://github.com/dtolnay/ryu |
| same-file | Unlicense/MIT | MIT | https://github.com/BurntSushi/same-file |
| schannel | MIT | MIT | https://github.com/steffengy/schannel-rs |
| scopeguard | MIT OR Apache-2.0 | MIT | https://github.com/bluss/scopeguard |
| sec1 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| security-framework | MIT OR Apache-2.0 | MIT | https://github.com/kornelski/rust-security-framework |
| security-framework-sys | MIT OR Apache-2.0 | MIT | https://github.com/kornelski/rust-security-framework |
| semver | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/semver |
| serde | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/serde |
| serde-value | MIT | MIT | https://github.com/arcnmx/serde-value |
| serde_core | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/serde |
| serde_derive | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/serde |
| serde_ignored | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/serde-ignored |
| serde_json | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/json |
| serde_path_to_error | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/path-to-error |
| serde_urlencoded | MIT/Apache-2.0 | MIT | https://github.com/nox/serde_urlencoded |
| serdect | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| sha1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| sha1_smol | BSD-3-Clause | BSD-3-Clause | https://github.com/mitsuhiko/sha1-smol |
| sha2 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| sharded-slab | MIT | MIT | https://github.com/hawkw/sharded-slab |
| shlex | MIT OR Apache-2.0 | MIT | https://github.com/comex/rust-shlex |
| signal-hook-registry | MIT OR Apache-2.0 | MIT | https://github.com/vorner/signal-hook |
| signature | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/traits |
| simd-adler32 | MIT | MIT | https://github.com/mcountryman/simd-adler32 |
| simd_cesu8 | Apache-2.0 OR MIT | MIT | https://github.com/seancroach/simd_cesu8 |
| simdutf8 | MIT OR Apache-2.0 | MIT | https://github.com/rusticstuff/simdutf8 |
| siphasher | MIT OR Apache-2.0 | MIT | https://github.com/jedisct1/rust-siphash |
| skrifa | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| slab | MIT | MIT | https://github.com/tokio-rs/slab |
| smallvec | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-smallvec |
| socket2 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/socket2 |
| spin | MIT | MIT | https://github.com/mvdnes/spin-rs.git |
| spki | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| sqlx | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-core | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-macros | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-macros-core | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-mysql | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-postgres | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-sqlite | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sse-stream | MIT OR Apache-2.0 | MIT | https://github.com/4t145/sse-stream/ |
| stable_deref_trait | MIT OR Apache-2.0 | MIT | https://github.com/storyyeller/stable_deref_trait |
| streaming-iterator | MIT OR Apache-2.0 | MIT | https://github.com/sfackler/streaming-iterator |
| strict-num | MIT | MIT | https://github.com/RazrFalcon/strict-num |
| string_cache | MIT OR Apache-2.0 | MIT | https://github.com/servo/string-cache |
| string_cache_codegen | MIT OR Apache-2.0 | MIT | https://github.com/servo/string-cache |
| stringprep | MIT/Apache-2.0 | MIT | https://github.com/sfackler/rust-stringprep |
| strsim | MIT | MIT | https://github.com/rapidfuzz/strsim-rs |
| strum | MIT | MIT | https://github.com/Peternator7/strum |
| strum_macros | MIT | MIT | https://github.com/Peternator7/strum |
| subsetter | MIT OR Apache-2.0 | MIT | https://github.com/typst/subsetter |
| subtle | BSD-3-Clause | BSD-3-Clause | https://github.com/dalek-cryptography/subtle |
| syn | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/syn |
| sync_wrapper | Apache-2.0 | Apache-2.0 | https://github.com/Actyx/sync_wrapper |
| synstructure | MIT | MIT | https://github.com/mystor/synstructure |
| system-configuration | MIT OR Apache-2.0 | MIT | https://github.com/mullvad/system-configuration-rs |
| system-configuration-sys | MIT OR Apache-2.0 | MIT | https://github.com/mullvad/system-configuration-rs |
| tendril | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| thiserror | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/thiserror |
| thiserror-impl | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/thiserror |
| thread_local | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/thread_local-rs |
| tiff | MIT | MIT | https://github.com/image-rs/image-tiff |
| time | MIT OR Apache-2.0 | MIT | https://github.com/time-rs/time |
| time-core | MIT OR Apache-2.0 | MIT | https://github.com/time-rs/time |
| time-macros | MIT OR Apache-2.0 | MIT | https://github.com/time-rs/time |
| tiny-skia-path | BSD-3-Clause | BSD-3-Clause | https://github.com/linebender/tiny-skia/tree/master/path |
| tinystr | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| tinyvec | Zlib OR Apache-2.0 OR MIT | MIT | https://github.com/Lokathor/tinyvec |
| tokio | MIT | MIT | https://github.com/tokio-rs/tokio |
| tokio-macros | MIT | MIT | https://github.com/tokio-rs/tokio |
| tokio-rustls | MIT OR Apache-2.0 | MIT | https://github.com/rustls/tokio-rustls |
| tokio-stream | MIT | MIT | https://github.com/tokio-rs/tokio |
| tokio-util | MIT | MIT | https://github.com/tokio-rs/tokio |
| tower | MIT | MIT | https://github.com/tower-rs/tower |
| tower-http | MIT | MIT | https://github.com/tower-rs/tower-http |
| tower-layer | MIT | MIT | https://github.com/tower-rs/tower |
| tower-service | MIT | MIT | https://github.com/tower-rs/tower |
| tracing | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-attributes | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-core | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-log | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-subscriber | MIT | MIT | https://github.com/tokio-rs/tracing |
| tree-sitter | MIT | MIT | https://github.com/tree-sitter/tree-sitter |
| tree-sitter-language | MIT | MIT | https://github.com/tree-sitter/tree-sitter |
| tree-sitter-lua | MIT | MIT | https://github.com/tree-sitter-grammars/tree-sitter-lua |
| try-lock | MIT | MIT | https://github.com/seanmonstar/try-lock |
| typed-arena | MIT | MIT | https://github.com/SimonSapin/rust-typed-arena |
| typed-path | MIT OR Apache-2.0 | MIT | https://github.com/chipsenkbeil/typed-path |
| typeid | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/typeid |
| typenum | MIT OR Apache-2.0 | MIT | https://github.com/paholg/typenum |
| unicase | MIT OR Apache-2.0 | MIT | https://github.com/seanmonstar/unicase |
| unicode-bidi | MIT OR Apache-2.0 | MIT | https://github.com/servo/unicode-bidi |
| unicode-general-category | Apache-2.0 | Apache-2.0 | https://github.com/yeslogic/unicode-general-category |
| unicode-ident | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT, Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| unicode-normalization | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT, Unicode-3.0 | https://github.com/unicode-rs/unicode-normalization |
| unicode-properties | MIT/Apache-2.0 | MIT | https://github.com/unicode-rs/unicode-properties |
| unicode-script | MIT OR Apache-2.0 | MIT | https://github.com/unicode-rs/unicode-script |
| untrusted | ISC | ISC | https://github.com/briansmith/untrusted |
| url | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url |
| urlencoding | MIT | MIT | https://github.com/kornelski/rust_urlencoding |
| utf16string | MIT OR Apache-2.0 | MIT | https://github.com/getsentry/utf16string |
| utf8_iter | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/utf8_iter |
| utf8parse | Apache-2.0 OR MIT | MIT | https://github.com/alacritty/vte |
| uuid | Apache-2.0 OR MIT | MIT | https://github.com/uuid-rs/uuid |
| uuid-simd | MIT | MIT | https://github.com/Nugine/simd |
| valuable | MIT | MIT | https://github.com/tokio-rs/valuable |
| vcpkg | MIT/Apache-2.0 | MIT | https://github.com/mcgoo/vcpkg-rs |
| vecmath | MIT | MIT | https://github.com/pistondevelopers/vecmath.git |
| version_check | MIT/Apache-2.0 | MIT | https://github.com/SergioBenitez/version_check |
| vsimd | MIT | MIT | https://github.com/Nugine/simd |
| walkdir | Unlicense/MIT | MIT | https://github.com/BurntSushi/walkdir |
| want | MIT | MIT | https://github.com/seanmonstar/want |
| wasi | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT | https://github.com/bytecodealliance/wasi |
| wasip2 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT | https://github.com/bytecodealliance/wasi-rs |
| wasm-bindgen | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen |
| wasm-bindgen-futures | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures |
| wasm-bindgen-macro | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro |
| wasm-bindgen-macro-support | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support |
| wasm-bindgen-shared | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared |
| wasm-streams | MIT OR Apache-2.0 | MIT | https://github.com/MattiasBuelens/wasm-streams/ |
| web-sys | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys |
| web-time | MIT OR Apache-2.0 | MIT | https://github.com/daxpedda/web-time |
| web_atoms | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| webpki-root-certs | CDLA-Permissive-2.0 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| webpki-roots | CDLA-Permissive-2.0 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| weezl | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/weezl |
| which | MIT | MIT | https://github.com/harryfei/which-rs.git |
| whoami | Apache-2.0 OR BSL-1.0 OR MIT | MIT | https://github.com/ardaku/whoami |
| winapi-util | Unlicense OR MIT | MIT | https://github.com/BurntSushi/winapi-util |
| windows-core | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-implement | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-interface | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-link | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-registry | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-result | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-strings | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-sys | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-targets | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_aarch64_gnullvm | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_aarch64_msvc | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_i686_gnu | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_i686_gnullvm | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_i686_msvc | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnullvm | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| winnow | MIT | MIT | https://github.com/winnow-rs/winnow |
| wit-bindgen | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT | https://github.com/bytecodealliance/wit-bindgen |
| wnaf | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| write-fonts | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| writeable | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| xml5ever | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| xmlparser | MIT/Apache-2.0 | MIT | https://github.com/RazrFalcon/xmlparser |
| xmp-writer | MIT OR Apache-2.0 | MIT | https://github.com/typst/xmp-writer |
| xxhash-rust | BSL-1.0 | BSL-1.0 | https://github.com/DoumanAsh/xxhash-rust |
| yoke | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| yoke-derive | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerocopy | BSD-2-Clause OR Apache-2.0 OR MIT | MIT | https://github.com/google/zerocopy |
| zerocopy-derive | BSD-2-Clause OR Apache-2.0 OR MIT | MIT | https://github.com/google/zerocopy |
| zerofrom | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerofrom-derive | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zeroize | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| zeroize_derive | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| zerotrie | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec-derive | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zip | MIT | MIT | https://github.com/zip-rs/zip2 |
| zlib-rs | Zlib | Zlib | https://github.com/trifectatechfoundation/zlib-rs |
| zmij | MIT | MIT | https://github.com/dtolnay/zmij |
| zopfli | Apache-2.0 | Apache-2.0 | https://github.com/zopfli-rs/zopfli |
| zstd | BSD-3-Clause | BSD-3-Clause | https://github.com/gyscos/zstd-rs |
| zstd-safe | BSD-3-Clause | BSD-3-Clause | https://github.com/gyscos/zstd-rs |
| zstd-sys | BSD-3-Clause | BSD-3-Clause | https://github.com/gyscos/zstd-rs |
| zune-core | MIT OR Apache-2.0 OR Zlib | MIT | https://github.com/etemesi254/zune-image |
| zune-jpeg | MIT OR Apache-2.0 OR Zlib | MIT | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |

## MPL-2.0 source availability

IronFlow links the following crates unmodified. Their complete source code,
for the version recorded in `Cargo.lock`, is available at the listed
repository and from crates.io, as section 3.2 of the Mozilla Public License
2.0 requires for executable distributions.

| Crate | Source | crates.io |
| --- | --- | --- |
| cssparser | https://github.com/servo/rust-cssparser | https://crates.io/crates/cssparser |
| dtoa-short | https://github.com/upsuper/dtoa-short | https://crates.io/crates/dtoa-short |

## License texts

Each license below applies to the crates whose "Used under" column names it.

### MIT

Used by 454 crates.

```text
MIT License

Copyright (c) <year> <copyright holders>

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the
following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO
EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### Apache-2.0

Used by 39 crates.

```text
Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

"License" shall mean the terms and conditions for use, reproduction, and distribution as defined by Sections 1 through 9 of this document.

"Licensor" shall mean the copyright owner or entity authorized by the copyright owner that is granting the License.

"Legal Entity" shall mean the union of the acting entity and all other entities that control, are controlled by, or are under common control with that entity. For the purposes of this definition, "control" means (i) the power, direct or indirect, to cause the direction or management of such entity, whether by contract or otherwise, or (ii) ownership of fifty percent (50%) or more of the outstanding shares, or (iii) beneficial ownership of such entity.

"You" (or "Your") shall mean an individual or Legal Entity exercising permissions granted by this License.

"Source" form shall mean the preferred form for making modifications, including but not limited to software source code, documentation source, and configuration files.

"Object" form shall mean any form resulting from mechanical transformation or translation of a Source form, including but not limited to compiled object code, generated documentation, and conversions to other media types.

"Work" shall mean the work of authorship, whether in Source or Object form, made available under the License, as indicated by a copyright notice that is included in or attached to the work (an example is provided in the Appendix below).

"Derivative Works" shall mean any work, whether in Source or Object form, that is based on (or derived from) the Work and for which the editorial revisions, annotations, elaborations, or other modifications represent, as a whole, an original work of authorship. For the purposes of this License, Derivative Works shall not include works that remain separable from, or merely link (or bind by name) to the interfaces of, the Work and Derivative Works thereof.

"Contribution" shall mean any work of authorship, including the original version of the Work and any modifications or additions to that Work or Derivative Works thereof, that is intentionally submitted to Licensor for inclusion in the Work by the copyright owner or by an individual or Legal Entity authorized to submit on behalf of the copyright owner. For the purposes of this definition, "submitted" means any form of electronic, verbal, or written communication sent to the Licensor or its representatives, including but not limited to communication on electronic mailing lists, source code control systems, and issue tracking systems that are managed by, or on behalf of, the Licensor for the purpose of discussing and improving the Work, but excluding communication that is conspicuously marked or otherwise designated in writing by the copyright owner as "Not a Contribution."

"Contributor" shall mean Licensor and any individual or Legal Entity on behalf of whom a Contribution has been received by Licensor and subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable copyright license to reproduce, prepare Derivative Works of, publicly display, publicly perform, sublicense, and distribute the Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable (except as stated in this section) patent license to make, have made, use, offer to sell, sell, import, and otherwise transfer the Work, where such license applies only to those patent claims licensable by such Contributor that are necessarily infringed by their Contribution(s) alone or by combination of their Contribution(s) with the Work to which such Contribution(s) was submitted. If You institute patent litigation against any entity (including a cross-claim or counterclaim in a lawsuit) alleging that the Work or a Contribution incorporated within the Work constitutes direct or contributory patent infringement, then any patent licenses granted to You under this License for that Work shall terminate as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the Work or Derivative Works thereof in any medium, with or without modifications, and in Source or Object form, provided that You meet the following conditions:

     (a) You must give any other recipients of the Work or Derivative Works a copy of this License; and

     (b) You must cause any modified files to carry prominent notices stating that You changed the files; and

     (c) You must retain, in the Source form of any Derivative Works that You distribute, all copyright, patent, trademark, and attribution notices from the Source form of the Work, excluding those notices that do not pertain to any part of the Derivative Works; and

     (d) If the Work includes a "NOTICE" text file as part of its distribution, then any Derivative Works that You distribute must include a readable copy of the attribution notices contained within such NOTICE file, excluding those notices that do not pertain to any part of the Derivative Works, in at least one of the following places: within a NOTICE text file distributed as part of the Derivative Works; within the Source form or documentation, if provided along with the Derivative Works; or, within a display generated by the Derivative Works, if and wherever such third-party notices normally appear. The contents of the NOTICE file are for informational purposes only and do not modify the License. You may add Your own attribution notices within Derivative Works that You distribute, alongside or as an addendum to the NOTICE text from the Work, provided that such additional attribution notices cannot be construed as modifying the License.

     You may add Your own copyright statement to Your modifications and may provide additional or different license terms and conditions for use, reproduction, or distribution of Your modifications, or for any such Derivative Works as a whole, provided Your use, reproduction, and distribution of the Work otherwise complies with the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise, any Contribution intentionally submitted for inclusion in the Work by You to the Licensor shall be under the terms and conditions of this License, without any additional terms or conditions. Notwithstanding the above, nothing herein shall supersede or modify the terms of any separate license agreement you may have executed with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except as required for reasonable and customary use in describing the origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or agreed to in writing, Licensor provides the Work (and each Contributor provides its Contributions) on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied, including, without limitation, any warranties or conditions of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A PARTICULAR PURPOSE. You are solely responsible for determining the appropriateness of using or redistributing the Work and assume any risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory, whether in tort (including negligence), contract, or otherwise, unless required by applicable law (such as deliberate and grossly negligent acts) or agreed to in writing, shall any Contributor be liable to You for damages, including any direct, indirect, special, incidental, or consequential damages of any character arising as a result of this License or out of the use or inability to use the Work (including but not limited to damages for loss of goodwill, work stoppage, computer failure or malfunction, or any and all other commercial damages or losses), even if such Contributor has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing the Work or Derivative Works thereof, You may choose to offer, and charge a fee for, acceptance of support, warranty, indemnity, or other liability obligations and/or rights consistent with this License. However, in accepting such obligations, You may act only on Your own behalf and on Your sole responsibility, not on behalf of any other Contributor, and only if You agree to indemnify, defend, and hold each Contributor harmless for any liability incurred by, or claims asserted against, such Contributor by reason of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

To apply the Apache License to your work, attach the following boilerplate notice, with the fields enclosed by brackets "[]" replaced with your own identifying information. (Don't include the brackets!)  The text should be enclosed in the appropriate comment syntax for the file format. We also recommend that a file or class name and description of purpose be included on the same "printed page" as the copyright notice for easier identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

### BSD-2-Clause

Used by 2 crates.

```text
Copyright (c) <year> <owner>

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### BSD-3-Clause

Used by 12 crates.

```text
Copyright (c) <year> <owner>.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### ISC

Used by 6 crates.

```text
ISC License

Copyright <year> <owner>

Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### Zlib

Used by 2 crates.

```text
zlib License

This software is provided 'as-is', without any express or implied warranty.  In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

     1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

     2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

     3. This notice may not be removed or altered from any source distribution.
```

### 0BSD

Used by 1 crate.

```text
Copyright (C) YEAR by AUTHOR EMAIL

Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### MIT-0

Used by 2 crates.

```text
MIT No Attribution

Copyright <YEAR> <COPYRIGHT HOLDER>

Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software
without restriction, including without limitation the rights to use, copy, modify,
merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### BSL-1.0

Used by 1 crate.

```text
Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization obtaining a copy of the software and accompanying documentation covered by this license (the "Software") to use, reproduce, display, distribute, execute, and transmit the Software, and to prepare derivative works of the Software, and to permit third-parties to whom the Software is furnished to do so, all subject to the following:

The copyright notices in the Software and this entire statement, including the above license grant, this restriction and the following disclaimer, must be included in all copies of the Software, in whole or in part, and all derivative works of the Software, unless such copies or derivative works are solely in the form of machine-executable object code generated by a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### Unicode-3.0

Used by 24 crates.

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```

### Unicode-DFS-2016

Used by 1 crate.

```text
UNICODE, INC. LICENSE AGREEMENT - DATA FILES AND SOFTWARE

Unicode Data Files include all data files under the directories http://www.unicode.org/Public/, http://www.unicode.org/reports/, http://www.unicode.org/cldr/data/, http://source.icu-project.org/repos/icu/, and http://www.unicode.org/utility/trac/browser/.

Unicode Data Files do not include PDF online code charts under the directory http://www.unicode.org/Public/.

Software includes any source code published in the Unicode Standard or under the directories http://www.unicode.org/Public/, http://www.unicode.org/reports/, http://www.unicode.org/cldr/data/, http://source.icu-project.org/repos/icu/, and http://www.unicode.org/utility/trac/browser/.

NOTICE TO USER: Carefully read the following legal agreement. BY DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING UNICODE INC.'S DATA FILES ("DATA FILES"), AND/OR SOFTWARE ("SOFTWARE"), YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2016 Unicode, Inc. All rights reserved. Distributed under the Terms of Use in http://www.unicode.org/copyright.html.

Permission is hereby granted, free of charge, to any person obtaining a copy of the Unicode data files and any associated documentation (the "Data Files") or Unicode software and any associated documentation (the "Software") to deal in the Data Files or Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, and/or sell copies of the Data Files or Software, and to permit persons to whom the Data Files or Software are furnished to do so, provided that either

     (a) this copyright and permission notice appear with all copies of the Data Files or Software, or
     (b) this copyright and permission notice appear in associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall not be used in advertising or otherwise to promote the sale, use or other dealings in these Data Files or Software without prior written authorization of the copyright holder.
```

### bzip2-1.0.6

Used by 1 crate.

```text
This program, "bzip2", the associated library "libbzip2", and all documentation, are copyright (C) 1996-2010 Julian R Seward. All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

     1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

     2. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

     3. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

     4. The name of the author may not be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Julian Seward, jseward@bzip.org bzip2/libbzip2 version 1.0.6 of 6 September 2010
```

### CDLA-Permissive-2.0

Used by 2 crates.

```text
Community Data License Agreement - Permissive - Version 2.0

This is the Community Data License Agreement - Permissive, Version 2.0 (the "agreement"). Data Provider(s) and Data Recipient(s) agree as follows:

1. Provision of the Data

1.1. A Data Recipient may use, modify, and share the Data made available by Data Provider(s) under this agreement if that Data Recipient follows the terms of this agreement.

1.2. This agreement does not impose any restriction on a Data Recipient's use, modification, or sharing of any portions of the Data that are in the public domain or that may be used, modified, or shared under any other legal exception or limitation.

2. Conditions for Sharing Data

2.1. A Data Recipient may share Data, with or without modifications, so long as the Data Recipient makes available the text of this agreement with the shared Data.

3. No Restrictions on Results

3.1. This agreement does not impose any restriction or obligations with respect to the use, modification, or sharing of Results.

4. No Warranty; Limitation of Liability

4.1. All Data Recipients receive the Data subject to the following terms:

THE DATA IS PROVIDED ON AN "AS IS" BASIS, WITHOUT REPRESENTATIONS, WARRANTIES OR CONDITIONS OF ANY KIND, EITHER EXPRESS OR IMPLIED INCLUDING, WITHOUT LIMITATION, ANY WARRANTIES OR CONDITIONS OF TITLE, NON-INFRINGEMENT, MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.

NO DATA PROVIDER SHALL HAVE ANY LIABILITY FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING WITHOUT LIMITATION LOST PROFITS), HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE DATA OR RESULTS, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

5. Definitions

5.1. "Data" means the material received by a Data Recipient under this agreement.

5.2. "Data Provider" means any person who is the source of Data provided under this agreement and in reliance on a Data Recipient's agreement to its terms.

5.3. "Data Recipient" means any person who receives Data directly or indirectly from a Data Provider and agrees to the terms of this agreement.

5.4. "Results" means any outcome obtained by computational analysis of Data, including for example machine learning models and models' insights.
```

### MPL-2.0

Used by 2 crates.

```text
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.
```
