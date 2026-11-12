# Third-party licenses

IronFlow is distributed under the MIT License (`LICENSE`). Its binaries and
container image statically link the Rust crates listed below, which are
redistributed under their own licenses. The bundled document fonts are covered
separately by the SIL Open Font License (`assets/fonts/OFL.txt` in the source
tree, `OFL.txt` in the release archives, `fonts-OFL.txt` in the image).

This file is generated from `Cargo.lock` by
`bun --no-env-file scripts/third_party_notices.ts write`; do not edit it by
hand. It covers every crate reachable from IronFlow through normal or build
dependencies with all Cargo features enabled (578 crates), so it also lists
crates that only the `-full` archives and the container image include. Where
a crate offers a choice of licenses, IronFlow uses the first option in this
order:

MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, MIT-0,
Unlicense, CC0-1.0, BSL-1.0, Unicode-3.0, Unicode-DFS-2016, bzip2-1.0.6,
CDLA-Permissive-2.0, MPL-2.0, Apache-2.0 WITH LLVM-exception

Each crate's own license file, including its copyright notices, is available
at the listed source.

## Crates

| Crate | Version | License | Used under | Source |
| --- | --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | MIT | https://github.com/oyvindln/adler2 |
| aes | 0.9.3 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/block-ciphers |
| ahash | 0.8.12 | MIT OR Apache-2.0 | MIT | https://github.com/tkaitchuck/ahash |
| aho-corasick | 1.1.5 | Unlicense OR MIT | MIT | https://github.com/BurntSushi/aho-corasick |
| alloc-no-stdlib | 2.0.4 | BSD-3-Clause | BSD-3-Clause | https://github.com/dropbox/rust-alloc-no-stdlib |
| alloc-stdlib | 0.2.4 | BSD-3-Clause | BSD-3-Clause | https://github.com/dropbox/rust-alloc-no-stdlib |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | MIT | https://github.com/zakarumych/allocator-api2 |
| ammonia | 4.2.4 | MIT OR Apache-2.0 | MIT | https://github.com/rust-ammonia/ammonia |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | MIT | https://github.com/nical/android_system_properties |
| anstream | 1.0.0 | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/anyhow |
| arc-swap | 1.9.3 | MIT OR Apache-2.0 | MIT | https://github.com/vorner/arc-swap |
| arcstr | 1.2.0 | Apache-2.0 OR MIT OR Zlib | MIT | https://github.com/thomcc/arcstr |
| arrayref | 0.3.9 | BSD-2-Clause | BSD-2-Clause | https://github.com/droundy/arrayref |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | MIT | https://github.com/bluss/arrayvec |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/async-lock |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/async-trait |
| atoi | 2.0.0 | MIT | MIT | https://github.com/pacman82/atoi-rs |
| atoi_simd | 0.18.1 | MIT OR Apache-2.0 | MIT | https://github.com/RoDmitry/atoi_simd |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/atomic-waker |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | MIT | https://github.com/cuviper/autocfg |
| aws-config | 1.13.2 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-credential-types | 1.3.1 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-lc-rs | 1.19.0 | ISC AND (Apache-2.0 OR ISC) | Apache-2.0, ISC | https://github.com/aws/aws-lc-rs |
| aws-lc-sys | 0.47.0 | ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0) | MIT, Apache-2.0, BSD-3-Clause, ISC | https://github.com/aws/aws-lc-rs |
| aws-runtime | 1.10.4 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-sdk-s3 | 1.159.0 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-s3vectors | 1.49.0 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-sso | 1.121.0 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-ssooidc | 1.123.0 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-sts | 1.126.0 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sigv4 | 1.6.5 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-async | 1.3.1 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-checksums | 0.65.1 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-eventstream | 0.61.6 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-http | 0.64.3 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-http-client | 1.5.2 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-json | 0.65.1 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-observability | 0.3.1 | Apache-2.0 | Apache-2.0 | https://github.com/awslabs/smithy-rs |
| aws-smithy-query | 0.64.0 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-runtime | 1.16.3 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-runtime-api | 1.21.0 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-runtime-api-macros | 1.2.0 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-schema | 1.2.0 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-types | 1.9.4 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-smithy-xml | 0.64.1 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| aws-types | 1.7.0 | Apache-2.0 | Apache-2.0 | https://github.com/smithy-lang/smithy-rs |
| axum | 0.8.9 | MIT | MIT | https://github.com/tokio-rs/axum |
| axum-core | 0.5.6 | MIT | MIT | https://github.com/tokio-rs/axum |
| backon | 1.6.0 | Apache-2.0 | Apache-2.0 | https://github.com/Xuanwo/backon |
| base16ct | 1.0.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| base64 | 0.22.1 | MIT OR Apache-2.0 | MIT | https://github.com/marshallpierce/rust-base64 |
| base64 | 0.23.1 | MIT OR Apache-2.0 | MIT | https://github.com/marshallpierce/rust-base64 |
| base64-simd | 0.8.0 | MIT | MIT | https://github.com/Nugine/simd |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | MIT | https://github.com/contain-rs/bit-set |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | MIT | https://github.com/contain-rs/bit-vec |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | MIT | https://github.com/bitflags/bitflags |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| block-padding | 0.4.2 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| borrow-or-share | 0.2.4 | MIT-0 | MIT-0 | https://github.com/yescallop/borrow-or-share |
| brotli-decompressor | 5.0.3 | BSD-3-Clause/MIT | MIT | https://github.com/dropbox/rust-brotli-decompressor |
| bstr | 1.13.1 | MIT OR Apache-2.0 | MIT | https://github.com/BurntSushi/bstr |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | MIT | https://github.com/fitzgen/bumpalo |
| bytecount | 0.6.9 | Apache-2.0/MIT | MIT | https://github.com/llogiq/bytecount |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | MIT | https://github.com/Lokathor/bytemuck |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | MIT | https://github.com/Lokathor/bytemuck |
| byteorder | 1.5.0 | Unlicense OR MIT | MIT | https://github.com/BurntSushi/byteorder |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | MIT | https://github.com/image-rs/byteorder-lite |
| bytes | 1.12.1 | MIT | MIT | https://github.com/tokio-rs/bytes |
| bytes-utils | 0.1.4 | Apache-2.0/MIT | MIT | https://github.com/vorner/bytes-utils |
| bzip2 | 0.6.1 | MIT OR Apache-2.0 | MIT | https://github.com/trifectatechfoundation/bzip2-rs |
| calamine | 0.38.0 | MIT | MIT | https://github.com/tafia/calamine |
| caseless | 0.2.2 | MIT | MIT | https://github.com/unicode-rs/rust-caseless |
| cbc | 0.2.1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/block-modes |
| cc | 1.10.0 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cc-rs |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cfg-if |
| cfg_aliases | 0.2.2 | MIT | MIT | https://github.com/katharostech/cfg_aliases |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/stream-ciphers |
| chrono | 0.4.45 | MIT OR Apache-2.0 | MIT | https://github.com/chronotope/chrono |
| chrono-tz | 0.10.4 | MIT OR Apache-2.0 | MIT | https://github.com/chronotope/chrono-tz |
| cipher | 0.5.2 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| clap | 4.6.7 | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 | MIT | https://github.com/clap-rs/clap |
| cmake | 0.1.58 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cmake-rs |
| cmov | 0.5.4 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| codepage | 0.1.3 | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/codepage |
| color_quant | 1.1.0 | MIT | MIT | https://github.com/image-rs/color_quant.git |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 | MIT | https://github.com/rust-cli/anstyle.git |
| combine | 4.6.9 | MIT | MIT | https://github.com/Marwes/combine |
| comrak | 0.59.0 | BSD-2-Clause | BSD-2-Clause | https://github.com/kivikakk/comrak |
| console_error_panic_hook | 0.1.7 | Apache-2.0/MIT | MIT | https://github.com/rustwasm/console_error_panic_hook |
| console_log | 1.1.0 | MIT/Apache-2.0 | MIT | https://github.com/iamcodemaker/console_log |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| constant_time_eq | 0.6.1 | CC0-1.0 OR MIT-0 OR Apache-2.0 | Apache-2.0 | https://codeberg.org/cesarb/constant_time_eq |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | MIT | https://github.com/servo/core-foundation-rs |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | MIT | https://github.com/servo/core-foundation-rs |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | MIT | https://github.com/servo/core-foundation-rs |
| core_detect | 1.0.0 | MIT/Apache-2.0 | MIT | https://github.com/thomcc/core_detect |
| cpubits | 0.1.1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| crc | 3.4.0 | MIT OR Apache-2.0 | MIT | https://github.com/mrhooray/crc-rs.git |
| crc-catalog | 2.5.0 | MIT OR Apache-2.0 | MIT | https://github.com/akhilles/crc-catalog.git |
| crc-fast | 1.10.0 | MIT OR Apache-2.0 | MIT | https://github.com/awesomized/crc-fast-rust |
| crc32fast | 1.5.3 | MIT OR Apache-2.0 | MIT | https://github.com/srijs/rust-crc32fast |
| cron | 0.17.0 | MIT OR Apache-2.0 | MIT | https://github.com/zslayton/cron |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 | MIT | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | MIT | https://github.com/crossbeam-rs/crossbeam |
| crunchy | 0.2.4 | MIT | MIT | https://github.com/eira-fransham/crunchy |
| crypto-bigint | 0.7.10 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/crypto-bigint |
| crypto-common | 0.1.6 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| cssparser | 0.38.0 | MPL-2.0 | MPL-2.0 | https://github.com/servo/rust-cssparser |
| csv | 1.4.0 | Unlicense/MIT | MIT | https://github.com/BurntSushi/rust-csv |
| csv-core | 0.1.13 | Unlicense/MIT | MIT | https://github.com/BurntSushi/rust-csv |
| ctutils | 0.4.7 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| data-encoding | 2.11.1 | MIT | MIT | https://github.com/ia0/data-encoding |
| debug_unsafe | 0.1.4 | MIT OR Apache-2.0 | MIT | https://github.com/RoDmitry/debug_unsafe |
| deflate64 | 0.1.12 | MIT | MIT | https://github.com/anatawa12/deflate64-rs |
| der | 0.8.3 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| deranged | 0.5.9 | MIT OR Apache-2.0 | MIT | https://github.com/jhpratt/deranged |
| digest | 0.10.7 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| digest | 0.11.3 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/traits |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | MIT | https://github.com/yaahc/displaydoc |
| dotenvy | 0.15.7 | MIT | MIT | https://github.com/allan2/dotenvy |
| dtoa | 1.0.11 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/dtoa |
| dtoa-short | 0.3.5 | MPL-2.0 | MPL-2.0 | https://github.com/upsuper/dtoa-short |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | Apache-2.0 | https://gitlab.com/kornelski/dunce |
| ecb | 0.2.1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/block-modes |
| ecdsa | 0.17.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/signatures |
| either | 1.19.0 | MIT OR Apache-2.0 | MIT | https://github.com/rayon-rs/either |
| elliptic-curve | 0.14.3 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/traits |
| email-encoding | 0.4.2 | MIT OR Apache-2.0 | MIT | https://github.com/lettre/email-encoding |
| email_address | 0.2.9 | MIT | MIT | https://github.com/johnstonskj/rust-email_address.git |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause | MIT, BSD-3-Clause | https://github.com/hsivonen/encoding_rs |
| entities | 1.0.1 | MIT | MIT | https://github.com/p-jackson/entities |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | MIT | https://github.com/indexmap-rs/equivalent |
| erased-serde | 0.4.11 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/erased-serde |
| errno | 0.3.14 | MIT OR Apache-2.0 | MIT | https://github.com/lambda-fairy/rust-errno |
| etcetera | 0.11.0 | MIT OR Apache-2.0 | MIT | https://github.com/lunacookies/etcetera |
| euclid | 0.22.14 | MIT OR Apache-2.0 | MIT | https://github.com/servo/euclid |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/event-listener |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/event-listener-strategy |
| fancy-regex | 0.19.2 | MIT | MIT | https://github.com/fancy-regex/fancy-regex |
| fast-float2 | 0.2.4 | MIT OR Apache-2.0 | MIT | https://github.com/Alexhuszagh/fast-float-rust |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/fastrand |
| fax | 0.2.7 | MIT | MIT | https://github.com/pdf-rs/fax |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/fdeflate |
| fearless_simd | 1.2.0 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/fearless_simd |
| fearless_simd_macros | 0.1.0 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/fearless_simd |
| ff | 0.14.0 | MIT/Apache-2.0 | MIT | https://github.com/zkcrypto/ff |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/cc-rs |
| finl_unicode | 1.5.0 | (MIT OR Apache-2.0) AND Unicode-DFS-2016 | MIT, Unicode-DFS-2016 | https://github.com/dahosek/finl_unicode |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/flate2-rs |
| float-cmp | 0.9.0 | MIT | MIT | https://github.com/mikedilger/float-cmp |
| fluent-uri | 0.4.1 | MIT | MIT | https://github.com/yescallop/fluent-uri-rs |
| flume | 0.12.0 | Apache-2.0/MIT | MIT | https://github.com/zesterer/flume |
| fnv | 1.0.7 | Apache-2.0 / MIT | MIT | https://github.com/servo/rust-fnv |
| foldhash | 0.2.0 | Zlib | Zlib | https://github.com/orlp/foldhash |
| font-types | 0.11.3 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| font-types | 0.12.7 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| fontique | 0.12.0 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url |
| fraction | 0.17.0 | MIT OR Apache-2.0 | MIT | https://github.com/dnsl48/fraction.git |
| fs_extra | 1.3.0 | MIT | MIT | https://github.com/webdesus/fs_extra |
| futures | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-intrusive | 0.5.0 | MIT OR Apache-2.0 | MIT | https://github.com/Matthias247/futures-intrusive |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/futures-rs |
| generic-array | 0.14.9 | MIT | MIT | https://github.com/fizyk20/generic-array.git |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/getrandom |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/getrandom |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/getrandom |
| gif | 0.14.2 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image-gif |
| group | 0.14.0 | MIT/Apache-2.0 | MIT | https://github.com/zkcrypto/group |
| h2 | 0.4.20 | MIT | MIT | https://github.com/hyperium/h2 |
| half | 2.7.1 | MIT OR Apache-2.0 | MIT | https://github.com/VoidStarKat/half-rs |
| harfrust | 0.12.0 | MIT | MIT | https://github.com/harfbuzz/harfrust |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/hashbrown |
| hashlink | 0.11.1 | MIT OR Apache-2.0 | MIT | https://github.com/djc/hashlink |
| heck | 0.5.0 | MIT OR Apache-2.0 | MIT | https://github.com/withoutboats/heck |
| hex | 0.4.3 | MIT OR Apache-2.0 | MIT | https://github.com/KokaKiwi/rust-hex |
| hkdf | 0.13.0 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/KDFs/ |
| hmac | 0.13.0 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/MACs |
| hostname | 0.4.2 | MIT | MIT | https://github.com/djc/hostname |
| htmd | 0.5.6 | Apache-2.0 | Apache-2.0 | https://github.com/letmutex/htmd |
| html5ever | 0.38.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| html5ever | 0.40.1 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| http | 1.5.0 | MIT OR Apache-2.0 | MIT | https://github.com/hyperium/http |
| http-body | 1.1.0 | MIT | MIT | https://github.com/hyperium/http-body |
| http-body-util | 0.1.5 | MIT | MIT | https://github.com/hyperium/http-body |
| http-range-header | 0.4.2 | MIT | MIT | https://github.com/MarcusGrass/parse-range-headers |
| httparse | 1.10.1 | MIT OR Apache-2.0 | MIT | https://github.com/seanmonstar/httparse |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | MIT | https://github.com/pyfisch/httpdate |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hybrid-array |
| hyper | 1.12.0 | MIT | MIT | https://github.com/hyperium/hyper |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | MIT | https://github.com/rustls/hyper-rustls |
| hyper-util | 0.1.21 | MIT | MIT | https://github.com/hyperium/hyper-util |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | MIT | https://github.com/strawlab/iana-time-zone |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | MIT | https://github.com/strawlab/iana-time-zone |
| icu_collections | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_core | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_fallback | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_fallback_data | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties_data | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_provider | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_segmenter | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_segmenter_data | 2.4.0 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| idna | 1.1.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url/ |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/idna_adapter |
| image | 0.25.10 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image-webp |
| imagesize | 0.14.0 | MIT | MIT | https://github.com/Roughsketch/imagesize |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | MIT | https://github.com/indexmap-rs/indexmap |
| inout | 0.2.5 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/utils |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | MIT | https://github.com/krisprice/ipnet |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 | MIT | https://github.com/polyfill-rs/is_terminal_polyfill |
| itertools | 0.15.0 | MIT OR Apache-2.0 | MIT | https://github.com/rust-itertools/itertools |
| itoa | 1.0.18 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/itoa |
| jni | 0.22.4 | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-rs |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-rs |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-sys |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | MIT | https://github.com/jni-rs/jni-sys |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/jobserver-rs |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys |
| jsonschema | 0.60.2 | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| jsonschema-regex | 0.60.2 | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| jsonschema-value | 0.60.2 | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| krilla | 0.8.2 | MIT OR Apache-2.0 | MIT | https://github.com/LaurenzV/krilla |
| kurbo | 0.13.1 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/kurbo |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang-nursery/lazy-static.rs |
| lettre | 0.11.23 | MIT | MIT | https://github.com/lettre/lettre |
| libbz2-rs-sys | 0.2.5 | bzip2-1.0.6 | bzip2-1.0.6 | https://github.com/trifectatechfoundation/libbzip2-rs |
| libc | 0.2.191 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/libc |
| libloading | 0.9.0 | ISC | ISC | https://github.com/nagisa/rust_libloading/ |
| libm | 0.2.16 | MIT | MIT | https://github.com/rust-lang/compiler-builtins |
| libsqlite3-sys | 0.37.0 | MIT | MIT | https://github.com/rusqlite/rusqlite |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/raw_resource_handle |
| litemap | 0.8.3 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/parking_lot |
| log | 0.4.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/log |
| lopdf | 0.47.0 | MIT | MIT | https://github.com/J-F-Liu/lopdf.git |
| lru | 0.18.5 | MIT | MIT | https://github.com/jeromefroe/lru-rs.git |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | MIT | https://github.com/Ralith/lru-slab |
| lua-src | 551.0.2 | MIT | MIT | https://github.com/mlua-rs/lua-src-rs |
| luajit-src | 210.7.4+c6ffc14 | MIT | MIT | https://github.com/mlua-rs/luajit-src-rs |
| lzma-rust2 | 0.25.0 | Apache-2.0 | Apache-2.0 | https://github.com/hasenbanck/lzma-rust2/ |
| maplit | 1.0.2 | MIT/Apache-2.0 | MIT | https://github.com/bluss/maplit |
| markup5ever | 0.38.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| markup5ever | 0.40.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| markup5ever_rcdom | 0.38.0+unofficial | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| matchers | 0.2.0 | MIT | MIT | https://github.com/hawkw/matchers |
| matchit | 0.8.4 | MIT AND BSD-3-Clause | MIT, BSD-3-Clause | https://github.com/ibraheemdev/matchit |
| maybe-owned | 0.3.4 | MIT OR Apache-2.0 | MIT | https://github.com/rustonaut/maybe-owned |
| md-5 | 0.11.0 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| memchr | 2.8.3 | Unlicense OR MIT | MIT | https://github.com/BurntSushi/memchr |
| memchr-n | 0.1.10 | MIT OR Apache-2.0 | MIT | https://github.com/Dr-Emann/memchr_n |
| memmap2 | 0.9.12 | MIT OR Apache-2.0 | MIT | https://github.com/RazrFalcon/memmap2-rs |
| micromap | 0.3.0 | MIT | MIT | https://github.com/yegor256/micromap |
| mime | 0.3.17 | MIT OR Apache-2.0 | MIT | https://github.com/hyperium/mime |
| mime_guess | 2.0.5 | MIT | MIT | https://github.com/abonander/mime_guess |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | MIT | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | MIT | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| mio | 1.2.4 | MIT | MIT | https://github.com/tokio-rs/mio |
| mlua | 0.12.2 | MIT | MIT | https://github.com/mlua-rs/mlua |
| mlua-sys | 0.13.0 | MIT | MIT | https://github.com/mlua-rs/mlua |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | Apache-2.0 | https://github.com/awxkee/moxcms.git |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/multiversion_no_op |
| new_debug_unreachable | 1.0.6 | MIT | MIT | https://github.com/mbrubeck/rust-debug-unreachable |
| nix | 0.31.3 | MIT | MIT | https://github.com/nix-rust/nix |
| nom | 8.0.0 | MIT | MIT | https://github.com/rust-bakery/nom |
| noyalib | 0.0.58 | MIT OR Apache-2.0 | MIT | https://github.com/sebastienrousseau/noyalib |
| nu-ansi-term | 0.50.3 | MIT | MIT | https://github.com/nushell/nu-ansi-term |
| num | 0.4.3 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-bigint |
| num-bigint | 0.5.1 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-bigint |
| num-cmp | 0.1.0 | MIT/Apache-2.0 | MIT | https://github.com/lifthrasiir/num-cmp |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-complex |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | MIT | https://github.com/jhpratt/num-conv |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-integer |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-iter |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-rational |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | MIT | https://github.com/rust-num/num-traits |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | MIT | https://github.com/matklad/once_cell |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 | MIT | https://github.com/polyfill-rs/once_cell_polyfill |
| openssl-probe | 0.2.2 | MIT OR Apache-2.0 | MIT | https://github.com/rustls/openssl-probe |
| ordered-float | 2.10.1 | MIT | MIT | https://github.com/reem/rust-ordered-float |
| outref | 0.5.2 | MIT | MIT | https://github.com/Nugine/outref |
| p256 | 0.14.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| parking | 2.2.1 | Apache-2.0 OR MIT | MIT | https://github.com/smol-rs/parking |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/parking_lot |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/parking_lot |
| parlance | 0.1.1 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| parley | 0.12.0 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| parley_data | 0.12.0 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| parley_emoji | 0.12.0 | MIT | MIT | https://github.com/linebender/parley |
| parley_engine | 0.12.0 | Apache-2.0 OR MIT | MIT | https://github.com/linebender/parley |
| pbkdf2 | 0.13.0 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/password-hashes |
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 | MIT | https://github.com/typst/pdf-writer |
| pdfium-render | 0.9.5 | MIT OR Apache-2.0 | MIT | https://github.com/ajrcarey/pdfium-render |
| pem-rfc7468 | 1.0.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url/ |
| phf | 0.11.3 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf | 0.12.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf | 0.13.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf | 0.14.0 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_codegen | 0.13.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_codegen | 0.14.0 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_generator | 0.11.3 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_generator | 0.13.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_generator | 0.14.0 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_macros | 0.11.3 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_macros | 0.13.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.11.3 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.12.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.13.1 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.14.0 | MIT | MIT | https://github.com/rust-phf/rust-phf |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | MIT | https://github.com/taiki-e/pin-project-lite |
| piston-float | 1.0.1 | MIT | MIT | https://github.com/pistondevelopers/float.git |
| pkcs8 | 0.11.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/pkg-config-rs |
| png | 0.18.1 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/image-png |
| polycool | 0.4.0 | MIT OR Apache-2.0 | MIT | https://github.com/linebender/kurbo |
| potential_utf | 0.1.6 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | MIT | https://github.com/jhpratt/powerfmt |
| ppmd-rust | 1.5.2 | CC0-1.0 OR MIT-0 | MIT-0 | https://github.com/hasenbanck/ppmd-rust |
| precomputed-hash | 0.1.1 | MIT | MIT | https://github.com/emilio/precomputed-hash |
| primefield | 0.14.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| primeorder | 0.14.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/proc-macro2 |
| prometheus-client | 0.25.1 | Apache-2.0 OR MIT | MIT | https://github.com/prometheus/client_rust |
| prometheus-client-derive-encode | 0.5.1 | Apache-2.0 OR MIT | MIT | https://github.com/prometheus/client_rust |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | Apache-2.0 | https://github.com/awxkee/pxfm |
| qoi | 0.4.1 | MIT/Apache-2.0 | MIT | https://github.com/aldanor/qoi-rust |
| quick-error | 2.0.1 | MIT/Apache-2.0 | MIT | http://github.com/tailhook/quick-error |
| quick-xml | 0.42.0 | MIT | MIT | https://github.com/tafia/quick-xml |
| quinn | 0.11.12 | MIT OR Apache-2.0 | MIT | https://github.com/quinn-rs/quinn |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 | MIT | https://github.com/quinn-rs/quinn |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 | MIT | https://github.com/quinn-rs/quinn |
| quote | 1.0.47 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/quote |
| quoted_printable | 0.5.2 | 0BSD | 0BSD | https://github.com/staktrace/quoted-printable |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | MIT | https://github.com/r-efi/r-efi |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | MIT | https://github.com/r-efi/r-efi |
| rand | 0.10.3 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand |
| rand | 0.8.8 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand_core |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rand |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | MIT | https://github.com/rust-random/rngs |
| rangemap | 1.8.0 | MIT/Apache-2.0 | MIT | https://github.com/jeffparsons/rangemap |
| read-fonts | 0.39.2 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| redis | 1.8.1 | BSD-3-Clause | BSD-3-Clause | https://github.com/redis-rs/redis-rs |
| redox_syscall | 0.5.18 | MIT | MIT | https://gitlab.redox-os.org/redox-os/syscall |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/ref-cast |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/ref-cast |
| referencing | 0.60.2 | MIT | MIT | https://github.com/Stranger6667/jsonschema |
| regex | 1.13.1 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| regex-lite | 0.1.9 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/regex |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | MIT | https://github.com/seanmonstar/reqwest |
| rfc6979 | 0.6.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/signatures |
| ring | 0.17.14 | Apache-2.0 AND ISC | Apache-2.0, ISC | https://github.com/briansmith/ring |
| rmcp | 3.9.0 | Apache-2.0 | Apache-2.0 | https://github.com/modelcontextprotocol/rust-sdk/ |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | MIT | https://github.com/rust-lang/rustc-hash |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | MIT | https://github.com/djc/rustc-version-rs |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | MIT | https://github.com/rustls/rustls |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | MIT | https://github.com/rustls/rustls-native-certs |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | MIT | https://github.com/rustls/pki-types |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 | MIT | https://github.com/rustls/rustls-platform-verifier |
| rustls-platform-verifier-android | 0.2.0 | MIT OR Apache-2.0 | MIT | https://github.com/rustls/rustls-platform-verifier |
| rustls-webpki | 0.103.16 | ISC | ISC | https://github.com/rustls/webpki |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/rustversion |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | Apache-2.0 | https://github.com/dtolnay/ryu |
| same-file | 1.0.6 | Unlicense/MIT | MIT | https://github.com/BurntSushi/same-file |
| schannel | 0.1.30 | MIT | MIT | https://github.com/steffengy/schannel-rs |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | MIT | https://github.com/bluss/scopeguard |
| sec1 | 0.8.1 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | MIT | https://github.com/kornelski/rust-security-framework |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | MIT | https://github.com/kornelski/rust-security-framework |
| semver | 1.0.28 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/semver |
| serde | 1.0.229 | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/serde |
| serde-value | 0.7.0 | MIT | MIT | https://github.com/arcnmx/serde-value |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/serde |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/serde |
| serde_ignored | 0.1.14 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/serde-ignored |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | MIT | https://github.com/serde-rs/json |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/path-to-error |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | MIT | https://github.com/nox/serde_urlencoded |
| serdect | 0.4.3 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| sha1 | 0.11.0 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| sha1_smol | 1.0.1 | BSD-3-Clause | BSD-3-Clause | https://github.com/mitsuhiko/sha1-smol |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | MIT | https://github.com/RustCrypto/hashes |
| sharded-slab | 0.1.7 | MIT | MIT | https://github.com/hawkw/sharded-slab |
| shlex | 2.0.1 | MIT OR Apache-2.0 | MIT | https://github.com/comex/rust-shlex |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | MIT | https://github.com/vorner/signal-hook |
| signature | 3.0.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/traits |
| simd-adler32 | 0.3.10 | MIT | MIT | https://github.com/mcountryman/simd-adler32 |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | MIT | https://github.com/seancroach/simd_cesu8 |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | MIT | https://github.com/rusticstuff/simdutf8 |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | MIT | https://github.com/jedisct1/rust-siphash |
| skrifa | 0.42.1 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| slab | 0.4.12 | MIT | MIT | https://github.com/tokio-rs/slab |
| smallvec | 1.16.4 | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-smallvec |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | MIT | https://github.com/rust-lang/socket2 |
| spin | 0.10.1 | MIT | MIT | https://github.com/mvdnes/spin-rs.git |
| spin | 0.9.9 | MIT | MIT | https://github.com/mvdnes/spin-rs.git |
| spki | 0.8.1 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/formats |
| sqlx | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-core | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-macros | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-macros-core | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-mysql | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-postgres | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sqlx-sqlite | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/launchbadge/sqlx |
| sse-stream | 0.2.6 | MIT OR Apache-2.0 | MIT | https://github.com/4t145/sse-stream/ |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | MIT | https://github.com/storyyeller/stable_deref_trait |
| streaming-iterator | 0.1.9 | MIT OR Apache-2.0 | MIT | https://github.com/sfackler/streaming-iterator |
| strict-num | 0.1.1 | MIT | MIT | https://github.com/RazrFalcon/strict-num |
| string_cache | 0.11.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/string-cache |
| string_cache | 0.9.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/string-cache |
| string_cache_codegen | 0.11.2 | MIT OR Apache-2.0 | MIT | https://github.com/servo/string-cache |
| string_cache_codegen | 0.6.1 | MIT OR Apache-2.0 | MIT | https://github.com/servo/string-cache |
| stringprep | 0.1.5 | MIT/Apache-2.0 | MIT | https://github.com/sfackler/rust-stringprep |
| strsim | 0.11.1 | MIT | MIT | https://github.com/rapidfuzz/strsim-rs |
| strum | 0.28.0 | MIT | MIT | https://github.com/Peternator7/strum |
| strum_macros | 0.28.0 | MIT | MIT | https://github.com/Peternator7/strum |
| subsetter | 0.2.6 | MIT OR Apache-2.0 | MIT | https://github.com/typst/subsetter |
| subtle | 2.6.1 | BSD-3-Clause | BSD-3-Clause | https://github.com/dalek-cryptography/subtle |
| syn | 2.0.119 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/syn |
| syn | 3.0.9 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/syn |
| sync_wrapper | 1.0.2 | Apache-2.0 | Apache-2.0 | https://github.com/Actyx/sync_wrapper |
| synstructure | 0.14.0 | MIT | MIT | https://github.com/mystor/synstructure |
| system-configuration | 0.7.0 | MIT OR Apache-2.0 | MIT | https://github.com/mullvad/system-configuration-rs |
| system-configuration-sys | 0.6.0 | MIT OR Apache-2.0 | MIT | https://github.com/mullvad/system-configuration-rs |
| tendril | 0.5.1 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/thiserror |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | MIT | https://github.com/Amanieu/thread_local-rs |
| tiff | 0.11.3 | MIT | MIT | https://github.com/image-rs/image-tiff |
| time | 0.3.55 | MIT OR Apache-2.0 | MIT | https://github.com/time-rs/time |
| time-core | 0.1.9 | MIT OR Apache-2.0 | MIT | https://github.com/time-rs/time |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | MIT | https://github.com/time-rs/time |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | BSD-3-Clause | https://github.com/linebender/tiny-skia/tree/master/path |
| tinystr | 0.8.5 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | MIT | https://github.com/Lokathor/tinyvec |
| tokio | 1.54.1 | MIT | MIT | https://github.com/tokio-rs/tokio |
| tokio-macros | 2.7.2 | MIT | MIT | https://github.com/tokio-rs/tokio |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | MIT | https://github.com/rustls/tokio-rustls |
| tokio-stream | 0.1.19 | MIT | MIT | https://github.com/tokio-rs/tokio |
| tokio-util | 0.7.20 | MIT | MIT | https://github.com/tokio-rs/tokio |
| tower | 0.5.3 | MIT | MIT | https://github.com/tower-rs/tower |
| tower-http | 0.6.11 | MIT | MIT | https://github.com/tower-rs/tower-http |
| tower-http | 0.7.1 | MIT | MIT | https://github.com/tower-rs/tower-http |
| tower-layer | 0.3.3 | MIT | MIT | https://github.com/tower-rs/tower |
| tower-service | 0.3.3 | MIT | MIT | https://github.com/tower-rs/tower |
| tracing | 0.1.44 | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-attributes | 0.1.31 | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-core | 0.1.36 | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-log | 0.2.0 | MIT | MIT | https://github.com/tokio-rs/tracing |
| tracing-subscriber | 0.3.23 | MIT | MIT | https://github.com/tokio-rs/tracing |
| tree-sitter | 0.27.1 | MIT | MIT | https://github.com/tree-sitter/tree-sitter |
| tree-sitter-language | 0.1.9 | MIT | MIT | https://github.com/tree-sitter/tree-sitter |
| tree-sitter-lua | 0.5.0 | MIT | MIT | https://github.com/tree-sitter-grammars/tree-sitter-lua |
| try-lock | 0.2.5 | MIT | MIT | https://github.com/seanmonstar/try-lock |
| typed-arena | 2.0.2 | MIT | MIT | https://github.com/SimonSapin/rust-typed-arena |
| typed-path | 0.13.0 | MIT OR Apache-2.0 | MIT | https://github.com/chipsenkbeil/typed-path |
| typeid | 1.0.3 | MIT OR Apache-2.0 | MIT | https://github.com/dtolnay/typeid |
| typenum | 1.20.1 | MIT OR Apache-2.0 | MIT | https://github.com/paholg/typenum |
| unicase | 2.10.0 | MIT OR Apache-2.0 | MIT | https://github.com/seanmonstar/unicase |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | MIT | https://github.com/servo/unicode-bidi |
| unicode-general-category | 1.1.0 | Apache-2.0 | Apache-2.0 | https://github.com/yeslogic/unicode-general-category |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT, Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT, Unicode-3.0 | https://github.com/unicode-rs/unicode-normalization |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 | MIT | https://github.com/unicode-rs/unicode-properties |
| unicode-script | 0.5.9 | MIT OR Apache-2.0 | MIT | https://github.com/unicode-rs/unicode-script |
| untrusted | 0.9.0 | ISC | ISC | https://github.com/briansmith/untrusted |
| url | 2.5.8 | MIT OR Apache-2.0 | MIT | https://github.com/servo/rust-url |
| urlencoding | 2.1.3 | MIT | MIT | https://github.com/kornelski/rust_urlencoding |
| utf16string | 0.2.0 | MIT OR Apache-2.0 | MIT | https://github.com/getsentry/utf16string |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | MIT | https://github.com/hsivonen/utf8_iter |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT | MIT | https://github.com/alacritty/vte |
| uuid | 1.29.0 | Apache-2.0 OR MIT | MIT | https://github.com/uuid-rs/uuid |
| uuid-simd | 0.8.0 | MIT | MIT | https://github.com/Nugine/simd |
| valuable | 0.1.1 | MIT | MIT | https://github.com/tokio-rs/valuable |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | MIT | https://github.com/mcgoo/vcpkg-rs |
| vecmath | 1.0.0 | MIT | MIT | https://github.com/pistondevelopers/vecmath.git |
| version_check | 0.9.5 | MIT/Apache-2.0 | MIT | https://github.com/SergioBenitez/version_check |
| vsimd | 0.8.0 | MIT | MIT | https://github.com/Nugine/simd |
| walkdir | 2.5.0 | Unlicense/MIT | MIT | https://github.com/BurntSushi/walkdir |
| want | 0.3.2 | MIT | MIT | https://github.com/seanmonstar/want |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT | https://github.com/bytecodealliance/wasi |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT | https://github.com/bytecodealliance/wasi-rs |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared |
| wasm-streams | 0.5.0 | MIT OR Apache-2.0 | MIT | https://github.com/MattiasBuelens/wasm-streams/ |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | MIT | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys |
| web-time | 1.1.0 | MIT OR Apache-2.0 | MIT | https://github.com/daxpedda/web-time |
| web_atoms | 0.2.6 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| web_atoms | 0.3.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| webpki-root-certs | 1.0.11 | CDLA-Permissive-2.0 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| webpki-roots | 1.0.11 | CDLA-Permissive-2.0 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| weezl | 0.1.12 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/weezl |
| weezl | 0.2.1 | MIT OR Apache-2.0 | MIT | https://github.com/image-rs/weezl |
| which | 8.0.6 | MIT | MIT | https://github.com/harryfei/which-rs.git |
| whoami | 2.1.3 | Apache-2.0 OR BSL-1.0 OR MIT | MIT | https://github.com/ardaku/whoami |
| winapi-util | 0.1.11 | Unlicense OR MIT | MIT | https://github.com/BurntSushi/winapi-util |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | MIT | https://github.com/microsoft/windows-rs |
| winnow | 0.7.15 | MIT | MIT | https://github.com/winnow-rs/winnow |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT | https://github.com/bytecodealliance/wit-bindgen |
| wnaf | 0.14.1 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/elliptic-curves |
| write-fonts | 0.48.1 | MIT OR Apache-2.0 | MIT | https://github.com/googlefonts/fontations |
| writeable | 0.6.4 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| xml5ever | 0.38.0 | MIT OR Apache-2.0 | MIT | https://github.com/servo/html5ever |
| xmlparser | 0.13.6 | MIT/Apache-2.0 | MIT | https://github.com/RazrFalcon/xmlparser |
| xmp-writer | 0.3.3 | MIT OR Apache-2.0 | MIT | https://github.com/typst/xmp-writer |
| xxhash-rust | 0.8.19 | BSL-1.0 | BSL-1.0 | https://github.com/DoumanAsh/xxhash-rust |
| yoke | 0.8.4 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| yoke-derive | 0.8.4 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | MIT | https://github.com/google/zerocopy |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | MIT | https://github.com/google/zerocopy |
| zerofrom | 0.1.8 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | MIT | https://github.com/RustCrypto/utils |
| zerotrie | 0.2.5 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec | 0.11.9 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec-derive | 0.11.7 | Unicode-3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zip | 9.5.1 | MIT | MIT | https://github.com/zip-rs/zip2 |
| zlib-rs | 0.6.8 | Zlib | Zlib | https://github.com/trifectatechfoundation/zlib-rs |
| zmij | 1.0.23 | MIT | MIT | https://github.com/dtolnay/zmij |
| zopfli | 0.8.4 | Apache-2.0 | Apache-2.0 | https://github.com/zopfli-rs/zopfli |
| zstd | 0.14.3 | BSD-3-Clause | BSD-3-Clause | https://github.com/gyscos/zstd-rs |
| zstd-safe | 8.2.0 | BSD-3-Clause | BSD-3-Clause | https://github.com/gyscos/zstd-rs |
| zstd-sys | 2.1.2+zstd.1.5.7 | BSD-3-Clause | BSD-3-Clause | https://github.com/gyscos/zstd-rs |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | MIT | https://github.com/etemesi254/zune-image |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | MIT | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |

## MPL-2.0 source availability

IronFlow links the following crates unmodified. Their complete source code
is available at the listed repository and from crates.io, as section 3.2 of
the Mozilla Public License 2.0 requires for executable distributions.

| Crate | Version | Source | crates.io |
| --- | --- | --- | --- |
| cssparser | 0.38.0 | https://github.com/servo/rust-cssparser | https://crates.io/crates/cssparser/0.38.0 |
| dtoa-short | 0.3.5 | https://github.com/upsuper/dtoa-short | https://crates.io/crates/dtoa-short/0.3.5 |

## License texts

Each license below applies to the crates whose "Used under" column names it.

### MIT

Used by 493 crates.

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
